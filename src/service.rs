use std::collections::VecDeque;
use std::time::Instant;

use chrono::{DateTime, Local};

use crate::config::ServiceSpec;

pub const LOG_CAP: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Starting,
    Running,
    Stopped,
    Crashed,
}

impl Status {
    pub fn dot(self) -> &'static str {
        match self {
            Status::Running => "●",
            Status::Starting => "◐",
            Status::Stopped => "○",
            Status::Crashed => "✗",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Starting => "starting",
            Status::Stopped => "stopped",
            Status::Crashed => "crashed",
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
    pub started_at: Option<Instant>,
    pub last_start: Option<Instant>,
    pub restart_count: u32,
    pub pid: Option<u32>,
}

impl Service {
    pub fn new(spec: ServiceSpec) -> Self {
        Self {
            spec,
            status: Status::Stopped,
            logs: VecDeque::with_capacity(LOG_CAP),
            started_at: None,
            last_start: None,
            restart_count: 0,
            pid: None,
        }
    }

    pub fn push_log(&mut self, origin: Origin, text: String) {
        if self.logs.len() >= LOG_CAP {
            self.logs.pop_front();
        }
        self.logs.push_back(LogLine {
            ts: Local::now(),
            origin,
            text,
        });
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    pub fn uptime(&self) -> Option<std::time::Duration> {
        self.started_at.map(|t| t.elapsed())
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
}
