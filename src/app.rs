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
use crate::keymap::{map as keymap, Action};
use crate::ports::{self, PortResult};
use crate::probe::{self, ProbeResult};
use crate::service::{Origin, Service, Status};
use crate::shutdown;
use crate::supervisor::{self, SpawnedChild, SupEvent};

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
}

impl App {
    fn new(
        cfg: Config,
        tx: mpsc::UnboundedSender<SupEvent>,
        probe_tx: mpsc::UnboundedSender<ProbeResult>,
        port_tx: mpsc::UnboundedSender<PortResult>,
    ) -> Self {
        let services = cfg.services.into_iter().map(Service::new).collect();
        Self {
            services,
            selected: 0,
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
        if let Some(s) = self.services.get_mut(idx) {
            s.status = Status::Starting;
            s.last_start = Some(Instant::now());
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
            shutdown::terminate(&mut child, Duration::from_millis(1500)).await;
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
        }
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
                if let Some(s) = self.services.get_mut(idx) {
                    s.push_log(origin, line);
                }
            }
            SupEvent::Exited { idx, code } => {
                let was_tracked = self.children.remove(&idx).is_some();
                self.stop_watchers(idx);
                if let Some(s) = self.services.get_mut(idx) {
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

    // config watcher (hot-reload). optional; no-op if no path given.
    let mut reload_rx = config_watcher(config_path.clone());

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
                term.draw(|f| crate::ui::draw(f, &app))?;
                if app.quitting {
                    break Ok(());
                }
            }
            _ = agent_tick.tick() => {
                app.tick_agents();
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
        }
    }

    // added: push + start
    for spec in &new_cfg.services {
        if !old_names.contains(&spec.name) {
            app.services.push(Service::new(spec.clone()));
            let idx = app.services.len() - 1;
            app.start_service(idx).await;
        }
    }

    if app.selected >= app.services.len() {
        app.selected = app.services.len().saturating_sub(1);
    }
    app.push_msg("[pulse] config reloaded".into());
    Ok(())
}
