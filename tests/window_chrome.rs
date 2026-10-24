//! Caption rendering and controls using software windows, without desktop input.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use runbay::{CompactWindow, MainWindow, MiniBridge, SettingsBridge};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, PhysicalSize, Rgb8Pixel};

struct Offscreen(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}

fn render(window: &MinimalSoftwareWindow, name: &str) -> Vec<Rgb8Pixel> {
    let size = window.size();
    let mut pixels = vec![Rgb8Pixel::default(); (size.width * size.height) as usize];
    window.request_redraw();
    assert!(window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, size.width as usize);
    }));
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.artefacts/RunBay/gui/custom-window-chrome");
    std::fs::create_dir_all(&folder).unwrap();
    let bytes: Vec<_> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    image::save_buffer(
        folder.join(format!("{name}.png")),
        &bytes,
        size.width,
        size.height,
        image::ColorType::Rgb8,
    )
    .unwrap();
    pixels
}

fn click(window: &MinimalSoftwareWindow, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

#[test]
fn captions_render_in_both_themes_and_follow_layout_and_dpi() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(Offscreen(windows.clone()))).unwrap();
    let full = MainWindow::new().unwrap();
    let full_window = windows.borrow().last().unwrap().clone();
    full_window.set_size(PhysicalSize::new(1100, 736));
    full.show().unwrap();
    let compact = CompactWindow::new().unwrap();
    let compact_window = windows.borrow().last().unwrap().clone();
    compact_window.set_size(PhysicalSize::new(360, 420));
    compact.show().unwrap();

    for theme in [1, 2] {
        full.invoke_set_theme(theme);
        compact.invoke_set_theme(theme);
        let mut previous = None;
        for active in [true, false] {
            full.set_window_active(active);
            compact.set_window_active(active);
            let pixels = render(&full_window, &format!("full-{theme}-{active}"));
            // Inactive title and icons must actually change the rendered caption.
            if let Some(previous) = previous {
                assert!(
                    pixels[..1100 * 36] != previous,
                    "inactive caption must be dimmed"
                );
            }
            previous = Some(pixels[..1100 * 36].to_vec());
            render(&compact_window, &format!("compact-{theme}-{active}"));
        }
    }

    let minimize = Rc::new(Cell::new(0));
    let close = Rc::new(Cell::new(0));
    let maximize = Rc::new(Cell::new(0));
    let count = minimize.clone();
    full.on_caption_minimize(move || count.set(count.get() + 1));
    let count = close.clone();
    full.on_caption_close(move || count.set(count.get() + 1));
    let count = maximize.clone();
    full.on_caption_maximize(move || count.set(count.get() + 1));
    for scale in [1.0, 1.5, 2.0] {
        full_window.dispatch_event(WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
        full_window.set_size(PhysicalSize::new(
            (900.0 * scale) as u32,
            (636.0 * scale) as u32,
        ));
        render(&full_window, &format!("full-scale-{scale}"));
        assert_eq!(full.get_caption_height(), 36.0);
        assert_eq!(full.get_drag_end(), 900.0 - 138.0);
        assert_eq!(full.get_max_button_x(), 900.0 - 92.0);
        assert_eq!(full.get_max_button_width(), 46.0);
        click(&full_window, full.get_drag_end() + 23.0, 18.0);
        click(&full_window, 900.0 - 23.0, 18.0);
    }
    assert_eq!(minimize.get(), 3);
    assert_eq!(close.get(), 3);
    full.invoke_caption_maximize(); // Accessibility/keyboard path; native pointer hits are Win32's.
    assert_eq!(maximize.get(), 1);
    full.set_window_maximized(true);
    full.set_max_hover(true);
    full.set_max_pressed(true);
    render(&full_window, "full-restore-pressed");

    let count = minimize.clone();
    compact.on_caption_minimize(move || count.set(count.get() + 1));
    let count = close.clone();
    compact.on_caption_close(move || count.set(count.get() + 1));
    render(&compact_window, "compact-controls");
    assert_eq!(compact.get_caption_height(), 48.0);
    let drag_end = compact.get_drag_end();
    compact
        .global::<MiniBridge>()
        .set_agents("Connected agent".into());
    render(&compact_window, "compact-agent");
    assert!(
        compact.get_drag_end() < drag_end,
        "agent indicator must stay outside the drag area"
    );
    click(&compact_window, 360.0 - 8.0 - 32.0 - 2.0 - 16.0, 24.0);
    click(&compact_window, 360.0 - 8.0 - 16.0, 24.0);
    assert_eq!(minimize.get(), 4);
    assert_eq!(close.get(), 4);

    let themes = Rc::new(Cell::new(0));
    let count = themes.clone();
    compact.on_cycle_theme(move || count.set(count.get() + 1));
    for theme in [0, 1, 2] {
        compact.set_theme_mode(theme);
        compact.invoke_set_theme(theme);
        render(&compact_window, &format!("compact-theme-{theme}"));
        // At minimum width, with an agent connected, every header control fits.
        click(&compact_window, 360.0 - 8.0 - 5.0 * 34.0 - 16.0, 24.0);
    }
    assert_eq!(themes.get(), 3);

    full_window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    full_window.set_size(PhysicalSize::new(1100, 900));
    full.set_window_active(true);
    full.set_max_hover(false);
    full.set_max_pressed(false);
    full_window.dispatch_event(WindowEvent::PointerExited);
    full.set_page("settings".into());
    let settings = full.global::<SettingsBridge>();
    settings.set_top_compact(true);
    settings.set_top_icon(false);
    settings.set_taskbar_full(true);
    settings.set_taskbar_compact(false);
    for theme in [1, 2] {
        full.invoke_set_theme(theme);
        render(&full_window, &format!("settings-toggles-{theme}"));
        std::thread::sleep(std::time::Duration::from_millis(150));
        slint::platform::update_timers_and_animations();
        render(&full_window, &format!("settings-toggles-{theme}"));
    }

    let changes = Rc::new(Cell::new(0));
    let count = changes.clone();
    settings.on_changed(move || count.set(count.get() + 1));
    click(&full_window, 238.0, 501.0);
    assert!(
        !settings.get_top_compact(),
        "pointer toggles the saved binding"
    );
    full_window.dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
    full_window.dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
    assert!(
        settings.get_top_compact(),
        "Space toggles the focused switch"
    );
    assert_eq!(changes.get(), 2, "each toggle requests one automatic save");
}
