use std::collections::{HashMap, VecDeque};
use std::io::Stdout;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use rand::SeedableRng;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use regex::Regex;
use tokio::sync::mpsc;

use crate::agents::{self, Mood};
use crate::config::Config;
use crate::deps;
use crate::errors::{missing_cwd_hint, port_in_use_hint};
use crate::keymap::{map as keymap, Action};
use crate::patterns;
use crate::ports::{self, PortResult};
use crate::probe::{self, ProbeResult};
use crate::service::{Origin, Service, Status};
use crate::shutdown;
use crate::supervisor::{self, SpawnedChild, SupEvent};
use crate::tap::{self, SharedRing};

/// which full-screen overlay (if any) is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Tap,
    TapDetail,
    Graph,
    Help,
}

/// status-bar message with fade-out.
#[derive(Debug, Clone)]
pub struct AgentMsg {
    pub text: String,
    pub born: Instant,
    pub ttl: Duration,
}

pub struct App {
    pub services: Vec<Service>,
    pub selected: usize,
    pub focus_logs: bool,
    pub filter_mode: bool,
    pub filter_input: String,
    pub compiled_filter: Option<Regex>,
    pub quitting: bool,
    pub messages: VecDeque<AgentMsg>,
    /// live children keyed by service index.
    children: HashMap<usize, tokio::process::Child>,
    /// probe task handles keyed by service index (to abort on stop/reload).
    probe_tasks: HashMap<usize, tokio::task::JoinHandle<()>>,
    port_tasks: HashMap<usize, tokio::task::JoinHandle<()>>,
    tx: mpsc::UnboundedSender<SupEvent>,
    probe_tx: mpsc::UnboundedSender<ProbeResult>,
    port_tx: mpsc::UnboundedSender<PortResult>,
    rng: rand::rngs::StdRng,
    pub config_path: Option<PathBuf>,
    pub overlay: Overlay,
    pub tap_rings: Vec<Option<SharedRing>>,
    pub stop_timeout: Duration,
    pub log_cap: usize,
    /// deadlines for auto-restart; `(idx, when)`. sorted; earliest first.
    pending_restarts: Vec<(usize, Instant)>,
    /// services that exited cleanly within the "auto-restart on .env change"
    /// window. kept so env-watch fires an immediate restart.
    env_watchers: HashMap<usize, Box<dyn std::any::Any + Send>>,
    env_rx: HashMap<usize, mpsc::UnboundedReceiver<()>>,
}

impl App {
    fn new(
        cfg: Config,
        tx: mpsc::UnboundedSender<SupEvent>,
        probe_tx: mpsc::UnboundedSender<ProbeResult>,
        port_tx: mpsc::UnboundedSender<PortResult>,
    ) -> Self {
        let stop_timeout = cfg
            .global
            .as_ref()
            .map(|g| g.stop_timeout_dur())
            .unwrap_or_else(|| Duration::from_millis(1500));
        let log_cap = cfg.log_buffer_size();
        let services: Vec<Service> = cfg
            .services
            .into_iter()
            .map(|s| Service::with_log_cap(s, log_cap))
            .collect();
        let tap_rings: Vec<Option<SharedRing>> = services
            .iter()
            .map(|s| s.spec.tap.as_ref().map(|_| tap::new_ring()))
            .collect();
        Self {
            services,
            selected: 0,
            overlay: Overlay::None,
            tap_rings,
            stop_timeout,
            focus_logs: false,
            filter_mode: false,
            filter_input: String::new(),
            compiled_filter: None,
            quitting: false,
            messages: VecDeque::new(),
            children: HashMap::new(),
            probe_tasks: HashMap::new(),
            port_tasks: HashMap::new(),
            tx,
            probe_tx,
            port_tx,
            rng: rand::rngs::StdRng::from_entropy(),
            config_path: None,
            log_cap,
            pending_restarts: Vec::new(),
            env_watchers: HashMap::new(),
            env_rx: HashMap::new(),
        }
    }

