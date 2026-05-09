//! smoke test for the `completions` subcommand. we can't easily exec the
//! binary from a unit test, so we invoke clap_complete directly against the
//! public CLI surface via `env!` + a minimal re-definition would duplicate
//! too much. instead, shell out to the built binary when present.

use std::process::Command;

fn bin() -> Option<std::path::PathBuf> {
    // cargo sets CARGO_BIN_EXE_<name> for integration tests
    let p = env!("CARGO_BIN_EXE_pulse");
    let pb = std::path::PathBuf::from(p);
    if pb.exists() {
        Some(pb)
    } else {
        None
    }
}

#[test]
fn bash_completions_include_subcommands() {
    let Some(b) = bin() else {
        eprintln!("skipping: pulse binary not built");
        return;
    };
    let out = Command::new(&b)
        .args(["completions", "bash"])
        .output()
        .expect("run completions");
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("pulse"));
    // bash completion scripts include subcommand names verbatim somewhere
    assert!(s.contains("init"));
    assert!(s.contains("share"));
}

#[test]
fn zsh_completions_run() {
    let Some(b) = bin() else { return };
    let out = Command::new(&b)
        .args(["completions", "zsh"])
        .output()
        .expect("run completions");
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("_pulse") || s.contains("pulse"));
}

#[test]
fn fish_completions_run() {
    let Some(b) = bin() else { return };
    let out = Command::new(&b)
        .args(["completions", "fish"])
        .output()
        .expect("run completions");
    assert!(out.status.success());
}

#[test]
fn unknown_shell_errors() {
    let Some(b) = bin() else { return };
    let out = Command::new(&b)
        .args(["completions", "pulseshell"])
        .output()
        .expect("run completions");
    assert!(!out.status.success());
}
