//! Windows integration: theme, clipboard, opening files and folders, window focus,
//! pickers, autostart, monitors, and the tray.

pub mod autostart;
pub mod dialogs;
pub mod monitors;
pub mod tray;

use std::path::Path;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, IsIconic, MB_ICONERROR, MB_OK, MessageBoxW, SW_RESTORE,
    SW_SHOW, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow,
};
use windows::core::{HSTRING, w};

/// True when Windows apps use the dark theme.
pub fn system_is_dark() -> bool {
    let mut value = 1u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: reading a DWORD registry value into a local.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    result.is_ok() && value == 0
}

/// Lets any process take the foreground (called before asking another instance to show).
pub fn allow_foreground() {
    // SAFETY: plain Win32 call.
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

/// Shows, restores, and focuses a window.
pub fn bring_to_front(hwnd: isize) {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: plain Win32 calls on a window handle owned by this process.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Opens a file or folder with its Windows association (Explorer for folders).
pub fn open_path(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("{} does not exist", path.display());
    }
    // SAFETY: plain ShellExecuteW call with valid strings.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(path.as_os_str()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        bail!("cannot open {}", path.display());
    }
    Ok(())
}

/// Opens a file in its editor association (`edit` verb), falling back to `open`.
pub fn edit_path(path: &Path) -> Result<()> {
    // SAFETY: plain ShellExecuteW call with valid strings.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("edit"),
            &HSTRING::from(path.as_os_str()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return open_path(path);
    }
    Ok(())
}

/// Copies text to the clipboard.
pub fn copy_text(text: &str) -> Result<()> {
    const CF_UNICODETEXT: u32 = 13;
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: standard clipboard sequence; the memory is owned by the clipboard after
    // SetClipboardData succeeds and freed by us otherwise.
    unsafe {
        OpenClipboard(None).context("cannot open the clipboard")?;
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            let mem = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
            let ptr = GlobalLock(mem) as *mut u16;
            if ptr.is_null() {
                let _ = GlobalFree(Some(mem));
                bail!("cannot lock clipboard memory");
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            let _ = GlobalUnlock(mem);
            if let Err(e) = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(mem.0))) {
                let _ = GlobalFree(Some(HGLOBAL(mem.0)));
                return Err(e.into());
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Shows a modal error message (used when RunBay cannot start).
pub fn error_box(title: &str, text: &str) {
    // SAFETY: plain MessageBoxW call with valid strings.
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(title),
            MB_OK | MB_ICONERROR,
        );
    }
}
