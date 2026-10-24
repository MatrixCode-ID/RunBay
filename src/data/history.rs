//! Run history: `history/index.jsonl` lines, log files, stale-run repair, and retention.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use super::atomic::write_atomic;
use super::model::{AppDef, Retention};

/// Status of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    /// Still running.
    Running,
    /// Ended with exit code 0.
    Success,
    /// Ended with a non-zero exit code or could not start.
    Failed,
    /// Stopped by the user or an agent.
    Stopped,
    /// Not run because the build of a build+run failed.
    Skipped,
}

impl RunStatus {
    /// Lowercase name as stored in the index.
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Success => "success",
            RunStatus::Failed => "failed",
            RunStatus::Stopped => "stopped",
            RunStatus::Skipped => "skipped",
        }
    }

    /// Parses a name produced by [`RunStatus::as_str`].
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "running" => RunStatus::Running,
            "success" => RunStatus::Success,
            "failed" => RunStatus::Failed,
            "stopped" => RunStatus::Stopped,
            "skipped" => RunStatus::Skipped,
            _ => return None,
        })
    }
}

/// Who started a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// A click in the GUI.
    #[default]
    Ui,
    /// An MCP agent.
    Mcp,
}

/// One line of `history/index.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    /// Run id (`yyyyMMdd-HHmmss-<action-id>-<4 hex>`).
    pub run_id: String,
    /// Action id.
    pub action_id: String,
    /// Start time with offset.
    pub start: DateTime<FixedOffset>,
    /// End time, absent while running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<DateTime<FixedOffset>>,
    /// Exit code, absent while running or when the process could not start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Status.
    pub status: RunStatus,
    /// Who started the run.
    #[serde(default)]
    pub source: Source,
    /// MCP client name for agent runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Log path relative to the history folder (`yyyy-MM/<run-id>.log`).
    pub log: String,
}

/// Paths of one app's history.
#[derive(Debug, Clone)]
pub struct History {
    dir: PathBuf,
}

impl History {
    /// History of the app folder `app_dir`.
    pub fn new(app_dir: &Path) -> Self {
        Self {
            dir: app_dir.join("history"),
        }
    }

    /// The `history` folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path of `index.jsonl`.
    pub fn index_path(&self) -> PathBuf {
        self.dir.join("index.jsonl")
    }

    /// Relative log path for a run started at `start`.
    pub fn relative_log(run_id: &str, start: &DateTime<FixedOffset>) -> String {
        format!("{}/{run_id}.log", start.format("%Y-%m"))
    }

    /// Absolute path of a record's log.
    pub fn log_path(&self, record: &RunRecord) -> PathBuf {
        self.dir.join(&record.log)
    }

