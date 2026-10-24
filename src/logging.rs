//! A small file logger writing to `data/meta/runbay.log` (rotated to `runbay.old.log` at 1 MB).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};

const MAX_SIZE: u64 = 1024 * 1024;

struct FileLogger {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl FileLogger {
    fn open(&self) -> Option<File> {
        if fs::metadata(&self.path).is_ok_and(|m| m.len() > MAX_SIZE) {
            let _ = fs::rename(&self.path, self.path.with_file_name("runbay.old.log"));
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .ok()
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::Level::Info || metadata.target().starts_with("runbay")
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {}: {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.target(),
            record.args()
        );
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        let mut file = self.file.lock().unwrap();
        if file.is_none() {
            *file = self.open();
        }
        if let Some(f) = file.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Some(f) = self.file.lock().unwrap().as_mut() {
            let _ = f.flush();
        }
    }
}

/// Installs the logger for the data folder `data_dir`. Logging failures are ignored.
pub fn init(data_dir: &Path) {
    let meta = data_dir.join("meta");
    let _ = fs::create_dir_all(&meta);
    let logger = FileLogger {
        path: meta.join("runbay.log"),
        file: Mutex::new(None),
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(if cfg!(debug_assertions) {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        });
    }
}
