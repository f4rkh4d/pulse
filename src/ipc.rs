//! tiny unix-socket IPC so `pulse share` can grab live state from a running
//! TUI instead of writing a configured-only snapshot.
//!
//! wire protocol is one line in, one blob out. no framing ceremony.

use std::path::PathBuf;

/// directory we drop sockets into. tries `~/.cache/pulse/` first, falls back
/// to `/tmp/`. returned path is guaranteed to exist.
pub fn socket_dir() -> PathBuf {
    if let Some(home) = dirs_home() {
        let dir = home.join(".cache").join("pulse");
        if std::fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    PathBuf::from("/tmp")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// where the current process's socket should live.
pub fn socket_path_for(pid: u32) -> PathBuf {
    socket_dir().join(format!("{pid}.sock"))
}

/// find the most recently-modified pulse socket in the socket dir. returns
/// None when nothing's running.
pub fn find_latest_socket() -> Option<PathBuf> {
    let dir = socket_dir();
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("sock") {
            continue;
        }
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        match &best {
            Some((t, _)) if *t >= mtime => {}
            _ => best = Some((mtime, path)),
        }
    }
    best.map(|(_, p)| p)
}

/// also try `/tmp` if the default dir came up empty. handy when the user ran
/// pulse under a container that doesn't expose $HOME.
pub fn find_latest_socket_with_fallback() -> Option<PathBuf> {
    find_latest_socket().or_else(|| {
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for entry in std::fs::read_dir("/tmp").ok()?.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !name.starts_with("pulse-") || !name.ends_with(".sock") {
                continue;
            }
            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            match &best {
                Some((t, _)) if *t >= mtime => {}
                _ => best = Some((mtime, path)),
            }
        }
        best.map(|(_, p)| p)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_uses_pid() {
        let p = socket_path_for(12345);
        assert!(p.to_string_lossy().contains("12345"));
    }
}
