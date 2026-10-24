//! Custom captions over the native frame. All state and Win32 calls stay on the UI thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

const SUBCLASS_ID: usize = 0x52424348;

/// The two regular windows that draw custom captions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Full has the native maximize hit target and a three-button caption.
    Full,
    /// Compact cannot maximize and uses its existing header as the caption.
    Compact,
}

/// Title-row bounds in client coordinates; `scaled` converts logical to physical pixels.
#[derive(Clone, Copy, Default)]
pub struct Metrics {
    /// Bottom of the title row.
    pub height: f32,
    /// First interactive control (including the Compact agent indicator).
    pub drag_end: f32,
    /// Left of the Full maximize button.
    pub max_x: f32,
    /// Width of the Full maximize button; zero in Compact.
    pub max_width: f32,
}

impl Metrics {
    /// Converts every bound with the same DPI scale used by Slint.
    pub fn scaled(self, scale: f32) -> Self {
        Self {
            height: self.height * scale,
            drag_end: self.drag_end * scale,
            max_x: self.max_x * scale,
            max_width: self.max_width * scale,
        }
    }
}

/// Caption appearance sent back to Slint without querying the UI during hit testing.
#[derive(Clone, Copy, Default)]
pub struct Appearance {
    /// Whether this window is active.
    pub active: bool,
    /// Whether Full currently fills the monitor work area.
    pub maximized: bool,
    /// Native hover over Full's maximize button.
    pub hover: bool,
    /// Native press on Full's maximize button.
    pub pressed: bool,
}

/// Messages for the UI bridge; callbacks must defer UI work until after the native call.
#[derive(Clone, Copy)]
pub enum Event {
    /// Apply caption colors and maximize state.
    Appearance(Appearance),
    /// Refresh layout bounds after size or DPI changes.
    MetricsChanged,
    /// Apply the current taskbar-dependent minimize policy.
    Minimize,
}

#[derive(Clone)]
struct State {
    kind: Kind,
    logical: Metrics,
    physical: Metrics,
    appearance: Appearance,
    callback: Rc<dyn Fn(Event)>,
}

thread_local! {
    static WINDOWS: RefCell<HashMap<isize, State>> = RefCell::new(HashMap::new());
}

fn state(hwnd: HWND) -> Option<State> {
    WINDOWS.with(|windows| windows.borrow().get(&(hwnd.0 as isize)).cloned())
}

fn update(hwnd: HWND, change: impl FnOnce(&mut State)) {
    WINDOWS.with(|windows| {
        if let Some(state) = windows.borrow_mut().get_mut(&(hwnd.0 as isize)) {
            change(state);
        }
    });
}

fn appearance(hwnd: HWND, change: impl FnOnce(&mut Appearance)) {
    update(hwnd, |state| change(&mut state.appearance));
    if let Some(state) = state(hwnd) {
        (state.callback)(Event::Appearance(state.appearance));
    }
}

/// Chooses the minimize behavior; hiding avoids an unreachable minimized stub.
pub fn minimize_to_tray(taskbar_visible: bool) -> bool {
    !taskbar_visible
}

/// Pure hit testing inside the client rectangle. Side/bottom frame hits stay native.
pub fn hit_test(
    point: (i32, i32),
    size: (i32, i32),
    metrics: Metrics,
    maximized: bool,
    border: i32,
) -> u32 {
    let (x, y) = point;
    let (width, height) = size;
    if x < 0 || y < 0 || x >= width || y >= height {
        return HTCLIENT;
    }
    if !maximized && y < border {
        return if x < border {
            HTTOPLEFT
        } else if x >= width - border {
            HTTOPRIGHT
        } else {
            HTTOP
        };
    }
    if (y as f32) < metrics.height {
        if (x as f32) >= metrics.max_x && (x as f32) < metrics.max_x + metrics.max_width {
            return HTMAXBUTTON;
        }
        if (x as f32) < metrics.drag_end {
            return HTCAPTION;
        }
    }
    HTCLIENT
}

