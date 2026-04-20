//! tcp port probing + a small `lsof` wrapper for the `pulse ports` subcommand.

use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct PortResult {
    pub idx: usize,
    pub port: u16,
    pub bound: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PortState {
    pub last_bound: Option<bool>,
    pub last_checked: Option<Instant>,
}

impl PortState {
    pub fn record(&mut self, r: &PortResult) {
        self.last_bound = Some(r.bound);
        self.last_checked = Some(Instant::now());
    }
}

/// try-connect to localhost:port. if it connects, something is listening.
pub fn is_bound(port: u16) -> bool {
    let addrs = [
        SocketAddr::from(([127, 0, 0, 1], port)),
        SocketAddr::from(([0, 0, 0, 0], port)),
    ];
    for a in addrs.iter() {
        if TcpStream::connect_timeout(a, Duration::from_millis(200)).is_ok() {
            return true;
        }
    }
    false
}

pub async fn run(idx: usize, port: u16, tx: mpsc::UnboundedSender<PortResult>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(2));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        // blocking connect, off the reactor.
        let bound = tokio::task::spawn_blocking(move || is_bound(port))
            .await
            .unwrap_or(false);
        if tx.send(PortResult { idx, port, bound }).is_err() {
            break;
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListenEntry {
    pub command: String,
    pub pid: u32,
    pub port: u16,
}

/// shell out to `lsof` and parse LISTEN entries. unix-only; returns empty on failure.
pub fn listeners() -> Vec<ListenEntry> {
    #[cfg(not(unix))]
    {
        return Vec::new();
    }
    #[cfg(unix)]
    {
        let out = match Command::new("lsof").args(["-i", "-P", "-n"]).output() {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };
        if !out.status.success() {
            return Vec::new();
        }
        let text = String::from_utf8_lossy(&out.stdout);
        parse_lsof(&text)
    }
}

pub fn parse_lsof(text: &str) -> Vec<ListenEntry> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        if !line.contains("LISTEN") {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 9 {
            continue;
        }
        let command = cols[0].to_string();
        let pid: u32 = match cols[1].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let name = cols[8];
        if let Some(port) = port_from_lsof_name(name) {
            out.push(ListenEntry { command, pid, port });
        }
    }
    out.sort_by_key(|e| e.port);
    out.dedup_by(|a, b| a.port == b.port && a.pid == b.pid);
    out
}

fn port_from_lsof_name(name: &str) -> Option<u16> {
    // examples: *:3000, 127.0.0.1:5432, [::1]:8080
    let colon = name.rfind(':')?;
    let tail = &name[colon + 1..];
    let tail = tail.split(' ').next().unwrap_or(tail);
    let tail = tail.trim_end_matches("(LISTEN)").trim();
    tail.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lsof_output() {
        let sample = "\
COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
node    4242 me  18u IPv4 0x1    0t0     TCP  *:3000 (LISTEN)
postgres 55  me  7u  IPv6 0x2    0t0     TCP  [::1]:5432 (LISTEN)
chrome   99  me  10u IPv4 0x3    0t0     TCP  10.0.0.1:55123->1.1.1.1:443 (ESTABLISHED)
";
        let list = parse_lsof(sample);
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|e| e.port == 3000 && e.command == "node"));
        assert!(list.iter().any(|e| e.port == 5432));
    }

    #[test]
    fn is_bound_false_for_unused_high_port() {
        // 1 is basically never listening from userspace.
        assert!(!is_bound(1));
    }
}
