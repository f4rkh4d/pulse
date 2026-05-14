use regex::Regex;

use pulse::config::ServiceSpec;
use pulse::service::{Origin, Service, LOG_CAP};

fn fresh() -> Service {
    Service::new(ServiceSpec {
        name: "t".into(),
        cmd: "true".into(),
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
fn ring_respects_cap() {
    let mut s = fresh();
    for i in 0..(LOG_CAP * 2) {
        s.push_log(Origin::Stdout, format!("n={i}"));
    }
    assert_eq!(s.logs.len(), LOG_CAP);
    assert_eq!(s.logs.front().unwrap().text, format!("n={}", LOG_CAP));
}

#[test]
fn clear_wipes() {
    let mut s = fresh();
    for _ in 0..5 {
        s.push_log(Origin::Stderr, "x".into());
    }
    s.clear_logs();
    assert!(s.logs.is_empty());
}

#[test]
fn regex_filter_matches() {
    let mut s = fresh();
    s.push_log(Origin::Stdout, "info: booting".into());
    s.push_log(Origin::Stderr, "ERROR: panic at disco".into());
    s.push_log(Origin::Stdout, "info: done".into());
    let re = Regex::new("(?i)error").unwrap();
    let matches: Vec<_> = s.logs.iter().filter(|l| re.is_match(&l.text)).collect();
    assert_eq!(matches.len(), 1);
    assert!(matches[0].text.contains("panic"));
}

#[test]
fn distinguishes_origins() {
    let mut s = fresh();
    s.push_log(Origin::Stdout, "a".into());
    s.push_log(Origin::Stderr, "b".into());
    s.push_log(Origin::System, "c".into());
    let stderrs = s.logs.iter().filter(|l| l.origin == Origin::Stderr).count();
    assert_eq!(stderrs, 1);
}