    /// Reads all records; unreadable lines are skipped. A missing index is empty.
    pub fn read(&self) -> Result<Vec<RunRecord>> {
        let text = match fs::read_to_string(self.index_path()) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).context("cannot read the history index"),
        };
        Ok(text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| match serde_json::from_str(l) {
                Ok(r) => Some(r),
                Err(e) => {
                    log::warn!("skipping a bad history line in {}: {e}", self.dir.display());
                    None
                }
            })
            .collect())
    }

    /// Appends one record to the end of the index.
    pub fn append(&self, record: &RunRecord) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let mut line = serde_json::to_string(record)?;
        line.push('\n');
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.index_path())
            .context("cannot open the history index")?;
        file.write_all(line.as_bytes())?;
        Ok(())
    }

    /// Rewrites the whole index atomically.
    pub fn write_all(&self, records: &[RunRecord]) -> Result<()> {
        let mut text = String::new();
        for r in records {
            text.push_str(&serde_json::to_string(r)?);
            text.push('\n');
        }
        write_atomic(&self.index_path(), text.as_bytes())
    }

    /// Replaces the record with the same run id (or appends it), then applies retention.
    pub fn finish(&self, record: &RunRecord, app: &AppDef) -> Result<()> {
        let mut records = self.read()?;
        match records.iter_mut().find(|r| r.run_id == record.run_id) {
            Some(existing) => *existing = record.clone(),
            None => records.push(record.clone()),
        }
        let records = self.retain(records, app);
        self.write_all(&records)
    }

    /// Changes `running` records whose run id is not in `live` to `stopped`.
    /// Returns the number of repaired records.
    pub fn fix_stale(&self, live: &dyn Fn(&str) -> bool) -> Result<usize> {
        let mut records = self.read()?;
        let mut fixed = 0;
        for r in records.iter_mut() {
            if r.status == RunStatus::Running && !live(&r.run_id) {
                r.status = RunStatus::Stopped;
                if r.end.is_none() {
                    r.end = Some(r.start);
                }
                fixed += 1;
            }
        }
        if fixed > 0 {
            self.write_all(&records)?;
        }
        Ok(fixed)
    }

    /// Applies the retention of every action of `app` to the index and logs on disk.
    pub fn apply_retention(&self, app: &AppDef) -> Result<()> {
        let records = self.read()?;
        let before = records.len();
        let kept = self.retain(records, app);
        if kept.len() != before {
            self.write_all(&kept)?;
        }
        Ok(())
    }

    /// Returns the records to keep and deletes the logs of the others. Running records are
    /// always kept; records of actions that no longer exist are dropped.
    fn retain(&self, records: Vec<RunRecord>, app: &AppDef) -> Vec<RunRecord> {
        let mut keep = vec![true; records.len()];
        for (i, r) in records.iter().enumerate() {
            if app.action(&r.action_id).is_none() && r.status != RunStatus::Running {
                keep[i] = false;
            }
        }
        for action in &app.actions {
            let indices: Vec<usize> = records
                .iter()
                .enumerate()
                .filter(|(_, r)| r.action_id == action.id && r.status != RunStatus::Running)
                .map(|(i, _)| i)
                .collect();
            let limit = match action.retention {
                Retention::Forever => continue,
                Retention::Off => 0,
                Retention::Last { count } => count as usize,
            };
            // Records are appended in start order, so the oldest come first.
            let mut sorted = indices;
            sorted.sort_by(|a, b| records[*a].start.cmp(&records[*b].start));
            let excess = sorted.len().saturating_sub(limit);
            for &i in &sorted[..excess] {
                keep[i] = false;
            }
        }
        let mut result = Vec::new();
        for (r, keep) in records.into_iter().zip(keep) {
            if keep {
                result.push(r);
            } else {
                let path = self.dir.join(&r.log);
                if let Err(e) = fs::remove_file(&path)
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    log::warn!("cannot delete {}: {e}", path.display());
                }
            }
        }
        self.remove_empty_month_folders();
        result
    }

    fn remove_empty_month_folders(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Fails (and is ignored) when the folder is not empty.
                let _ = fs::remove_dir(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::model::Settings;
    use chrono::TimeZone;

    fn record(action: &str, minute: u32, status: RunStatus) -> RunRecord {
        let start = FixedOffset::east_opt(7 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 10, 24, 10, minute, 0)
            .unwrap();
        let run_id = format!("20261024-10{minute:02}00-{action}-0000");
        RunRecord {
            log: History::relative_log(&run_id, &start),
            run_id,
            action_id: action.into(),
            start,
            end: Some(start),
            exit_code: Some(0),
            status,
            source: Source::Ui,
            agent: None,
        }
    }

    fn write_logs(h: &History, records: &[RunRecord]) {
        for r in records {
            let p = h.log_path(r);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "log").unwrap();
        }
    }

    #[test]
    fn index_line_format() {
        let mut r = record("build", 15, RunStatus::Success);
        r.source = Source::Mcp;
        r.agent = Some("Claude Code".into());
        let line = serde_json::to_string(&r).unwrap();
        assert!(line.contains("\"runId\":\"20261024-101500-build-0000\""));
        assert!(line.contains("\"source\":\"mcp\""));
        assert!(line.contains("\"log\":\"2026-10/20261024-101500-build-0000.log\""));
        let ui = serde_json::to_string(&record("run", 1, RunStatus::Failed)).unwrap();
        assert!(!ui.contains("agent"));
    }

    #[test]
    fn retention_last_keeps_newest() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path());
        let mut app = AppDef::new("a", "A", &Settings::default());
        app.action_mut("run").unwrap().retention = Retention::Last { count: 2 };
        let records: Vec<_> = (0..4)
            .map(|m| record("run", m, RunStatus::Success))
            .collect();
        write_logs(&h, &records);
        h.write_all(&records).unwrap();
        h.apply_retention(&app).unwrap();
        let left = h.read().unwrap();
        assert_eq!(left.len(), 2);
        assert_eq!(left[0].run_id, records[2].run_id);
        assert!(!h.log_path(&records[0]).exists());
        assert!(h.log_path(&records[3]).exists());
    }

    #[test]
    fn retention_off_and_forever() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path());
        let mut app = AppDef::new("a", "A", &Settings::default());
        app.action_mut("run").unwrap().retention = Retention::Off;
        app.action_mut("build").unwrap().retention = Retention::Forever;
        let mut records: Vec<_> = (0..3)
            .map(|m| record("run", m, RunStatus::Success))
            .collect();
        records.extend((0..60).map(|m| record("build", m, RunStatus::Success)));
        records.push(record("run", 59, RunStatus::Running));
        h.write_all(&records).unwrap();
        h.apply_retention(&app).unwrap();
        let left = h.read().unwrap();
        assert_eq!(left.iter().filter(|r| r.action_id == "build").count(), 60);
        // The running record survives even with retention off.
        assert_eq!(left.iter().filter(|r| r.action_id == "run").count(), 1);
    }

    #[test]
    fn finish_replaces_running_record() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path());
        let app = AppDef::new("a", "A", &Settings::default());
        let mut r = record("build", 1, RunStatus::Running);
        h.append(&r).unwrap();
        r.status = RunStatus::Failed;
        h.finish(&r, &app).unwrap();
        let left = h.read().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].status, RunStatus::Failed);
    }

    #[test]
    fn stale_running_entries_become_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path());
        let a = record("build", 1, RunStatus::Running);
        let b = record("run", 2, RunStatus::Running);
        h.append(&a).unwrap();
        h.append(&b).unwrap();
        let live = b.run_id.clone();
        assert_eq!(h.fix_stale(&|id| id == live).unwrap(), 1);
        let left = h.read().unwrap();
        assert_eq!(left[0].status, RunStatus::Stopped);
        assert_eq!(left[1].status, RunStatus::Running);
    }

    #[test]
    fn bad_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path());
        h.append(&record("build", 1, RunStatus::Success)).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(h.index_path())
            .unwrap()
            .write_all(b"{broken\n")
            .unwrap();
        assert_eq!(h.read().unwrap().len(), 1);
    }
}
