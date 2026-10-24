//! Load with Windows: a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

use anyhow::{Context, Result};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{HSTRING, w};

const RUN_KEY: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");

/// The registry value name used by RunBay.
pub const VALUE_NAME: &str = "RunBay";

/// The command line written for autostart: the quoted executable path plus `--autostart`.
pub fn command_line() -> Result<String> {
    let exe = std::env::current_exe()?;
    Ok(format!("\"{}\" --autostart", exe.display()))
}

/// Writes (`Some`) or removes (`None`) the autostart value `name`.
pub fn set(name: &str, command: Option<&str>) -> Result<()> {
    let name = HSTRING::from(name);
    // SAFETY: registry calls with valid strings and buffers.
    unsafe {
        match command {
            Some(command) => {
                let wide: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    RUN_KEY,
                    &name,
                    REG_SZ.0,
                    Some(wide.as_ptr() as *const _),
                    (wide.len() * 2) as u32,
                )
                .ok()
                .context("cannot write the autostart registry value")
            }
            None => {
                let result = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, &name);
                if result.is_ok() || result == ERROR_FILE_NOT_FOUND {
                    Ok(())
                } else {
                    result
                        .ok()
                        .context("cannot remove the autostart registry value")
                }
            }
        }
    }
}

/// Reads the autostart value `name`, if present.
pub fn get(name: &str) -> Option<String> {
    let name = HSTRING::from(name);
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: the buffer size is passed in bytes.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            &name,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };
    if result.is_err() {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_and_remove_a_test_value() {
        let name = format!("RunBay-Test-{}", std::process::id());
        set(&name, Some("\"C:\\RunBay\\RunBay.exe\" --autostart")).unwrap();
        assert_eq!(
            get(&name).as_deref(),
            Some("\"C:\\RunBay\\RunBay.exe\" --autostart")
        );
        set(&name, None).unwrap();
        assert_eq!(get(&name), None);
        // Removing a missing value is not an error.
        set(&name, None).unwrap();
    }
}