    fn sys_log(&mut self, idx: usize, msg: impl Into<String>) {
        if let Some(s) = self.services.get_mut(idx) {
            s.push_log(Origin::System, msg.into());
        }
    }

    async fn start_service(&mut self, idx: usize) {
        let spec = match self.services.get(idx) {
            Some(s) => s.spec.clone(),
            None => return,
        };
        if self.children.contains_key(&idx) {
            return;
        }
        // pre-flight: cwd exists?
        if let Some(cwd) = &spec.cwd {
            if !cwd.exists() {
                let hint = missing_cwd_hint(cwd);
                self.sys_log(idx, format!("error: {hint}"));
                self.push_msg(format!("[{}] {hint}", spec.name));
                if let Some(s) = self.services.get_mut(idx) {
                    s.status = Status::Crashed;
                }
                return;
            }
        }
        // pre-flight: port already bound by somebody else? warn but still try.
        if let Some(pp) = &spec.port {
            if crate::ports::listeners()
                .iter()
                .any(|e| e.port == pp.expect)
            {
                let hint = port_in_use_hint(pp.expect);
                self.sys_log(idx, format!("warn: {hint}"));
                self.push_msg(format!("[{}] {hint}", spec.name));
            }
        }
        if let Some(s) = self.services.get_mut(idx) {
            s.status = Status::Starting;
            s.last_start = Some(Instant::now());
        }
        // set up .env watcher lazily the first time we start this service
        if spec.watch_env_enabled() && !self.env_watchers.contains_key(&idx) {
            let cwd = spec.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
            if let Some((rx, guard)) = crate::envwatch::watch(&cwd) {
                self.env_watchers.insert(idx, guard);
                self.env_rx.insert(idx, rx);
            }
        }
        match supervisor::spawn_one(idx, &spec, self.tx.clone()).await {
            Ok(SpawnedChild {
                child,
                pid,
                started,
            }) => {
                if let Some(s) = self.services.get_mut(idx) {
                    s.pid = Some(pid);
                    s.started_at = Some(started);
                    s.status = Status::Running;
                }
                self.children.insert(idx, child);
                self.start_watchers(idx, &spec);
            }
            Err(e) => {
                self.sys_log(idx, format!("spawn failed: {e}"));
                if let Some(s) = self.services.get_mut(idx) {
                    s.status = Status::Crashed;
                }
            }
        }
    }

    /// spin up the proxy tap for this service if configured. idempotent-ish:
    /// repeat calls after reloads just bind a new listener if the old one died.
    pub async fn start_tap(&mut self, idx: usize) {
        let spec = match self.services.get(idx) {
            Some(s) => s.spec.clone(),
            None => return,
        };
        let Some(tspec) = spec.tap.clone() else {
            return;
        };
        let ring = match self.tap_rings.get(idx).cloned().flatten() {
            Some(r) => r,
            None => {
                let r = tap::new_ring();
                if let Some(slot) = self.tap_rings.get_mut(idx) {
                    *slot = Some(r.clone());
                }
                r
            }
        };
        if tap::Mode::parse(&tspec.mode) == tap::Mode::Passive {
            self.sys_log(
                idx,
                "tap: passive mode not implemented, use mode = \"proxy\"",
            );
            return;
        }
        let Some(listen) = tspec.listen else {
            self.sys_log(idx, "tap: no listen port configured");
            return;
        };
        let target = match tap::derive_target(
            &tspec,
            spec.port.as_ref().map(|p| p.expect),
            spec.probe.as_ref().map(|p| p.url.as_str()),
        ) {
            Some(t) => t,
            None => {
                self.sys_log(idx, "tap: can't derive target port");
                return;
            }
        };
        match tap::run_proxy(listen, target, ring).await {
            Ok(bound) => {
                self.sys_log(idx, format!("tap: proxy :{bound} -> :{target}"));
            }
            Err(e) => {
                self.sys_log(idx, format!("tap: bind :{listen} failed: {e}"));
            }
        }
    }

