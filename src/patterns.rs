//! scan log lines for scary words. agents yelp when they see one.
//!
//! kept tiny on purpose. full-text log rules would turn this into a SIEM,
//! which isn't what anyone opens pulse for.

use std::time::{Duration, Instant};

use crate::agents::Species;
use crate::service::Service;

/// patterns we care about. order matters a little — first match wins so the
/// reported key is stable (one alert per pattern per cooldown).
pub const PATTERNS: &[&str] = &[
    "panic",
    "PANIC",
    "Panic!",
    "fatal",
    "FATAL",
    "Error:",
    "error[",
    "exception",
    "Exception",
    "uncaught",
    "segfault",
    "assertion failed",
    "stack overflow",
];

/// one alert per pattern per 30s per service.
pub const COOLDOWN: Duration = Duration::from_secs(30);

/// scan a freshly-pushed log line for known patterns. returns the key that
/// matched (stable for cooldown bookkeeping) and the cleaned snippet to show.
pub fn scan(line: &str) -> Option<(&'static str, String)> {
    for pat in PATTERNS {
        if line.contains(pat) {
            let cleaned = clean(line);
            return Some((pat, cleaned));
        }
    }
    None
}

/// strip ansi escapes + collapse whitespace + clip to 80 chars. not strict —
/// the goal is "readable in a one-line status message".
pub fn clean(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_esc = false;
    for ch in line.chars() {
        if ch == '\x1b' {
            in_esc = true;
            continue;
        }
        if in_esc {
            // csi ends on a letter, basic fences good enough for log noise
            if ch.is_ascii_alphabetic() {
                in_esc = false;
            }
            continue;
        }
        if ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    let collapsed: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 80 {
        let mut s: String = collapsed.chars().take(79).collect();
        s.push('…');
        s
    } else {
        collapsed
    }
}

/// species-flavored alert phrase. {name} is the service, {line} the snippet.
pub fn alert_template(species: Species) -> &'static str {
    match species {
        Species::Goblin => "{name} screamed: {line}",
        Species::Cat => "{name} knocked over: {line}",
        Species::Ghost => "{name} whispers: {line}",
        Species::Robot => "{name}: LOG_ANOMALY: {line}",
        Species::Blob => "{name} made a mess: {line}",
    }
}

/// check the cooldown map on `svc` for this pattern. if it's clear, stamp it
/// and return true so the caller can fire an alert.
pub fn may_fire(svc: &mut Service, pat: &'static str) -> bool {
    let now = Instant::now();
    if let Some(prev) = svc.pattern_cooldowns.get(pat) {
        if now.duration_since(*prev) < COOLDOWN {
            return false;
        }
    }
    svc.pattern_cooldowns.insert(pat, now);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_panic() {
        let (k, _) = scan("thread 'main' panicked at src/main.rs:1:1").unwrap();
        assert_eq!(k, "panic");
    }

    #[test]
    fn finds_error_colon() {
        let (k, _) = scan("2024-01-01 Error: connection refused").unwrap();
        assert_eq!(k, "Error:");
    }

    #[test]
    fn plain_line_is_none() {
        assert!(scan("just a normal log").is_none());
        assert!(scan("Listening on :3000").is_none());
    }

    #[test]
    fn clean_strips_ansi_and_clips() {
        let raw = "\x1b[31mERR\x1b[0m line with a lot of stuff that keeps going on and on and on past eighty characters easily";
        let c = clean(raw);
        assert!(!c.contains('\x1b'));
        assert!(c.chars().count() <= 80);
    }

    #[test]
    fn cooldown_blocks_second_fire() {
        let spec = crate::config::ServiceSpec {
            name: "api".into(),
            cmd: "x".into(),
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
        let mut svc = Service::new(spec);
        assert!(may_fire(&mut svc, "panic"));
        assert!(!may_fire(&mut svc, "panic"));
        // distinct pattern still fires
        assert!(may_fire(&mut svc, "fatal"));
    }

    #[test]
    fn species_templates_distinct() {
        assert!(alert_template(Species::Goblin).contains("screamed"));
        assert!(alert_template(Species::Cat).contains("knocked"));
        assert!(alert_template(Species::Robot).contains("LOG_ANOMALY"));
    }
}