/// Installs the subclass once; reapplies Compact styles after backend style changes.
pub fn install(hwnd: isize, kind: Kind, callback: impl Fn(Event) + 'static) {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: the caller supplies a live window on its owning UI thread. No RefCell
    // borrow spans a Win32 call, since those calls can reenter the subclass.
    unsafe {
        if state(hwnd).is_none() {
            let state = State {
                kind,
                logical: Metrics::default(),
                physical: Metrics::default(),
                appearance: Appearance {
                    active: GetForegroundWindow() == hwnd,
                    maximized: IsZoomed(hwnd).as_bool(),
                    ..Default::default()
                },
                callback: Rc::new(callback),
            };
            WINDOWS.with(|windows| windows.borrow_mut().insert(hwnd.0 as isize, state));
            if !SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, 0).as_bool() {
                WINDOWS.with(|windows| windows.borrow_mut().remove(&(hwnd.0 as isize)));
                log::error!("cannot install the custom window caption");
                return;
            }
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &DWMWCP_ROUND as *const _ as *const _,
                size_of_val(&DWMWCP_ROUND) as u32,
            );
            refresh_frame(hwnd);
            appearance(hwnd, |_| {});
        }
        if kind == Kind::Compact {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            if style & WS_MAXIMIZEBOX.0 as isize != 0 {
                SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_MAXIMIZEBOX.0 as isize));
                refresh_frame(hwnd);
            }
        }
    }
}

