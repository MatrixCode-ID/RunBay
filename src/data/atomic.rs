//! Atomic file writes: write to `<file>.tmp`, flush, then rename over the target.

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

/// Writes `bytes` to `path` atomically so a crash never leaves a half-written file.
///
/// The parent folder is created when missing. On Windows `fs::rename` replaces an
/// existing target (it uses `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create folder {}", parent.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = Path::new(&tmp);
    {
        let mut file =
            fs::File::create(tmp).with_context(|| format!("cannot write {}", tmp.display()))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(tmp, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

/// Serializes `value` as pretty JSON and writes it atomically.
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_atomic(path, &bytes)
}

/// Reads and parses a JSON file.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    serde_json::from_slice(bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("a.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert!(!dir.path().join("sub").join("a.json.tmp").exists());
    }

    #[test]
    fn reads_json_with_bom() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.json");
        fs::write(&path, b"\xEF\xBB\xBF{\"x\":1}").unwrap();
        let v: serde_json::Value = read_json(&path).unwrap();
        assert_eq!(v["x"], 1);
    }
}
