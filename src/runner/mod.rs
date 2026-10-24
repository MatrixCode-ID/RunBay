//! Runs actions: PowerShell resolution, Job Objects, output pipes, run state, and the
//! `whenRunning` rules.

mod job;
mod manager;
pub mod shell;

pub use job::Job;
pub use manager::{OutputLine, Run, RunEvent, RunInfo, RunManager, RunRequest, StartOutcome};