    fn start_watchers(&mut self, idx: usize, spec: &crate::config::ServiceSpec) {
        // probe loop
        if let Some(p) = &spec.probe {
            let url = p.url.clone();
            let interval = spec.probe_interval().unwrap_or(Duration::from_secs(5));
            let timeout = spec.probe_timeout().unwrap_or(Duration::from_secs(2));
            let expect = p.expect_status;
            let tx = self.probe_tx.clone();
            let handle = tokio::spawn(async move {
                probe::run(idx, url, interval, timeout, expect, tx).await;
            });
            if let Some(old) = self.probe_tasks.insert(idx, handle) {
                old.abort();
            }
        }
        // port loop
        if let Some(pp) = &spec.port {
            let port = pp.expect;
            let tx = self.port_tx.clone();
            let handle = tokio::spawn(async move {
                ports::run(idx, port, tx).await;
            });
            if let Some(old) = self.port_tasks.insert(idx, handle) {
                old.abort();
            }
        }
    }

    fn stop_watchers(&mut self, idx: usize) {
        if let Some(h) = self.probe_tasks.remove(&idx) {
            h.abort();
        }
        if let Some(h) = self.port_tasks.remove(&idx) {
            h.abort();
        }
    }

    async fn stop_service(&mut self, idx: usize) {
        self.stop_watchers(idx);
        if let Some(mut child) = self.children.remove(&idx) {
            shutdown::terminate(&mut child, self.stop_timeout).await;
        }
        if let Some(s) = self.services.get_mut(idx) {
            s.status = Status::Stopped;
            s.started_at = None;
            s.pid = None;
        }
    }

    async fn restart_one(&mut self, idx: usize) {
        self.stop_service(idx).await;
        if let Some(s) = self.services.get_mut(idx) {
            s.restart_count = s.restart_count.saturating_add(1);
            // manual restart wipes the auto-restart streak + unhealthy flag
            s.crash_streak = 0;
            s.unhealthy = false;
        }
        // drop any auto-restart we were about to do for this service
        self.pending_restarts.retain(|(i, _)| *i != idx);
        self.start_service(idx).await;
    }

    async fn restart_service(&mut self, idx: usize) {
        self.restart_one(idx).await;
        // dependents that want to come back up cleanly
        let name = self.services.get(idx).map(|s| s.spec.name.clone());
        if let Some(n) = name {
            let specs: Vec<_> = self.services.iter().map(|s| s.spec.clone()).collect();
            let dep_indices = deps::dependents_of(&specs, &n);
            if !dep_indices.is_empty() {
                // 1s grace so the parent finishes booting first
                tokio::time::sleep(Duration::from_secs(1)).await;
                for di in dep_indices {
                    // only bounce ones we were already running
                    if self
                        .services
                        .get(di)
                        .map(|s| s.pid.is_some())
                        .unwrap_or(false)
                        || self.children.contains_key(&di)
                    {
                        self.restart_one(di).await;
                    }
                }
            }
        }
    }

    async fn stop_all(&mut self) {
        let idxs: Vec<usize> = self.children.keys().copied().collect();
        for i in idxs {
            self.stop_service(i).await;
        }
    }

    fn apply_filter(&mut self) {
        if self.filter_input.is_empty() {
            self.compiled_filter = None;
        } else {
            self.compiled_filter = Regex::new(&self.filter_input).ok();
        }
    }

    fn push_msg(&mut self, text: String) {
        self.messages.push_back(AgentMsg {
            text,
            born: Instant::now(),
            ttl: Duration::from_secs(5),
        });
        while self.messages.len() > 4 {
            self.messages.pop_front();
        }
    }