unsafe fn refresh_frame(hwnd: HWND) {
    // SAFETY: called on the window's UI thread; no geometry or activation change.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Updates cached layout bounds after Slint layout or a scale change, without polling.
pub fn set_metrics(hwnd: isize, logical: Metrics, scale: f32) {
    update(HWND(hwnd as *mut _), |state| {
        state.logical = logical;
        state.physical = logical.scaled(scale);
    });
}

/// Posts the normal close request so the existing save-and-hide handler is preserved.
pub fn close(hwnd: isize) {
    // SAFETY: posting to this process's window; stale handles only fail the post.
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

/// Minimizes to the taskbar, or uses the regular close-to-tray path when it is hidden.
pub fn minimize(hwnd: isize, taskbar_visible: bool) {
    if minimize_to_tray(taskbar_visible) {
        close(hwnd);
    } else {
        // SAFETY: the caller owns this window.
        unsafe {
            let _ = ShowWindow(HWND(hwnd as *mut _), SW_MINIMIZE);
        }
    }
}

/// Toggles Full's native maximized state, also used by accessibility activation.
pub fn toggle_maximize(hwnd: isize) {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: the caller owns this window. ShowWindow preserves the native work area.
    unsafe {
        let command = if IsZoomed(hwnd).as_bool() {
            SW_RESTORE
        } else {
            SW_MAXIMIZE
        };
        let _ = ShowWindow(hwnd, command);
    }
}

unsafe fn frame_border(hwnd: HWND) -> i32 {
    // SAFETY: plain DPI/metric queries for the live window.
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
    }
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    // SAFETY: Win32 supplies the message-specific pointers; they are only read for
    // their matching message. Unhandled messages always reach winit's procedure.
    unsafe {
        match message {
            WM_STYLECHANGING
                if wparam.0 as i32 == GWL_STYLE.0
                    && state(hwnd).is_some_and(|s| s.kind == Kind::Compact) =>
            {
                let result = DefSubclassProc(hwnd, message, wparam, lparam);
                // Keep backend style refreshes from restoring Compact's maximize box.
                let styles = lparam.0 as *mut STYLESTRUCT;
                (*styles).styleNew &= !WS_MAXIMIZEBOX.0;
                return result;
            }
            WM_NCCALCSIZE => {
                let rect = if wparam.0 != 0 {
                    &mut (*(lparam.0 as *mut NCCALCSIZE_PARAMS)).rgrc[0]
                } else {
                    &mut *(lparam.0 as *mut RECT)
                };
                let top = rect.top;
                let result = DefSubclassProc(hwnd, message, wparam, lparam);
                rect.top = top
                    + if IsZoomed(hwnd).as_bool() {
                        frame_border(hwnd)
                    } else {
                        0
                    };
                // The changed client origin invalidates DefWindowProc's valid-rect copy.
                return if wparam.0 != 0 { LRESULT(0) } else { result };
            }
            WM_NCHITTEST => {
                let native = DefSubclassProc(hwnd, message, wparam, lparam);
                let mut point = POINT {
                    x: lparam.0 as i16 as i32,
                    y: (lparam.0 >> 16) as i16 as i32,
                };
                let _ = ScreenToClient(hwnd, &mut point);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                if point.x < 0 || point.y < 0 || point.x >= rect.right || point.y >= rect.bottom {
                    return native;
                }
                if let Some(state) = state(hwnd) {
                    return LRESULT(hit_test(
                        (point.x, point.y),
                        (rect.right, rect.bottom),
                        state.physical,
                        IsZoomed(hwnd).as_bool(),
                        frame_border(hwnd),
                    ) as isize);
                }
            }
            WM_NCMOUSEMOVE if wparam.0 as u32 == HTMAXBUTTON => {
                if state(hwnd).is_some_and(|s| !s.appearance.hover) {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        ..Default::default()
                    };
                    let _ = TrackMouseEvent(&mut track);
                    appearance(hwnd, |a| a.hover = true);
                }
                // Let DefWindowProc see HTMAXBUTTON for the Windows 11 Snap flyout.
            }
            WM_NCMOUSEMOVE => {
                if state(hwnd).is_some_and(|s| s.appearance.hover) {
                    appearance(hwnd, |a| {
                        a.hover = false;
                        a.pressed = false;
                    });
                }
            }
            WM_NCMOUSELEAVE | WM_CANCELMODE | WM_CAPTURECHANGED => {
                appearance(hwnd, |a| {
                    a.hover = false;
                    a.pressed = false;
                });
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wparam.0 as u32 == HTMAXBUTTON => {
                appearance(hwnd, |a| a.pressed = true);
                return LRESULT(0);
            }
            WM_NCLBUTTONUP if wparam.0 as u32 == HTMAXBUTTON => {
                let pressed = state(hwnd).is_some_and(|s| s.appearance.pressed);
                appearance(hwnd, |a| a.pressed = false);
                if pressed {
                    toggle_maximize(hwnd.0 as isize);
                }
                return LRESULT(0);
            }
            WM_SYSCOMMAND if wparam.0 as u32 & 0xfff0 == SC_MINIMIZE => {
                if let Some(state) = state(hwnd) {
                    (state.callback)(Event::Minimize);
                }
                return LRESULT(0);
            }
            WM_SYSCOMMAND
                if wparam.0 as u32 & 0xfff0 == SC_MAXIMIZE
                    && state(hwnd).is_some_and(|s| s.kind == Kind::Compact) =>
            {
                return LRESULT(0);
            }
            WM_ACTIVATE | WM_SIZE | WM_DPICHANGED => {
                let result = DefSubclassProc(hwnd, message, wparam, lparam);
                if message == WM_ACTIVATE {
                    appearance(hwnd, |a| a.active = wparam.0 as u16 != WA_INACTIVE as u16);
                } else {
                    appearance(hwnd, |a| a.maximized = IsZoomed(hwnd).as_bool());
                    update(hwnd, |s| {
                        s.physical = s.logical.scaled(GetDpiForWindow(hwnd) as f32 / 96.0)
                    });
                    if let Some(state) = state(hwnd) {
                        (state.callback)(Event::MetricsChanged);
                    }
                }
                return result;
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
                WINDOWS.with(|windows| windows.borrow_mut().remove(&(hwnd.0 as isize)));
            }
            _ => {}
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_hit_targets_follow_dpi_and_maximize_state() {
        for scale in [1.0, 1.5, 2.0] {
            let metrics = Metrics {
                height: 36.0,
                drag_end: 662.0,
                max_x: 708.0,
                max_width: 46.0,
            }
            .scaled(scale);
            let size = ((800.0 * scale) as i32, (600.0 * scale) as i32);
            let border = (8.0 * scale) as i32;
            let hit = |x: f32, y: f32, max| {
                hit_test(
                    ((x * scale) as i32, (y * scale) as i32),
                    size,
                    metrics,
                    max,
                    border,
                )
            };
            assert_eq!(hit(100.0, 20.0, false), HTCAPTION);
            assert_eq!(hit(680.0, 20.0, false), HTCLIENT); // Minimize
            assert_eq!(hit(730.0, 20.0, false), HTMAXBUTTON);
            assert_eq!(hit(780.0, 20.0, false), HTCLIENT); // Close
            assert_eq!(hit(100.0, 40.0, false), HTCLIENT);
            assert_eq!(hit(100.0, 2.0, false), HTTOP);
            assert_eq!(hit(2.0, 2.0, false), HTTOPLEFT);
            assert_eq!(hit(798.0, 2.0, false), HTTOPRIGHT);
            assert_eq!(hit(2.0, 2.0, true), HTCAPTION);
            assert_eq!(hit(730.0, 2.0, true), HTMAXBUTTON);
            assert_eq!(hit(780.0, 2.0, true), HTCLIENT);
            assert_eq!(hit(662.0, 20.0, false), HTCLIENT);
            assert_eq!(hit(754.0, 20.0, false), HTCLIENT);
            assert_eq!(hit(-1.0, 20.0, false), HTCLIENT);
        }
    }

    #[test]
    fn compact_controls_and_agent_indicator_are_client_hits() {
        let metrics = Metrics {
            height: 48.0,
            drag_end: 130.0,
            ..Default::default()
        };
        assert_eq!(hit_test((30, 24), (320, 420), metrics, false, 8), HTCAPTION);
        for x in [130, 160, 220, 280, 310] {
            assert_eq!(hit_test((x, 24), (320, 420), metrics, false, 8), HTCLIENT);
        }
    }

    #[test]
    fn minimize_policy_uses_the_taskbar_preference() {
        assert!(!minimize_to_tray(true));
        assert!(minimize_to_tray(false));
    }

    #[test]
    fn hidden_windows_keep_regular_styles_and_clean_up_the_subclass() {
        use std::cell::Cell;
        use windows::core::w;
        // SAFETY: these windows are created on this test thread, never shown, and
        // destroyed here. No activation, desktop input, or real taskbar changes.
        unsafe {
            for kind in [Kind::Full, Kind::Compact] {
                let hwnd = CreateWindowExW(
                    WS_EX_APPWINDOW,
                    w!("STATIC"),
                    w!(""),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    800,
                    600,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();
                let requests = Rc::new(Cell::new(0));
                let count = requests.clone();
                install(hwnd.0 as isize, kind, move |event| {
                    if matches!(event, Event::Minimize) {
                        count.set(count.get() + 1);
                    }
                });
                assert!(state(hwnd).is_some());
                if kind == Kind::Compact {
                    let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                    SetWindowLongPtrW(hwnd, GWL_STYLE, style | WS_MAXIMIZEBOX.0 as isize);
                    assert_eq!(
                        GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_MAXIMIZEBOX.0 as isize,
                        0
                    );
                    SendMessageW(
                        hwnd,
                        WM_SYSCOMMAND,
                        Some(WPARAM(SC_MAXIMIZE as usize)),
                        Some(LPARAM(0)),
                    );
                    assert!(!IsZoomed(hwnd).as_bool());
                }
                let mut previous_owner = 0;
                for visible in [false, true, false, true] {
                    crate::platform::set_taskbar_button(hwnd.0 as isize, visible);
                    let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    assert_eq!(ex_style & WS_EX_TOOLWINDOW.0 as isize, 0);
                    let owner = GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT);
                    if visible {
                        assert_eq!(owner, 0);
                        assert_ne!(ex_style & WS_EX_APPWINDOW.0 as isize, 0);
                    } else {
                        assert_ne!(owner, 0);
                        assert!(!IsWindowVisible(HWND(owner as *mut _)).as_bool());
                        assert_eq!(ex_style & WS_EX_APPWINDOW.0 as isize, 0);
                        if previous_owner != 0 {
                            assert_eq!(owner, previous_owner);
                        }
                        previous_owner = owner;
                    }
                    assert!(!IsWindowVisible(hwnd).as_bool());
                }
                SendMessageW(
                    hwnd,
                    WM_SYSCOMMAND,
                    Some(WPARAM(SC_MINIMIZE as usize)),
                    Some(LPARAM(0)),
                );
                assert_eq!(requests.get(), 1);
                assert!(!IsIconic(hwnd).as_bool());
                DestroyWindow(hwnd).unwrap();
                assert!(state(hwnd).is_none());
            }
        }
    }
}
