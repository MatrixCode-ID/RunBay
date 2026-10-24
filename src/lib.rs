//! RunBay: registers applications and runs their build/run/custom actions (PowerShell
//! scripts), showing and storing the output of every run.
//!
//! The library holds everything that can be tested without a window; `main.rs` wires it to
//! the Slint user interface.

pub mod ansi;
pub mod data;
pub mod ipc;
pub mod logging;
pub mod mcp;
pub mod platform;
pub mod runner;
pub mod ui;

/// The RunBay version, from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

slint::include_modules!();
