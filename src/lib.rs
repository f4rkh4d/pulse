//! pulse — one window for all your local dev servers.
//!
//! this lib target exists so integration tests can reach the internals.
//! the actual UX lives in the `pulse` binary.

pub mod agents;
pub mod ansi_overwrite;
pub mod app;
pub mod config;
pub mod deps;
pub mod discover;
pub mod envwatch;
pub mod errors;
pub mod graph;
pub mod ipc;
pub mod keymap;
pub mod patterns;
pub mod ports;
pub mod probe;
pub mod service;
pub mod share;
pub mod shellcmd;
pub mod shutdown;
pub mod supervisor;
pub mod tap;
pub mod theme_file;
pub mod ui;

/// small startup banner. prints to stderr on tui launch unless --quiet.
/// kept as plain ascii so it renders in any terminal, even windows console
/// (whenever we get around to supporting that).
pub fn banner(version: &str) -> String {
    format!(
        "  _ __  _   _| |___  ___\n | '_ \\| | | | / __|/ _ \\\n | |_) | |_| | \\__ \\  __/\n | .__/ \\__,_|_|___/\\___|   v{version}\n |_|\n",
    )
}

#[cfg(test)]
mod banner_tests {
    use super::banner;

    #[test]
    fn banner_includes_version() {
        let b = banner("9.9.9");
        assert!(b.contains("v9.9.9"));
    }

    #[test]
    fn banner_is_plain_ascii() {
        let b = banner("0.3.0");
        assert!(b.is_ascii());
    }

    #[test]
    fn banner_multiline() {
        let b = banner("0.3.0");
        assert!(b.lines().count() >= 4);
    }
}
