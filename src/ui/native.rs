//! Native-window readiness: Slint can defer HWND creation until its event loop starts.

use slint::winit_030::{EventResult, WinitWindowAccessor};

use super::convert;

/// Runs setup on the first native event, before Slint processes that event.
/// A recreated native window gets its own setup; ordinary events do no extra work.
pub(super) fn on_created(window: &slint::Window, setup: impl Fn(isize) + 'static) {
    let mut initialized = None;
    window.on_winit_window_event(move |window, _| {
        if let Some(id) = window.with_winit_window(|native| native.id())
            && initialized != Some(id)
            && let Some(hwnd) = convert::hwnd(window)
        {
            // Mark first: setup can cause more native size/style messages.
            initialized = Some(id);
            setup(hwnd);
        }
        EventResult::Propagate
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use crate::platform::{self, chrome, chrome::Kind};
    use crate::{CompactWindow, MainWindow};
    use slint::ComponentHandle;
    use slint::winit_030::winit::platform::windows::EventLoopBuilderExtWindows;
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::*;

    #[test]
    fn deferred_native_creation_applies_captions_without_a_settings_change() {
        let mut event_loop = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
        event_loop.with_any_thread(true);
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name("software".into())
            .with_winit_event_loop_builder(event_loop)
            .select()
            .unwrap();

        // Leave every window hidden: exercise the actual backend lifecycle without
        // taking desktop focus, generating taskbar buttons, or sending input.
        let full = [MainWindow::new().unwrap(), MainWindow::new().unwrap()];
        let compact = [CompactWindow::new().unwrap(), CompactWindow::new().unwrap()];
        let setups = Rc::new(Cell::new(0));
        for (kind, window, taskbar) in [
            (Kind::Full, full[0].window(), false),
            (Kind::Full, full[1].window(), true),
            (Kind::Compact, compact[0].window(), false),
            (Kind::Compact, compact[1].window(), true),
        ] {
            assert!(convert::hwnd(window).is_none());
            let setups = setups.clone();
            on_created(window, move |hwnd| {
                setups.set(setups.get() + 1);
                chrome::install(hwnd, kind, |_| {});
                platform::set_taskbar_button(hwnd, taskbar);
            });
        }
        let verified = Rc::new(Cell::new(false));
        let done = verified.clone();
        let count = setups.clone();
        slint::Timer::single_shot(Duration::from_millis(300), move || {
            assert_eq!(
                count.get(),
                4,
                "setup must run after deferred HWND creation"
            );
            for (kind, window, taskbar) in [
                (Kind::Full, full[0].window(), false),
                (Kind::Full, full[1].window(), true),
                (Kind::Compact, compact[0].window(), false),
                (Kind::Compact, compact[1].window(), true),
            ] {
                let hwnd = HWND(convert::hwnd(window).unwrap() as *mut _);
                // SAFETY: these HWNDs belong to this thread, and stay hidden.
                unsafe {
                    assert!(!IsWindowVisible(hwnd).as_bool());
                    let mut bounds = RECT::default();
                    GetWindowRect(hwnd, &mut bounds).unwrap();
                    let mut origin = POINT::default();
                    assert!(ClientToScreen(hwnd, &mut origin).as_bool());
                    assert_eq!(origin.y, bounds.top, "native caption must be removed");
                    assert!(origin.x > bounds.left, "native resize frame must remain");
                    assert_eq!(
                        GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as isize,
                        0
                    );
                    assert_eq!(GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT) == 0, taskbar);
                    if kind == Kind::Compact {
                        assert_eq!(
                            GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_MAXIMIZEBOX.0 as isize,
                            0
                        );
                    }
                }
                window.set_size(slint::PhysicalSize::new(900, 600));
            }
            assert_eq!(count.get(), 4, "ordinary events must not rerun setup");
            done.set(true);
            slint::quit_event_loop().unwrap();
        });
        slint::run_event_loop_until_quit().unwrap();
        assert!(verified.get());
    }
}
