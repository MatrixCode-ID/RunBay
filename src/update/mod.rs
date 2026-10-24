//! Auto update: release lookup on GitHub, download with SHA-256 verification, and the
//! executable swap.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::data::atomic::{read_json, write_json};

/// The repository releases are published to.
pub const REPOSITORY: &str = "MatrixCode-ID/RunBay";
/// The GitHub REST API.
pub const GITHUB_API: &str = "https://api.github.com";

/// A published release with its assets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// Version from the tag (`v1.0.0-alpha.2` → `1.0.0-alpha.2`).
    pub version: String,
    /// Release page.
    pub html_url: String,
    /// Download URL of `RunBay.exe`.
    pub exe_url: String,
    /// Download URL of `SHA256SUMS`.
    pub sums_url: String,
}

impl Release {
    /// The parsed version.
    pub fn semver(&self) -> Option<Version> {
        Version::parse(&self.version).ok()
    }
}

/// `data/meta/update.json`: the last ETag and the releases it described, and the skipped
/// version.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdateState {
    /// ETag of the last release list.
    pub etag: Option<String>,
    /// Releases of the last list (used when the server answers 304).
    pub releases: Vec<Release>,
    /// A version the user chose to skip.
    pub skipped: Option<String>,
}

impl UpdateState {
    /// Loads the state; a missing or broken file gives the default.
    pub fn load(meta: &Path) -> Self {
        let path = meta.join("update.json");
        if path.exists() {
            read_json(&path).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    /// Saves the state.
    pub fn save(&self, meta: &Path) -> Result<()> {
        write_json(&meta.join("update.json"), self)
    }
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    html_url: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

/// Parses the GitHub release list; drafts, tags that are not versions, and releases
/// without both assets are ignored.
pub fn parse_releases(json: &str) -> Result<Vec<Release>> {
    let list: Vec<ApiRelease> = serde_json::from_str(json).context("invalid release list")?;
    Ok(list
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = r.tag_name.strip_prefix('v').unwrap_or(&r.tag_name);
            Version::parse(version).ok()?;
            let asset = |name: &str| {
                r.assets
                    .iter()
                    .find(|a| a.name.eq_ignore_ascii_case(name))
                    .map(|a| a.browser_download_url.clone())
            };
            Some(Release {
                version: version.to_string(),
                html_url: r.html_url.clone(),
                exe_url: asset("RunBay.exe")?,
                sums_url: asset("SHA256SUMS")?,
            })
        })
        .collect())
}

/// The highest release newer than `current` (SemVer precedence, no channels), unless it
/// is the skipped version.
pub fn newest(releases: &[Release], current: &str, skipped: Option<&str>) -> Option<Release> {
    let current = Version::parse(current).ok()?;
    let best = releases
        .iter()
        .filter_map(|r| r.semver().map(|v| (v, r)))
        .max_by(|a, b| a.0.cmp(&b.0))?;
    (best.0 > current && skipped != Some(best.1.version.as_str())).then(|| best.1.clone())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .user_agent(format!("RunBay/{}", crate::VERSION))
        .build()
        .into()
}

/// Fetches the release list (with `If-None-Match`) and returns the newest release to
/// offer. The state is updated with the new ETag and list.
pub fn check(api: &str, state: &mut UpdateState, current: &str) -> Result<Option<Release>> {
    let url = format!("{api}/repos/{REPOSITORY}/releases?per_page=30");
    let mut request = agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json");
    if let Some(etag) = &state.etag {
        request = request.header("If-None-Match", etag);
    }
    let mut response = request.call().context("cannot reach GitHub")?;
    match response.status().as_u16() {
        304 => {}
        200 => {
            let etag = response
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let body = response
                .body_mut()
                .read_to_string()
                .context("cannot read the release list")?;
            state.releases = parse_releases(&body)?;
            state.etag = etag;
        }
        403 | 429 => bail!("GitHub rate limit reached; try again later"),
        status => bail!("GitHub answered {status}"),
    }
    Ok(newest(&state.releases, current, state.skipped.as_deref()))
}

fn download_to(url: &str, path: &Path) -> Result<()> {
    let mut response = agent()
        .get(url)
        .call()
        .with_context(|| format!("cannot download {url}"))?;
    if response.status().as_u16() != 200 {
        bail!("download of {url} failed with {}", response.status());
    }
    let mut reader = response.body_mut().as_reader();
    let tmp = path.with_extension("part");
    let mut file = fs::File::create(&tmp)?;
    std::io::copy(&mut reader, &mut file)?;
    drop(file);
    fs::rename(&tmp, path)?;
    Ok(())
}

/// SHA-256 of a file as lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Finds the checksum of `file` in a `sha256sum` style list (`<hash> *<file>` or
/// `<hash>  <file>`).
pub fn checksum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        name.eq_ignore_ascii_case(file)
            .then(|| hash.trim().to_ascii_lowercase())
    })
}

/// Downloads a release into `dir` and verifies it. Returns the verified executable.
/// A checksum mismatch deletes the download.
pub fn download(release: &Release, dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let exe = dir.join(format!("RunBay-{}.exe", release.version));
    if exe.exists() && verify(release, dir, &exe).is_ok() {
        return Ok(exe);
    }
    download_to(&release.exe_url, &exe)?;
    verify(release, dir, &exe).inspect_err(|_| {
        let _ = fs::remove_file(&exe);
    })?;
    Ok(exe)
}

