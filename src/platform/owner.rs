//! One never-shown top-level owner for regular windows without taskbar buttons.

use std::cell::Cell;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::w;

thread_local! {
    static OWNER: Cell<isize> = const { Cell::new(0) };
}

unsafe extern "system" fn owner_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: forwards the original window message to the default procedure.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

pub(super) fn hidden_owner() -> Option<isize> {
    OWNER.with(|owner| {
        if owner.get() != 0 {
            return Some(owner.get());
        }
        // SAFETY: UI thread only; the registered class and owner live until process exit.
        // This is a real top-level window, never shown/minimized, not HWND_MESSAGE.
        let result = unsafe {
            GetModuleHandleW(None).and_then(|module| {
                let instance = module.into();
                let class = w!("RunBay.HiddenOwner");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(owner_proc),
                    hInstance: instance,
                    lpszClassName: class,
                    ..Default::default()
                };
                RegisterClassW(&wc);
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    class,
                    w!(""),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(instance),
                    None,
                )
            })
        };
        match result {
            Ok(hwnd) => {
                owner.set(hwnd.0 as isize);
                Some(owner.get())
            }
            Err(error) => {
                log::error!("cannot create the hidden taskbar owner: {error}");
                None
            }
        }
    })
}
