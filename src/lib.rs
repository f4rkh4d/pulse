//! pulse — one window for all your local dev servers.
//!
//! this lib target exists so integration tests can reach the internals.
//! the actual UX lives in the `pulse` binary.

pub mod app;
pub mod config;
pub mod keymap;
pub mod service;
pub mod shutdown;
pub mod supervisor;
pub mod ui;
