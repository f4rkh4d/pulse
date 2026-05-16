use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::config::ServiceSpec;
use crate::service::Origin;

/// events emitted by running children back to the app event loop.
#[derive(Debug)]
pub enum SupEvent {
    Started {
        idx: usize,
        pid: u32,
    },
    Log {
        idx: usize,
        origin: Origin,
        line: String,
    },
    Exited {
        idx: usize,
        code: Option<i32>,
    },
    SpawnError {
        idx: usize,
        msg: String,
    },
}

/// spawn a child process for `spec` and stream its output lines on `tx`.
pub async fn spawn_one(
    idx: usize,
    spec: &ServiceSpec,
    tx: mpsc::UnboundedSender<SupEvent>,
) -> Result<SpawnedChild> {
    let parts = spec
        .parse_cmd()
        .map_err(|e| anyhow!("bad cmd for {}: {e}", spec.name))?;
    let (prog, args) = parts.split_first().ok_or_else(|| anyhow!("empty cmd"))?;

    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(false);

    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }

    // new process group on unix so signals hit the whole tree.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // tokio::Command proxies pre_exec via process_group on newer versions,
        // but the std-style hook works via as_std_mut.
        unsafe {
            cmd.as_std_mut().pre_exec(|| {
                let _ = nix::unistd::setsid();
                Ok(())
            });
        }
    }

    let mut child: Child = cmd
        .spawn()
        .with_context(|| format!("spawn failed for `{}`", spec.name))?;

    let pid = child.id().unwrap_or(0);
    let _ = tx.send(SupEvent::Started { idx, pid });

    if let Some(out) = child.stdout.take() {
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                if tx2
                    .send(SupEvent::Log {
                        idx,
                        origin: Origin::Stdout,
                        line: l,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                if tx2
                    .send(SupEvent::Log {
                        idx,
                        origin: Origin::Stderr,
                        line: l,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    Ok(SpawnedChild {
        child,
        pid,
        started: Instant::now(),
    })
}

pub struct SpawnedChild {
    pub child: Child,
    pub pid: u32,
    pub started: Instant,
}

impl SpawnedChild {
    pub async fn watch(mut self, idx: usize, tx: mpsc::UnboundedSender<SupEvent>) {
        match self.child.wait().await {
            Ok(status) => {
                let _ = tx.send(SupEvent::Exited {
                    idx,
                    code: status.code(),
                });
            }
            Err(e) => {
                let _ = tx.send(SupEvent::SpawnError {
                    idx,
                    msg: e.to_string(),
                });
            }
        }
    }
}

/// exponential backoff for crash loops.
pub fn backoff_delay(last_start: Option<Instant>, restart_count: u32) -> Duration {
    match last_start {
        Some(t) if t.elapsed() < Duration::from_secs(2) => {
            let mult = 1u64 << restart_count.min(5);
            Duration::from_millis(500 * mult)
        }
        _ => Duration::from_millis(100),
    }
}

/// the crash-ladder we hand out for auto-restart. 1s, 2s, 4s, 8s, 15s cap.
pub fn crash_backoff(streak: u32) -> Duration {
    let secs = match streak {
        0 => 1,
        1 => 2,
        2 => 4,
        3 => 8,
        _ => 15,
    };
    Duration::from_secs(secs)
}

/// after this many consecutive quick crashes we stop auto-restarting.
pub const CRASH_GIVE_UP: u32 = 5;

/// a service is considered "crashing in a loop" when it dies within this
/// window of last_start. runs longer than that reset the streak.
pub fn is_quick_crash(last_start: Option<Instant>) -> bool {
    match last_start {
        Some(t) => t.elapsed() < Duration::from_secs(2),
        None => false,
    }
}

/// window after which a running service is deemed "healthy" and the streak
/// can be reset to zero.
pub const HEALTHY_WINDOW: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_on_quick_crash() {
        let now = Instant::now();
        let d1 = backoff_delay(Some(now), 0);
        let d2 = backoff_delay(Some(now), 2);
        assert!(d2 > d1);
    }

    #[test]
    fn backoff_small_on_slow_crash() {
        let d = backoff_delay(None, 5);
        assert!(d < Duration::from_secs(1));
    }

    #[test]
    fn crash_backoff_caps_at_15s() {
        assert_eq!(crash_backoff(0), Duration::from_secs(1));
        assert_eq!(crash_backoff(1), Duration::from_secs(2));
        assert_eq!(crash_backoff(2), Duration::from_secs(4));
        assert_eq!(crash_backoff(3), Duration::from_secs(8));
        assert_eq!(crash_backoff(4), Duration::from_secs(15));
        assert_eq!(crash_backoff(99), Duration::from_secs(15));
    }

    #[test]
    fn quick_crash_window() {
        assert!(is_quick_crash(Some(Instant::now())));
        assert!(!is_quick_crash(None));
    }
}
