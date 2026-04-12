use std::collections::HashMap;
use std::io::Stdout;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use regex::Regex;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::keymap::{map as keymap, Action};
use crate::service::{Origin, Service, Status};
use crate::shutdown;
use crate::supervisor::{self, SpawnedChild, SupEvent};

pub struct App {
    pub services: Vec<Service>,
    pub selected: usize,
    pub focus_logs: bool,
    pub filter_mode: bool,
    pub filter_input: String,
    pub compiled_filter: Option<Regex>,
    pub quitting: bool,
    /// live children keyed by service index.
    children: HashMap<usize, tokio::process::Child>,
    tx: mpsc::UnboundedSender<SupEvent>,
}

impl App {
    fn new(cfg: Config, tx: mpsc::UnboundedSender<SupEvent>) -> Self {
        let services = cfg.services.into_iter().map(Service::new).collect();
        Self {
            services,
            selected: 0,
            focus_logs: false,
            filter_mode: false,
            filter_input: String::new(),
            compiled_filter: None,
            quitting: false,
            children: HashMap::new(),
            tx,
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
            }
            Err(e) => {
                self.sys_log(idx, format!("spawn failed: {e}"));
                if let Some(s) = self.services.get_mut(idx) {
                    s.status = Status::Crashed;
                }
            }
        }
    }

    async fn stop_service(&mut self, idx: usize) {
        if let Some(mut child) = self.children.remove(&idx) {
            shutdown::terminate(&mut child, Duration::from_millis(1500)).await;
        }
        if let Some(s) = self.services.get_mut(idx) {
            s.status = Status::Stopped;
            s.started_at = None;
            s.pid = None;
        }
    }

    async fn restart_service(&mut self, idx: usize) {
        self.stop_service(idx).await;
        if let Some(s) = self.services.get_mut(idx) {
            s.restart_count = s.restart_count.saturating_add(1);
        }
        self.start_service(idx).await;
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
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = App::new(cfg, tx);

    // set up terminal BEFORE spawning children. if this fails (e.g. no tty),
    // we bail before anything becomes reachable.
    let mut term = match setup_terminal() {
        Ok(t) => t,
        Err(e) => return Err(e),
    };

    for idx in 0..app.services.len() {
        app.start_service(idx).await;
    }
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(150));

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;

    let res: Result<()> = loop {
        tokio::select! {
            _ = sigterm.recv() => { app.quitting = true; }
            _ = sighup.recv() => { app.quitting = true; }
            _ = tick.tick() => {
                reap_exited(&mut app);
                term.draw(|f| crate::ui::draw(f, &app))?;
                if app.quitting {
                    break Ok(());
                }
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
