//! The `data/` folder: settings, window state, groups, apps, and run history.

pub mod atomic;
pub mod history;
pub mod ids;
pub mod model;
pub mod store;

pub use history::{History, RunRecord, RunStatus, Source};
pub use model::*;
pub use store::{LoadedApp, Store};
