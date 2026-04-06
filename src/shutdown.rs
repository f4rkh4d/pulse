use std::time::Duration;

use tokio::process::Child;

/// send SIGTERM to the process group, wait up to `grace`, then SIGKILL.
#[cfg(unix)]
pub async fn terminate(child: &mut Child, grace: Duration) {
    let pid_i32 = child.id().map(|p| p as i32);
    if let Some(p) = pid_i32 {
        let pgid = nix::unistd::Pid::from_raw(-p);
        let _ = nix::sys::signal::kill(pgid, nix::sys::signal::Signal::SIGTERM);
    }
    let waited = tokio::time::timeout(grace, child.wait()).await;
    // always also sigkill the whole group as belt + suspenders; sh doesn't
    // always propagate sigterm to its sleep/date subprocesses cleanly.
    if let Some(p) = pid_i32 {
        let pgid = nix::unistd::Pid::from_raw(-p);
        let _ = nix::sys::signal::kill(pgid, nix::sys::signal::Signal::SIGKILL);
    }
    if waited.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

#[cfg(not(unix))]
pub async fn terminate(child: &mut Child, _grace: Duration) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}
