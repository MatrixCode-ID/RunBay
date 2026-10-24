//! Native file and folder pickers (`IFileOpenDialog`).

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows::core::{HSTRING, PCWSTR};

/// Shows the picker; `filter` is `(name, "*.png;*.ico")` for files, `None` for folders.
/// Returns `None` when cancelled.
fn pick(owner: Option<isize>, title: &str, filter: Option<(&str, &str)>) -> Option<PathBuf> {
    // SAFETY: COM calls on the UI thread; strings outlive the calls that use them.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let mut options = dialog.GetOptions().ok()? | FOS_FORCEFILESYSTEM;
        let (name, spec) = filter.map_or((HSTRING::new(), HSTRING::new()), |(n, s)| {
            (HSTRING::from(n), HSTRING::from(s))
        });
        if filter.is_some() {
            let specs = [COMDLG_FILTERSPEC {
                pszName: PCWSTR(name.as_ptr()),
                pszSpec: PCWSTR(spec.as_ptr()),
            }];
            dialog.SetFileTypes(&specs).ok()?;
        } else {
            options |= FOS_PICKFOLDERS;
        }
        dialog.SetOptions(options).ok()?;
        dialog.SetTitle(&HSTRING::from(title)).ok()?;
        dialog.Show(owner.map(|h| HWND(h as *mut _))).ok()?;
        let item = dialog.GetResult().ok()?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map(PathBuf::from)
    }
}

/// Lets the user pick a folder.
pub fn pick_folder(owner: Option<isize>, title: &str) -> Option<PathBuf> {
    pick(owner, title, None)
}

/// Lets the user pick a file matching `spec` (e.g. `*.png;*.ico;*.svg`).
pub fn pick_file(owner: Option<isize>, title: &str, name: &str, spec: &str) -> Option<PathBuf> {
    pick(owner, title, Some((name, spec)))
}