    fn scan_for_patterns(&mut self, idx: usize, line: &str) {
        let Some((key, snippet)) = patterns::scan(line) else {
            return;
        };
        let Some(svc) = self.services.get_mut(idx) else {
            return;
        };
        if !patterns::may_fire(svc, key) {
            return;
        }
        svc.unhealthy = true;
        // nudge the agent straight to Alert mood + emit a species-flavored msg
        let name = svc.spec.name.clone();
        let tpl = svc
            .agent
            .as_ref()
            .map(|a| patterns::alert_template(a.species));
        if let Some(agent) = svc.agent.as_mut() {
            agent.mood = crate::agents::Mood::Alert;
            agent.last_mood_change = Instant::now();
        }
        if let Some(tpl) = tpl {
            let msg = tpl.replace("{name}", &name).replace("{line}", &snippet);
            self.push_msg(format!("[{name}] {msg}"));
        } else {
            self.push_msg(format!("[{name}] log match: {snippet}"));
        }
    }

    fn schedule_auto_restart(&mut self, idx: usize, quick_crash: bool) {
        let streak = if let Some(s) = self.services.get_mut(idx) {
            if quick_crash {
                s.crash_streak = s.crash_streak.saturating_add(1);
            } else {
                s.crash_streak = 1;
            }
            s.crash_streak
        } else {
            return;
        };
        if streak >= supervisor::CRASH_GIVE_UP {
            if let Some(s) = self.services.get_mut(idx) {
                s.status = Status::CrashedTooMany;
                let name = s.spec.name.clone();
                self.push_msg(format!(
                    "[{name}] crashed {streak}x, auto-restart giving up. press r to retry."
                ));
            }
            return;
        }
        let delay = supervisor::crash_backoff(streak - 1);
        let when = Instant::now() + delay;
        self.pending_restarts.push((idx, when));
        self.sys_log(
            idx,
            format!("auto-restart in {}s (streak {streak})", delay.as_secs()),
        );
    }

    async fn run_due_restarts(&mut self) {
        let now = Instant::now();
        let mut ready: Vec<usize> = Vec::new();
        self.pending_restarts.retain(|(idx, when)| {
            if *when <= now {
                ready.push(*idx);
                false
            } else {
                true
            }
        });
        for idx in ready {
            // skip if user already restarted or stopped it
            if self.children.contains_key(&idx) {
                continue;
            }
            if let Some(s) = self.services.get(idx) {
                if matches!(s.status, Status::CrashedTooMany | Status::Stopped) {
                    continue;
                }
            }
            self.start_service(idx).await;
        }
    }

    /// reset the crash streak if a service has been up healthily for 30s.
    fn reset_healthy_streaks(&mut self) {
        for s in self.services.iter_mut() {
            if matches!(s.status, Status::Running)
                && s.crash_streak > 0
                && s.started_at
                    .map(|t| t.elapsed() >= supervisor::HEALTHY_WINDOW)
                    .unwrap_or(false)
            {
                s.crash_streak = 0;
            }
        }
    }

    fn prune_messages(&mut self) {
        let now = Instant::now();
        self.messages.retain(|m| now.duration_since(m.born) < m.ttl);
    }

    fn handle_sup(&mut self, ev: SupEvent) {
        match ev {
            SupEvent::Started { idx, pid } => {
                self.sys_log(idx, format!("started pid {pid}"));
            }
            SupEvent::Log { idx, origin, line } => {
                let snippet = line.clone();
                if let Some(s) = self.services.get_mut(idx) {
                    s.push_log(origin, line);
                }
                // error-pattern scan — only on stdout/stderr, not system lines
                if matches!(origin, Origin::Stdout | Origin::Stderr) {
                    self.scan_for_patterns(idx, &snippet);
                }
            }
            SupEvent::Exited { idx, code } => {
                let was_tracked = self.children.remove(&idx).is_some();
                self.stop_watchers(idx);
                let mut quick_crash = false;
                let mut spec_clone = None;
                if let Some(s) = self.services.get_mut(idx) {
                    quick_crash = supervisor::is_quick_crash(s.last_start);
                    s.started_at = None;
                    s.pid = None;
                    s.status = if was_tracked {
                        Status::Crashed
                    } else {
                        Status::Stopped
                    };
                    s.push_log(
                        Origin::System,
                        match code {
                            Some(c) => format!("exited with code {c}"),
                            None => "exited (killed)".into(),
                        },
                    );
                    spec_clone = Some(s.spec.clone());
                }
                if let Some(spec) = spec_clone {
                    if was_tracked && spec.auto_restart_enabled() {
                        self.schedule_auto_restart(idx, quick_crash);
                    }
                }
            }
            SupEvent::SpawnError { idx, msg } => {
                self.sys_log(idx, format!("error: {msg}"));
                if let Some(s) = self.services.get_mut(idx) {
                    s.status = Status::Crashed;
                }
            }
        }
    }

