//! integration tests for the v0.3.1 polish pass: scrollback, crash-loop,
//! log_buffer plumbing, env-watching, shell resolution, pattern detection,
//! ansi overwrite, error hints.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use pulse::ansi_overwrite;
use pulse::config::{self, ServiceSpec};
use pulse::errors;
use pulse::patterns;
use pulse::service::{Origin, Service};
use pulse::shellcmd;
use pulse::supervisor;

fn mk_spec(name: &str) -> ServiceSpec {
    ServiceSpec {
        name: name.into(),
        cmd: "echo hi".into(),
        cwd: None,
        env: HashMap::new(),
        color: None,
        probe: None,
        port: None,
        agent: None,
        depends_on: Vec::new(),
        tap: None,
        auto_restart: None,
        watch_env: None,
    }
}

#[test]
fn log_buffer_setting_bounds_ring_to_50() {
    // parse config with tiny ring, spam 100, expect 50 retained
    let raw = r#"
        [global]
        log_buffer = 50

        [[service]]
        name = "a"
        cmd = "echo"
    "#;
    let cfg = config::parse(raw).unwrap();
    assert_eq!(cfg.log_buffer_size(), 50);
    let mut svc = Service::with_log_cap(cfg.services[0].clone(), cfg.log_buffer_size());
    for i in 0..100 {
        svc.push_log(Origin::Stdout, format!("l{i}"));
    }
    assert_eq!(svc.logs.len(), 50);
    assert!(svc.logs.front().unwrap().text.ends_with("50"));
    assert!(svc.logs.back().unwrap().text.ends_with("99"));
}

#[test]
fn log_buffer_defaults_to_2000() {
    let raw = r#"
        [[service]]
        name = "a"
        cmd = "echo"
    "#;
    let cfg = config::parse(raw).unwrap();
    assert_eq!(cfg.log_buffer_size(), 2000);
}

#[test]
fn scroll_state_default_pinned_to_bottom() {
    let s = Service::new(mk_spec("api"));
    assert_eq!(s.log_scroll, 0);
    assert!(!s.is_scrolled());
}

#[test]
fn clear_logs_resets_scroll() {
    let mut s = Service::new(mk_spec("api"));
    for i in 0..20 {
        s.push_log(Origin::Stdout, format!("l{i}"));
    }
    s.log_scroll = 10;
    assert!(s.is_scrolled());
    s.clear_logs();
    assert!(!s.is_scrolled());
}

#[test]
fn crash_backoff_sequence() {
    assert_eq!(supervisor::crash_backoff(0), Duration::from_secs(1));
    assert_eq!(supervisor::crash_backoff(1), Duration::from_secs(2));
    assert_eq!(supervisor::crash_backoff(2), Duration::from_secs(4));
    assert_eq!(supervisor::crash_backoff(3), Duration::from_secs(8));
    assert_eq!(supervisor::crash_backoff(4), Duration::from_secs(15));
}

#[test]
fn crash_give_up_is_five() {
    assert_eq!(supervisor::CRASH_GIVE_UP, 5);
}

#[test]
fn auto_restart_default_on() {
    let s = mk_spec("x");
    assert!(s.auto_restart_enabled());
    let mut s2 = mk_spec("y");
    s2.auto_restart = Some(false);
    assert!(!s2.auto_restart_enabled());
}

#[test]
fn pattern_scanner_flags_panic() {
    let (k, _) = patterns::scan("thread panicked at x:1").unwrap();
    assert_eq!(k, "panic");
}

#[test]
fn pattern_scanner_ignores_clean_line() {
    assert!(patterns::scan("ok Listening on :3000").is_none());
}

#[test]
fn pattern_cooldown_blocks_rapid_refire() {
    let mut svc = Service::new(mk_spec("api"));
    assert!(patterns::may_fire(&mut svc, "panic"));
    assert!(!patterns::may_fire(&mut svc, "panic"));
}

#[test]
fn ansi_overwrite_compile_loop() {
    // emulate cargo-watch: Compiling… then Compiling… then Finished
    let raw = "Compiling foo\r\x1b[2KCompiling foo v0.1\r\x1b[2KFinished in 1.2s\n";
    let out = ansi_overwrite::collapse(raw);
    assert_eq!(out, vec!["Finished in 1.2s"]);
}

#[test]
fn ansi_overwrite_preserves_plain_newlines() {
    let raw = "listening\nready\n";
    assert_eq!(
        ansi_overwrite::collapse(raw),
        vec!["listening".to_string(), "ready".to_string()]
    );
}

#[test]
fn hint_fires_for_missing_cwd() {
    let h = errors::missing_cwd_hint(std::path::Path::new("./nope"));
    assert!(h.contains("nope"));
    assert!(h.contains("doesn't exist"));
}

#[test]
fn hint_fires_for_port_in_use() {
    let h = errors::port_in_use_hint(8080);
    assert!(h.contains("8080"));
    assert!(h.contains("pulse ports"));
}

#[test]
fn shell_plan_resolves_cwd_env() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join(".env"), "FROM_DOTENV=yes").unwrap();
    let cfg = config::parse(&format!(
        r#"
        [[service]]
        name = "api"
        cmd = "x"
        cwd = "{}"
        env = {{ CONFIG_KEY = "config_val" }}
        "#,
        tmp.path().display()
    ))
    .unwrap();
    let mut parent = HashMap::new();
    parent.insert("PARENT_KEY".into(), "parent_val".into());
    parent.insert("SHELL".into(), "/bin/bash".into());
    let plan = shellcmd::plan(&cfg, "api", &parent).unwrap();
    assert_eq!(plan.env.get("PARENT_KEY").unwrap(), "parent_val");
    assert_eq!(plan.env.get("FROM_DOTENV").unwrap(), "yes");
    assert_eq!(plan.env.get("CONFIG_KEY").unwrap(), "config_val");
    assert!(plan.ps1.contains("pulse:api"));
    assert_eq!(plan.cwd, PathBuf::from(tmp.path()));
    assert!(plan.shell.ends_with("bash"));
}

#[test]
fn envwatch_detects_env_files_in_cwd() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(pulse::envwatch::present_env_files(tmp.path()).is_empty());
    std::fs::write(tmp.path().join(".env"), "A=1").unwrap();
    let files = pulse::envwatch::present_env_files(tmp.path());
    assert_eq!(files.len(), 1);
}

#[test]
fn ipc_socket_path_uses_pid() {
    let p = pulse::ipc::socket_path_for(42);
    assert!(p.to_string_lossy().contains("42"));
    assert!(p.to_string_lossy().ends_with(".sock"));
}
