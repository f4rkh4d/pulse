use pulse::probe::{ProbeResult, ProbeState, HISTORY_CAP};

fn r(ok: bool, status: Option<u16>, ms: u128) -> ProbeResult {
    ProbeResult {
        idx: 0,
        status,
        latency_ms: ms,
        ok,
    }
}

#[test]
fn rate_over_mixed_history() {
    let mut s = ProbeState::default();
    for i in 0..10 {
        s.record(&r(i % 2 == 0, Some(200), 30));
    }
    let rate = s.success_rate().unwrap();
    assert!((rate - 0.5).abs() < 1e-6);
}

#[test]
fn history_capped() {
    let mut s = ProbeState::default();
    for _ in 0..(HISTORY_CAP * 3) {
        s.record(&r(true, Some(200), 12));
    }
    assert_eq!(s.history.len(), HISTORY_CAP);
    assert_eq!(s.status_history.len(), HISTORY_CAP);
}

#[test]
fn healthy_flips_after_consecutive_fails() {
    let mut s = ProbeState::default();
    s.record(&r(true, Some(200), 10));
    assert!(s.healthy());
    s.record(&r(false, Some(500), 2200));
    assert!(!s.healthy());
    s.record(&r(true, Some(200), 10));
    assert!(s.healthy());
}

#[test]
fn latency_tracked() {
    let mut s = ProbeState::default();
    s.record(&r(true, Some(200), 42));
    assert_eq!(s.last_latency.unwrap().as_millis(), 42);
}
