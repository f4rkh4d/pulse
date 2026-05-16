//! watch `.env` files in a service's cwd. on change, the service restarts.
//!
//! notify is already a dependency (used for config hot-reload), so we reuse
//! the recommended_watcher pattern here.

use std::path::{Path, PathBuf};

use notify::{Event as NotifyEvent, EventKind, RecursiveMode, Watcher};
use tokio::sync::mpsc;

/// standard env filenames we'll watch if present in `cwd`.
pub const ENV_FILES: &[&str] = &[".env", ".env.local", ".env.development"];

/// return the list of env files that actually exist inside `cwd`.
pub fn present_env_files(cwd: &Path) -> Vec<PathBuf> {
    ENV_FILES
        .iter()
        .map(|f| cwd.join(f))
        .filter(|p| p.exists())
        .collect()
}

/// spin up a watcher on any env files found in `cwd`. returns a receiver that
/// fires `()` on each change event, and a guard that must be held to keep the
/// watcher alive.
pub fn watch(cwd: &Path) -> Option<(mpsc::UnboundedReceiver<()>, Box<dyn std::any::Any + Send>)> {
    let files = present_env_files(cwd);
    if files.is_empty() {
        return None;
    }
    let (tx, rx) = mpsc::unbounded_channel();
    let targets: Vec<PathBuf> = files.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<NotifyEvent, _>| {
        if let Ok(ev) = res {
            if matches!(
                ev.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) && ev.paths.iter().any(|p| targets.iter().any(|t| p == t))
            {
                let _ = tx.send(());
            }
        }
    })
    .ok()?;
    // watch the cwd non-recursively. editors that do rename-dance will hit the
    // create/remove events on the parent dir so this catches them too.
    watcher.watch(cwd, RecursiveMode::NonRecursive).ok()?;
    Some((rx, Box::new(watcher)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn present_env_files_finds_dotenv() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "FOO=bar").unwrap();
        let got = present_env_files(dir.path());
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with(".env"));
    }

    #[test]
    fn present_env_files_empty_when_absent() {
        let dir = tempdir().unwrap();
        assert!(present_env_files(dir.path()).is_empty());
    }

    #[test]
    fn present_env_files_picks_up_local_and_dev() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".env.local"), "A=1").unwrap();
        std::fs::write(dir.path().join(".env.development"), "B=2").unwrap();
        let got = present_env_files(dir.path());
        assert_eq!(got.len(), 2);
    }
}
