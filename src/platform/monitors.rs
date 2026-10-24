//! Monitor work areas and moving a window rectangle back onto a visible monitor.

use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::core::BOOL;

use crate::data::WindowRect;

/// A work area as `(left, top, right, bottom)`.
pub type Area = (i32, i32, i32, i32);

/// Work areas (screen minus taskbar) of all monitors.
pub fn work_areas() -> Vec<Area> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        // SAFETY: `data` points to the Vec passed below; the monitor handle is valid here.
        unsafe {
            let areas = &mut *(data.0 as *mut Vec<Area>);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                let r = info.rcWork;
                areas.push((r.left, r.top, r.right, r.bottom));
            }
        }
        true.into()
    }
    let mut areas: Vec<Area> = Vec::new();
    // SAFETY: the callback only runs during this call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut areas as *mut _ as isize),
        );
    }
    areas
}

/// Returns `rect` when at least a 64×32 px part of it is on a monitor; otherwise moves it
/// into the nearest monitor's work area (shrinking it when it is larger than that area).
pub fn fit(rect: WindowRect, areas: &[Area]) -> WindowRect {
    if areas.is_empty() {
        return rect;
    }
    let (w, h) = (rect.width as i32, rect.height as i32);
    let visible = areas.iter().any(|&(l, t, r, b)| {
        let ix = (rect.x + w).min(r) - rect.x.max(l);
        let iy = (rect.y + h).min(b) - rect.y.max(t);
        ix >= 64.min(w) && iy >= 32.min(h)
    });
    if visible {
        return rect;
    }
    let (cx, cy) = (rect.x + w / 2, rect.y + h / 2);
    let distance = |&(l, t, r, b): &Area| {
        let dx = (l - cx).max(0).max(cx - r);
        let dy = (t - cy).max(0).max(cy - b);
        (dx as i64).pow(2) + (dy as i64).pow(2)
    };
    let &(l, t, r, b) = areas.iter().min_by_key(|a| distance(a)).unwrap();
    let width = w.min(r - l).max(1);
    let height = h.min(b - t).max(1);
    WindowRect {
        x: rect.x.clamp(l, r - width),
        y: rect.y.clamp(t, b - height),
        width: width as u32,
        height: height as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREAS: [Area; 2] = [(0, 0, 1920, 1040), (1920, 0, 3840, 1040)];

    fn rect(x: i32, y: i32, width: u32, height: u32) -> WindowRect {
        WindowRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn visible_rect_is_kept() {
        let r = rect(100, 100, 800, 600);
        assert_eq!(fit(r, &AREAS), r);
        let on_second = rect(2000, 50, 800, 600);
        assert_eq!(fit(on_second, &AREAS), on_second);
    }

    #[test]
    fn off_screen_rect_moves_to_the_nearest_monitor() {
        // A monitor on the right that was unplugged.
        assert_eq!(
            fit(rect(5000, 200, 800, 600), &AREAS),
            rect(3040, 200, 800, 600)
        );
        assert_eq!(
            fit(rect(100, -2000, 800, 600), &AREAS),
            rect(100, 0, 800, 600)
        );
    }

    #[test]
    fn too_large_rect_is_shrunk() {
        assert_eq!(
            fit(rect(-5000, -5000, 4000, 3000), &AREAS),
            rect(0, 0, 1920, 1040)
        );
    }
}