    fn handle_probe(&mut self, r: ProbeResult) {
        if let Some(s) = self.services.get_mut(r.idx) {
            s.probe.record(&r);
        }
    }

    fn handle_port(&mut self, r: PortResult) {
        if let Some(s) = self.services.get_mut(r.idx) {
            s.port.record(&r);
        }
    }

    /// refresh agents + push any transition lines into the status bar.
    fn tick_agents(&mut self) {
        let mut emits: Vec<(usize, Mood)> = Vec::new();
        for (i, svc) in self.services.iter_mut().enumerate() {
            let Some(agent) = svc.agent.as_mut() else {
                continue;
            };
            let slow = svc
                .probe
                .last_latency
                .map(|d| d > Duration::from_millis(1500))
                .unwrap_or(false);
            let idle = svc.last_activity.map(|t| t.elapsed());
            // no real req/s meter yet; use 2 consecutive good fast probes under 50ms as a tiny proxy
            let spike = svc
                .probe
                .last_latency
                .map(|d| d < Duration::from_millis(30))
                .unwrap_or(false)
                && svc.probe.consecutive_fails == 0
                && svc
                    .last_activity
                    .map(|t| t.elapsed() < Duration::from_secs(2))
                    .unwrap_or(false);
            let emitted =
                agent.update_mood(svc.status, svc.probe.consecutive_fails, slow, idle, spike);
            if let Some(m) = emitted {
                emits.push((i, m));
            }
        }
        for (i, mood) in emits {
            let name = self.services[i].spec.name.clone();
            let line_tpl = {
                let agent = self.services[i].agent.as_mut().unwrap();
                agent.speak(mood, &mut self.rng)
            };
            let line = agents::format_line(&line_tpl, &name);
            self.push_msg(format!("[{name}] {line}"));
        }
    }
}

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(term: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    crossterm::execute!(
        term.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    term.show_cursor()?;
    Ok(())
}

pub async fn run(cfg: Config) -> Result<()> {
    run_with_path(cfg, None).await
}

pub async fn run_with_path(cfg: Config, config_path: Option<PathBuf>) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (probe_tx, mut probe_rx) = mpsc::unbounded_channel();
    let (port_tx, mut port_rx) = mpsc::unbounded_channel();
    let mut app = App::new(cfg, tx, probe_tx, port_tx);
    app.config_path = config_path.clone();

    // set up terminal BEFORE spawning children. if this fails (e.g. no tty),
    // we bail before anything becomes reachable.
    let mut term = match setup_terminal() {
        Ok(t) => t,
        Err(e) => return Err(e),
    };

    // start in topo order so deps come up first.
    let specs: Vec<_> = app.services.iter().map(|s| s.spec.clone()).collect();
    let order = deps::topo_order(&specs);
    let idx_of: HashMap<String, usize> = app
        .services
        .iter()
        .enumerate()
        .map(|(i, s)| (s.spec.name.clone(), i))
        .collect();
    for name in &order {
        if let Some(&idx) = idx_of.get(name) {
            app.start_service(idx).await;
        }
    }
    // fall back for anything topo missed (shouldn't happen unless graph issue)
    for idx in 0..app.services.len() {
        if !app.children.contains_key(&idx) && app.services[idx].status == Status::Stopped {
            app.start_service(idx).await;
        }
    }
    // taps last — so the underlying service has a chance to bind first.
    for idx in 0..app.services.len() {
        app.start_tap(idx).await;
    }

    // config watcher (hot-reload). optional; no-op if no path given.
    let mut reload_rx = config_watcher(config_path.clone());

    // ipc socket for `pulse share` to grab live state.
    let (ipc_tx, mut ipc_rx) = mpsc::unbounded_channel::<tokio::sync::oneshot::Sender<String>>();
    let _ipc_guard = spawn_ipc_server(ipc_tx);

    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(150));
    let mut agent_tick = tokio::time::interval(Duration::from_secs(1));

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;

    let res: Result<()> = loop {
        tokio::select! {
            _ = sigterm.recv() => { app.quitting = true; }
            _ = sighup.recv() => { app.quitting = true; }
            _ = tick.tick() => {
                reap_exited(&mut app);
                app.prune_messages();
                app.run_due_restarts().await;
                app.reset_healthy_streaks();
                app.poll_env_watchers().await;
                term.draw(|f| crate::ui::draw(f, &app))?;
                if app.quitting {
                    break Ok(());
                }
            }
            _ = agent_tick.tick() => {
                app.tick_agents();
            }
            Some(reply) = ipc_rx.recv() => {
                // someone ran `pulse share` externally. produce live html, hand
                // back via the oneshot.
                let html = render_live_snapshot(&app);
                let _ = reply.send(html);
            }
            Some(Ok(ev)) = events.next() => {
                if let Event::Key(key) = ev {
                    if key.kind != KeyEventKind::Press { continue; }
                    let action = keymap(key, app.filter_mode);
                    match action {
                        Action::Quit => { app.quitting = true; }
                        Action::NavDown => {
                            if !app.services.is_empty() {
                                app.selected = (app.selected + 1) % app.services.len();
                            }
                        }
                        Action::NavUp => {
                            if !app.services.is_empty() {
                                if app.selected == 0 {
                                    app.selected = app.services.len() - 1;
                                } else {
                                    app.selected -= 1;
                                }
                            }
                        }
                        Action::Restart => {
                            let idx = app.selected;
                            app.restart_service(idx).await;
                        }
                        Action::Stop => {
                            let idx = app.selected;
                            app.stop_service(idx).await;
                        }
                        Action::StopAll => {
                            app.stop_all().await;
                        }
                        Action::ToggleFocus => {
                            app.focus_logs = !app.focus_logs;
                        }
                        Action::StartFilter => {
                            app.filter_mode = true;
                        }
                        Action::ClearLogs => {
                            let idx = app.selected;
                            if let Some(s) = app.services.get_mut(idx) {
                                s.clear_logs();
                            }
                        }
                        Action::FilterChar(c) => { app.filter_input.push(c); }
                        Action::FilterBackspace => { app.filter_input.pop(); }
                        Action::FilterSubmit => {
                            app.apply_filter();
                            app.filter_mode = false;
                        }
                        Action::FilterCancel => {
                            app.filter_mode = false;
                            app.filter_input.clear();
                            app.compiled_filter = None;
                        }
                        Action::ToggleTap => {
                            app.overlay = if matches!(app.overlay, Overlay::Tap) {
                                Overlay::None
                            } else {
                                Overlay::Tap
                            };
                        }
                        Action::ToggleTapDetail => {
                            app.overlay = if matches!(app.overlay, Overlay::TapDetail) {
                                Overlay::None
                            } else {
                                Overlay::TapDetail
                            };
                        }
                        Action::ToggleGraph => {
                            app.overlay = if matches!(app.overlay, Overlay::Graph) {
                                Overlay::None
                            } else {
                                Overlay::Graph
                            };
                        }
                        Action::ToggleHelp => {
                            app.overlay = if matches!(app.overlay, Overlay::None) {
                                Overlay::Help
                            } else {
                                Overlay::None
                            };
                        }
                        Action::ScrollLogsUp => {
                            let idx = app.selected;
                            if let Some(s) = app.services.get_mut(idx) {
                                let page = 10usize;
                                let total = s.logs.len();
                                s.log_scroll = (s.log_scroll + page).min(total.saturating_sub(1));
                            }
                        }
                        Action::ScrollLogsDown => {
                            let idx = app.selected;
                            if let Some(s) = app.services.get_mut(idx) {
                                let page = 10usize;
                                s.log_scroll = s.log_scroll.saturating_sub(page);
                            }
                        }
                        Action::ScrollLogsTop => {
                            let idx = app.selected;
                            if let Some(s) = app.services.get_mut(idx) {
                                if !s.logs.is_empty() {
                                    s.log_scroll = s.logs.len() - 1;
                                }
                            }
                        }
                        Action::ScrollLogsBottom => {
                            let idx = app.selected;
                            if let Some(s) = app.services.get_mut(idx) {
                                s.log_scroll = 0;
                            }
                        }
                        Action::ShareNow => {
                            match write_live_snapshot(&app) {
                                Ok(path) => {
                                    app.push_msg(format!("[pulse] wrote {}", path.display()));
                                }
                                Err(e) => {
                                    app.push_msg(format!("[pulse] share failed: {e}"));
                                }
                            }
                        }
                        Action::None => {}
                    }
                    term.draw(|f| crate::ui::draw(f, &app))?;
                }
            }
            Some(ev) = rx.recv() => {
                app.handle_sup(ev);
            }
            Some(ev) = probe_rx.recv() => {
                app.handle_probe(ev);
            }
            Some(ev) = port_rx.recv() => {
                app.handle_port(ev);
            }
            Some(()) = async { reload_rx.as_mut()?.recv().await }, if reload_rx.is_some() => {
                if let Err(e) = reload_config(&mut app).await {
                    app.push_msg(format!("[pulse] reload failed: {e}"));
                }
            }
        }
    };

    app.stop_all().await;
    restore_terminal(&mut term)?;
    res
}

