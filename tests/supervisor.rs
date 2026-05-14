use std::time::Duration;
use tokio::sync::mpsc;

use pulse::config::ServiceSpec;
use pulse::service::Origin;
use pulse::supervisor::{spawn_one, SupEvent};

#[tokio::test]
async fn spawns_echo_and_captures_stdout() {
    let spec = ServiceSpec {
        name: "echoer".into(),
        cmd: "sh -c 'echo hello && echo world'".into(),
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
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sc = spawn_one(0, &spec, tx.clone()).await.expect("spawn");
    let waiter = tokio::spawn(sc.watch(0, tx.clone()));

    let mut got = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(timeout, rx.recv()).await {
            Ok(Some(SupEvent::Log {
                origin: Origin::Stdout,
                line,
                ..
            })) => got.push(line),
            Ok(Some(SupEvent::Exited { .. })) => break,
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    let _ = waiter.await;
    assert!(got.iter().any(|l| l == "hello"));
    assert!(got.iter().any(|l| l == "world"));
}

#[tokio::test]
async fn bad_binary_errors_out() {
    let spec = ServiceSpec {
        name: "missing".into(),
        cmd: "definitely-not-a-real-binary-zzz".into(),
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
    let (tx, _rx) = mpsc::unbounded_channel();
    let r = spawn_one(0, &spec, tx).await;
    assert!(r.is_err());
}

#[tokio::test]
async fn env_is_injected() {
    let mut env = std::collections::HashMap::new();
    env.insert("PULSE_TEST_VAR".into(), "pong".into());
    let spec = ServiceSpec {
        name: "env".into(),
        cmd: "sh -c 'echo $PULSE_TEST_VAR'".into(),
        cwd: None,
        env,
        color: None,
        probe: None,
        port: None,
        agent: None,
        depends_on: Vec::new(),
        tap: None,
        auto_restart: None,
        watch_env: None,
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sc = spawn_one(0, &spec, tx.clone()).await.expect("spawn");
    let waiter = tokio::spawn(sc.watch(0, tx.clone()));
    let mut saw = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(timeout, rx.recv()).await {
            Ok(Some(SupEvent::Log {
                origin: Origin::Stdout,
                line,
                ..
            })) => {
                if line == "pong" {
                    saw = true;
                }
            }
            Ok(Some(SupEvent::Exited { .. })) => break,
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    let _ = waiter.await;
    assert!(saw, "expected PULSE_TEST_VAR to reach child env");
}
