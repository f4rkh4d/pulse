//! http health probe worker. lives on its own tokio task per service, pushes
//! results back to the app event loop.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

/// history cap for rolling success rate + sparkline.
pub const HISTORY_CAP: usize = 60;

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub idx: usize,
    pub status: Option<u16>,
    pub latency_ms: u128,
    pub ok: bool,
}

/// rolling probe state attached to a service.
#[derive(Debug, Default, Clone)]
pub struct ProbeState {
    pub last_status: Option<u16>,
    pub last_latency: Option<Duration>,
    pub last_checked: Option<Instant>,
    pub history: VecDeque<bool>,
    pub status_history: VecDeque<Option<u16>>,
    pub consecutive_fails: u32,
}

impl ProbeState {
    pub fn record(&mut self, r: &ProbeResult) {
        self.last_status = r.status;
        self.last_latency = Some(Duration::from_millis(r.latency_ms as u64));
        self.last_checked = Some(Instant::now());
        if self.history.len() >= HISTORY_CAP {
            self.history.pop_front();
        }
        self.history.push_back(r.ok);
        if self.status_history.len() >= HISTORY_CAP {
            self.status_history.pop_front();
        }
        self.status_history.push_back(r.status);
        if r.ok {
            self.consecutive_fails = 0;
        } else {
            self.consecutive_fails = self.consecutive_fails.saturating_add(1);
        }
    }

    pub fn success_rate(&self) -> Option<f32> {
        if self.history.is_empty() {
            return None;
        }
        let n = self.history.len() as f32;
        let ok = self.history.iter().filter(|b| **b).count() as f32;
        Some(ok / n)
    }

    pub fn healthy(&self) -> bool {
        self.consecutive_fails == 0 && self.last_status.is_some()
    }
}

/// spawn a probe loop for a single service. stops when `tx` closes.
pub async fn run(
    idx: usize,
    url: String,
    interval: Duration,
    timeout: Duration,
    expect: Option<u16>,
    tx: mpsc::UnboundedSender<ProbeResult>,
) {
    let client = match reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout)
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let start = Instant::now();
        let result = client.get(&url).send().await;
        let latency_ms = start.elapsed().as_millis();
        let (status, ok) = match result {
            Ok(resp) => {
                let s = resp.status().as_u16();
                let ok = match expect {
                    Some(code) => s == code,
                    None => resp.status().is_success(),
                };
                (Some(s), ok)
            }
            Err(_) => (None, false),
        };
        if tx
            .send(ProbeResult {
                idx,
                status,
                latency_ms,
                ok,
            })
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_starts_empty() {
        let s = ProbeState::default();
        assert!(s.success_rate().is_none());
    }

    #[test]
    fn rolling_window_caps_at_60() {
        let mut s = ProbeState::default();
        for i in 0..(HISTORY_CAP + 20) {
            s.record(&ProbeResult {
                idx: 0,
                status: Some(200),
                latency_ms: 10,
                ok: i % 2 == 0,
            });
        }
        assert_eq!(s.history.len(), HISTORY_CAP);
    }

    #[test]
    fn fail_counter_resets_on_success() {
        let mut s = ProbeState::default();
        for _ in 0..3 {
            s.record(&ProbeResult {
                idx: 0,
                status: None,
                latency_ms: 2000,
                ok: false,
            });
        }
        assert_eq!(s.consecutive_fails, 3);
        s.record(&ProbeResult {
            idx: 0,
            status: Some(200),
            latency_ms: 12,
            ok: true,
        });
        assert_eq!(s.consecutive_fails, 0);
    }
}