/// poll children for exit without blocking the event loop.
fn reap_exited(app: &mut App) {
    let keys: Vec<usize> = app.children.keys().copied().collect();
    for idx in keys {
        let maybe_status = if let Some(ch) = app.children.get_mut(&idx) {
            ch.try_wait().ok().flatten()
        } else {
            None
        };
        if let Some(status) = maybe_status {
            app.children.remove(&idx);
            app.stop_watchers(idx);
            if let Some(s) = app.services.get_mut(idx) {
                s.started_at = None;
                s.pid = None;
                s.status = Status::Crashed;
                s.push_log(
                    Origin::System,
                    match status.code() {
                        Some(c) => format!("exited with code {c}"),
                        None => "exited (signal)".into(),
                    },
                );
            }
        }
    }
}

/// set up a notify-backed watcher on the config file. returns a receiver that
/// fires (debounced-ish) whenever the file changes.
fn config_watcher(path: Option<PathBuf>) -> Option<mpsc::UnboundedReceiver<()>> {
    use notify::{Event as NotifyEvent, EventKind, RecursiveMode, Watcher};
    let path = path?;
    let parent = path.parent()?.to_path_buf();
    let (tx, rx) = mpsc::unbounded_channel();
    let target = path.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<NotifyEvent, _>| {
        if let Ok(ev) = res {
            if matches!(
                ev.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) && ev.paths.iter().any(|p| p == &target)
            {
                let _ = tx.send(());
            }
        }
    })
    .ok()?;
    watcher.watch(&parent, RecursiveMode::NonRecursive).ok()?;
    // leak the watcher so it lives for the process; alternative is a struct holder.
    Box::leak(Box::new(watcher));
    Some(rx)
}

