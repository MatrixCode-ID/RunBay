//! PowerShell resolution and the command line used to run a script.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::data::Shell;

/// Finds an executable by file name on `PATH`.
pub fn find_on_path(file: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(file))
        .find(|candidate| candidate.is_file())
}

fn find_pwsh() -> Option<PathBuf> {
    find_on_path("pwsh.exe").or_else(|| {
        let base = std::env::var_os("ProgramFiles")?;
        let candidate = Path::new(&base)
            .join("PowerShell")
            .join("7")
            .join("pwsh.exe");
        candidate.is_file().then_some(candidate)
    })
}

fn find_powershell() -> Option<PathBuf> {
    find_on_path("powershell.exe").or_else(|| {
        let root = std::env::var_os("SystemRoot")?;
        let candidate = Path::new(&root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        candidate.is_file().then_some(candidate)
    })
}

/// Resolves the executable for `shell`: `auto` prefers `pwsh.exe` and falls back to
/// `powershell.exe`.
pub fn resolve(shell: Shell) -> Result<PathBuf> {
    resolve_with(shell, find_pwsh, find_powershell)
}

/// [`resolve`] with injectable lookups, for tests.
pub fn resolve_with(
    shell: Shell,
    pwsh: impl Fn() -> Option<PathBuf>,
    powershell: impl Fn() -> Option<PathBuf>,
) -> Result<PathBuf> {
    match shell {
        Shell::Auto => match pwsh().or_else(powershell) {
            Some(p) => Ok(p),
            None => bail!("neither pwsh.exe nor powershell.exe was found"),
        },
        Shell::Pwsh => match pwsh() {
            Some(p) => Ok(p),
            None => bail!("pwsh.exe (PowerShell 7) was not found"),
        },
        Shell::Powershell => match powershell() {
            Some(p) => Ok(p),
            None => bail!("powershell.exe was not found"),
        },
    }
}

/// Quotes `text` as a PowerShell single-quoted string.
fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The arguments that run `script`.
///
/// The script runs through a short `-Command` bootstrap instead of `-File` so the output
/// encoding can be set to UTF-8 before the script starts (specification §5.1). The exit
/// code is the script's `exit` value; otherwise the last native command's exit code;
/// otherwise 1 when the script ended with an error, else 0.
pub fn script_args(script: &Path) -> Vec<OsString> {
    let bootstrap = format!(
        "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); \
         $OutputEncoding = [Console]::OutputEncoding; \
         if ($PSStyle) {{ $PSStyle.OutputRendering = 'Ansi' }}; \
         & {}; \
         if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; if (-not $?) {{ exit 1 }}; exit 0",
        ps_quote(&script.to_string_lossy())
    );
    [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
    ]
    .into_iter()
    .map(OsString::from)
    .chain(std::iter::once(OsString::from(bootstrap)))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_prefers_pwsh_and_falls_back() {
        let p = || Some(PathBuf::from("pwsh.exe"));
        let w = || Some(PathBuf::from("powershell.exe"));
        let none = || None;
        assert_eq!(
            resolve_with(Shell::Auto, p, w).unwrap(),
            PathBuf::from("pwsh.exe")
        );
        assert_eq!(
            resolve_with(Shell::Auto, none, w).unwrap(),
            PathBuf::from("powershell.exe")
        );
        assert!(resolve_with(Shell::Auto, none, none).is_err());
        assert!(resolve_with(Shell::Pwsh, none, w).is_err());
        assert_eq!(
            resolve_with(Shell::Powershell, p, w).unwrap(),
            PathBuf::from("powershell.exe")
        );
    }

    #[test]
    fn script_path_is_quoted() {
        let args = script_args(Path::new(r"C:\a b\it's.ps1"));
        let last = args.last().unwrap().to_string_lossy().into_owned();
        assert!(last.contains(r"& 'C:\a b\it''s.ps1'"), "{last}");
        assert_eq!(args[0], "-NoLogo");
    }
}