fn verify(release: &Release, dir: &Path, exe: &Path) -> Result<()> {
    let sums_path = dir.join(format!("SHA256SUMS-{}", release.version));
    download_to(&release.sums_url, &sums_path)?;
    let sums = fs::read_to_string(&sums_path)?;
    let expected =
        checksum_for(&sums, "RunBay.exe").context("SHA256SUMS has no entry for RunBay.exe")?;
    let actual = sha256_file(exe)?;
    if actual != expected {
        bail!("the downloaded RunBay.exe does not match its SHA-256 checksum");
    }
    Ok(())
}

/// Replaces `current` with `new`: `current` becomes `<current>.old`, `new` is moved into
/// place, and on failure the old executable is restored.
pub fn swap(current: &Path, new: &Path) -> Result<()> {
    let old = old_path(current);
    let _ = fs::remove_file(&old);
    fs::rename(current, &old).with_context(|| {
        format!(
            "cannot rename {} (no write access to its folder?)",
            current.display()
        )
    })?;
    if let Err(e) = fs::copy(new, current) {
        let _ = fs::remove_file(current);
        fs::rename(&old, current).context("cannot restore the previous executable")?;
        return Err(e).with_context(|| format!("cannot write {}", current.display()));
    }
    let _ = fs::remove_file(new);
    Ok(())
}

/// `RunBay.exe` → `RunBay.exe.old`.
pub fn old_path(current: &Path) -> PathBuf {
    let mut name = current.as_os_str().to_owned();
    name.push(".old");
    PathBuf::from(name)
}

/// Deletes `RunBay.exe.old` left by an update (retried briefly while the old process
/// exits).
pub fn remove_old(current: &Path) {
    let old = old_path(current);
    for _ in 0..20 {
        if !old.exists() || fs::remove_file(&old).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    log::warn!("cannot delete {}", old.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> Release {
        Release {
            version: version.into(),
            html_url: String::new(),
            exe_url: String::new(),
            sums_url: String::new(),
        }
    }

    #[test]
    fn picks_the_highest_newer_version_across_stages() {
        let list = [
            release("1.0.0-alpha.2"),
            release("1.0.0-beta.1"),
            release("1.0.0"),
            release("1.1.0-alpha.1"),
        ];
        assert_eq!(
            newest(&list, "1.0.0-alpha.1", None).unwrap().version,
            "1.1.0-alpha.1"
        );
        assert_eq!(
            newest(&list[..3], "1.0.0-alpha.1", None).unwrap().version,
            "1.0.0"
        );
        assert!(newest(&list, "1.1.0-alpha.1", None).is_none());
        assert!(newest(&list, "1.0.0-alpha.1", Some("1.1.0-alpha.1")).is_none());
        // alpha.10 sorts after alpha.2.
        let list = [release("1.0.0-alpha.10"), release("1.0.0-alpha.2")];
        assert_eq!(
            newest(&list, "1.0.0-alpha.1", None).unwrap().version,
            "1.0.0-alpha.10"
        );
    }

    #[test]
    fn parses_the_release_list() {
        let json = r#"[
          {"tag_name":"v1.0.0-alpha.2","draft":false,"html_url":"h2","assets":[
            {"name":"RunBay.exe","browser_download_url":"e2"},
            {"name":"SHA256SUMS","browser_download_url":"s2"}]},
          {"tag_name":"v9.0.0","draft":true,"html_url":"h9","assets":[]},
          {"tag_name":"nightly","draft":false,"html_url":"hn","assets":[]},
          {"tag_name":"v1.0.0-alpha.1","draft":false,"html_url":"h1","assets":[
            {"name":"RunBay.exe","browser_download_url":"e1"}]}
        ]"#;
        let list = parse_releases(json).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].version, "1.0.0-alpha.2");
        assert_eq!(list[0].exe_url, "e2");
    }

    #[test]
    fn checksum_lines() {
        let sums = "ABCDEF *RunBay.exe\n123456  other.zip";
        assert_eq!(checksum_for(sums, "RunBay.exe").as_deref(), Some("abcdef"));
        assert_eq!(checksum_for(sums, "other.zip").as_deref(), Some("123456"));
        assert_eq!(checksum_for(sums, "missing"), None);
    }

    #[test]
    fn swap_replaces_and_keeps_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("RunBay.exe");
        let new = dir.path().join("new.exe");
        fs::write(&current, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        swap(&current, &new).unwrap();
        assert_eq!(fs::read(&current).unwrap(), b"new");
        assert_eq!(fs::read(old_path(&current)).unwrap(), b"old");
        remove_old(&current);
        assert!(!old_path(&current).exists());
    }

    #[test]
    fn swap_restores_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("RunBay.exe");
        fs::write(&current, b"old").unwrap();
        assert!(swap(&current, &dir.path().join("missing.exe")).is_err());
        assert_eq!(fs::read(&current).unwrap(), b"old");
    }
}