async fn reload_config(app: &mut App) -> Result<()> {
    let Some(p) = app.config_path.clone() else {
        return Ok(());
    };
    // short debounce
    tokio::time::sleep(Duration::from_millis(150)).await;
    let new_cfg = crate::config::load(&p)?;
    // diff by name
    let old_names: std::collections::HashSet<String> =
        app.services.iter().map(|s| s.spec.name.clone()).collect();
    let new_names: std::collections::HashSet<String> =
        new_cfg.services.iter().map(|s| s.name.clone()).collect();

    // removed: stop + drop
    let removed: Vec<String> = old_names.difference(&new_names).cloned().collect();
    for rname in &removed {
        if let Some(idx) = app.services.iter().position(|s| s.spec.name == *rname) {
            app.stop_service(idx).await;
            app.services.remove(idx);
            if idx < app.tap_rings.len() {
                app.tap_rings.remove(idx);
            }
        }
    }

    // added: push + start
    for spec in &new_cfg.services {
        if !old_names.contains(&spec.name) {
            let ring_slot = spec.tap.as_ref().map(|_| tap::new_ring());
            app.services.push(Service::new(spec.clone()));
            app.tap_rings.push(ring_slot);
            let idx = app.services.len() - 1;
            app.start_service(idx).await;
            app.start_tap(idx).await;
        }
    }

    if app.selected >= app.services.len() {
        app.selected = app.services.len().saturating_sub(1);
    }
    app.push_msg("[pulse] config reloaded".into());
    Ok(())
}

