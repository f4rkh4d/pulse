use std::collections::VecDeque;
use std::time::Instant;

use chrono::{DateTime, Local};

use crate::agents::{Agent, Species};
use crate::config::ServiceSpec;
use crate::ports::PortState;
use crate::probe::ProbeState;

pub const LOG_CAP: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Starting,
    Running,
    Stopped,
    Crashed,
    CrashedTooMany,
}

impl Status {
    pub fn dot(self) -> &'static str {
        match self {
            Status::Running => "●",
            Status::Starting => "◐",
            Status::Stopped => "○",
            Status::Crashed => "✗",
            Status::CrashedTooMany => "✗",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Starting => "starting",
            Status::Stopped => "stopped",
            Status::Crashed => "crashed",
            Status::CrashedTooMany => "crashed-too-many",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub ts: DateTime<Local>,
    pub origin: Origin,
    pub text: String,
}

#[derive(Debug)]
pub struct Service {
    pub spec: ServiceSpec,
    pub status: Status,
    pub logs: VecDeque<LogLine>,
    pub log_cap: usize,
    pub started_at: Option<Instant>,
    pub last_start: Option<Instant>,
    pub restart_count: u32,
    /// consecutive crashes tracked for auto-restart backoff. reset on manual
    /// restart or after 30s of healthy uptime.
    pub crash_streak: u32,
    pub pid: Option<u32>,
    pub probe: ProbeState,
    pub port: PortState,
    pub agent: Option<Agent>,
    pub last_activity: Option<Instant>,
    /// when non-zero, logs pane is scrolled up by N lines from the tail. zero
    /// means pinned to bottom (auto-tail on new lines).
    pub log_scroll: usize,
    /// sidebar "unhealthy" flag, toggled by the error-pattern scanner.
    pub unhealthy: bool,
    /// per-pattern cooldown. stores the last time a given pattern fired so
    /// we don't spam alerts when a service is panic-looping.
    pub pattern_cooldowns: std::collections::HashMap<&'static str, Instant>,
}

impl Service {
    pub fn new(spec: ServiceSpec) -> Self {
        Self::with_log_cap(spec, LOG_CAP)
    }

    pub fn with_log_cap(spec: ServiceSpec, log_cap: usize) -> Self {
        let agent = spec
            .agent
            .as_ref()
            .map(|a| Agent::new(Species::parse(&a.kind)));
        let cap = log_cap.max(1);
        Self {
            spec,
            status: Status::Stopped,
            logs: VecDeque::with_capacity(cap),
            log_cap: cap,
            started_at: None,
            last_start: None,
            restart_count: 0,
            crash_streak: 0,
            pid: None,
            probe: ProbeState::default(),
            port: PortState::default(),
            agent,
            last_activity: None,
            log_scroll: 0,
            unhealthy: false,
            pattern_cooldowns: std::collections::HashMap::new(),
        }
    }

    pub fn push_log(&mut self, origin: Origin, text: String) {
        if self.logs.len() >= self.log_cap {
            self.logs.pop_front();
        }
        self.logs.push_back(LogLine {
            ts: Local::now(),
            origin,
            text,
        });
        if matches!(origin, Origin::Stdout | Origin::Stderr) {
            self.last_activity = Some(Instant::now());
        }
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
        self.log_scroll = 0;
    }

    pub fn uptime(&self) -> Option<std::time::Duration> {
        self.started_at.map(|t| t.elapsed())
    }

    /// scrolled off the tail? used by the renderer to show a "N below" hint
    /// and to decide whether to auto-tail on new lines.
    pub fn is_scrolled(&self) -> bool {
        self.log_scroll > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkservice() -> Service {
        Service::new(ServiceSpec {
            name: "t".into(),
            cmd: "echo hi".into(),
            cwd: None,
            env: Default::default(),
            color: None,
            probe: None,
            port: None,
            agent: None,
            depends_on: Vec::new(),
            tap: None,
            auto_restart: None,
            watch_env: None,
        })
    }

    #[test]
    fn ring_truncates() {
        let mut s = mkservice();
        for i in 0..(LOG_CAP + 50) {
            s.push_log(Origin::Stdout, format!("line {i}"));
        }
        assert_eq!(s.logs.len(), LOG_CAP);
        // oldest kept should be line 50
        assert!(s.logs.front().unwrap().text.ends_with("50"));
        assert!(s
            .logs
            .back()
            .unwrap()
            .text
            .ends_with(&format!("{}", LOG_CAP + 49)));
    }

    #[test]
    fn clear_empties() {
        let mut s = mkservice();
        s.push_log(Origin::Stdout, "x".into());
        s.push_log(Origin::Stderr, "y".into());
        assert_eq!(s.logs.len(), 2);
        s.clear_logs();
        assert!(s.logs.is_empty());
    }

    #[test]
    fn status_dots() {
        assert_eq!(Status::Running.dot(), "●");
        assert_eq!(Status::Crashed.dot(), "✗");
    }

    #[test]
    fn custom_log_cap_bounds_ring() {
        let spec = ServiceSpec {
            name: "t".into(),
            cmd: "echo hi".into(),
            cwd: None,
            env: Default::default(),
            color: None,
            probe: None,
            port: None,
            agent: None,
            depends_on: Vec::new(),
            tap: None,
            auto_restart: None,
            watch_env: None,
        };
        let mut s = Service::with_log_cap(spec, 50);
        for i in 0..100 {
            s.push_log(Origin::Stdout, format!("l{i}"));
        }
        assert_eq!(s.logs.len(), 50);
        assert!(s.logs.front().unwrap().text.ends_with("50"));
        assert!(s.logs.back().unwrap().text.ends_with("99"));
    }

    #[test]
    fn scroll_flag_tracks_state() {
        let mut s = mkservice();
        assert!(!s.is_scrolled());
        s.log_scroll = 10;
        assert!(s.is_scrolled());
        s.clear_logs();
        assert!(!s.is_scrolled());
    }
}