impl App {
    /// drain any .env-change events queued by the per-service watchers and
    /// restart the affected services with a short debounce.
    async fn poll_env_watchers(&mut self) {
        let idxs: Vec<usize> = self.env_rx.keys().copied().collect();
        let mut to_restart: Vec<usize> = Vec::new();
        for idx in idxs {
            let mut fired = false;
            if let Some(rx) = self.env_rx.get_mut(&idx) {
                while let Ok(()) = rx.try_recv() {
                    fired = true;
                }
            }
            if fired {
                to_restart.push(idx);
            }
        }
        for idx in to_restart {
            if let Some(s) = self.services.get_mut(idx) {
                s.push_log(Origin::System, "[pulse] .env changed, restarting".into());
            }
            self.restart_one(idx).await;
        }
    }
}

fn render_live_snapshot(app: &App) -> String {
    let snap = crate::share::collect(&app.services, &app.tap_rings);
    crate::share::render(&snap)
}

fn write_live_snapshot(app: &App) -> std::io::Result<PathBuf> {
    let html = render_live_snapshot(app);
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let out = PathBuf::from(format!("pulse-snapshot-{ts}.html"));
    std::fs::write(&out, html)?;
    Ok(out)
}

/// listen on a unix socket for incoming dump requests. each request dials us
/// in, we forward it to the app loop via the provided channel, wait for the
/// rendered html, write it back.
fn spawn_ipc_server(
    tx: mpsc::UnboundedSender<tokio::sync::oneshot::Sender<String>>,
) -> Option<IpcGuard> {
    let path = crate::ipc::socket_path_for(std::process::id());
    // clean up any stale socket for this pid (shouldn't exist but be safe)
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::UnixListener::bind(&path).ok()?;
    let path_clone = path.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let tx = tx.clone();
            tokio::spawn(async move {
                let (oneshot_tx, oneshot_rx) = tokio::sync::oneshot::channel();
                if tx.send(oneshot_tx).is_err() {
                    return;
                }
                if let Ok(html) = oneshot_rx.await {
                    use tokio::io::AsyncWriteExt;
                    let _ = sock.write_all(html.as_bytes()).await;
                    let _ = sock.shutdown().await;
                }
            });
        }
    });
    Some(IpcGuard { path: path_clone })
}

/// RAII-ish guard that deletes the socket file on drop.
pub struct IpcGuard {
    path: PathBuf,
}

impl Drop for IpcGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
