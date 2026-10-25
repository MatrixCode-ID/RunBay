//! Offscreen GUI checks: no native windows, desktop input, or focus changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use runbay::{
    AppRow, CompactWindow, IconPopup, MainWindow, MiniBridge, OutputBridge, OutputRow, Span, Theme,
};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, Rgb8Pixel, VecModel};

struct OffscreenPlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);

impl Platform for OffscreenPlatform {
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
    if let Some(folder) = std::env::var_os("RUNBAY_GUI_ARTIFACT_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        let rgb: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        image::save_buffer(
            folder.join(name.to_lowercase().replace(' ', "-") + ".png"),
            &rgb,
            size.width,
            size.height,
            image::ColorType::Rgb8,
        )
        .unwrap();
    }
    pixels
}

fn color_count(pixels: &[Rgb8Pixel], color: slint::Color) -> usize {
    pixels
        .iter()
        .filter(|p| (p.r, p.g, p.b) == (color.red(), color.green(), color.blue()))
        .count()
}

fn click(window: &MinimalSoftwareWindow, x: f32, y: f32) {
    for event in [
        WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        },
    ] {
        window.dispatch_event(event);
    }
}

#[test]
fn console_palettes_render_independently_in_full_and_output_popups() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(OffscreenPlatform(windows.clone()))).unwrap();
    let full = MainWindow::new().unwrap();
    let full_window = windows.borrow().last().unwrap().clone();
    full_window.set_size(PhysicalSize::new(1100, 700));
    let rows = Rc::new(VecModel::from(vec![OutputRow {
        plain_text: "Selected ANSI text".into(),
        selected_from: 0,
        selected_to: 8,
        spans: ModelRc::new(VecModel::from(vec![Span {
            text: "Selected ANSI text".into(),
            color: slint::Color::from_rgb_u8(192, 28, 40),
            dark_color: slint::Color::from_rgb_u8(246, 97, 81),
            colored: true,
            ..Span::default()
        }])),
        ..OutputRow::default()
    }]));
    full.set_output(ModelRc::from(rows.clone()));
    full.set_output_columns(18);
    full.show().unwrap();
    for ui_mode in [1, 2] {
        full.invoke_set_theme(ui_mode);
        for console_mode in [0, 1, 2] {
            let theme = full.global::<Theme>();
            theme.set_console_mode(console_mode);
            let dark = console_mode == 2 || (console_mode == 0 && ui_mode == 2);
            assert_eq!(theme.get_dark(), ui_mode == 2);
            assert_eq!(theme.get_console_dark(), dark);
            let name = format!("full-ui-{ui_mode}-console-{console_mode}");
            let pixels = render(&full_window, &name);
            assert!(color_count(&pixels, theme.get_console_background()) > 100_000);
            assert!(color_count(&pixels, theme.get_console_selection()) > 100);
            let span = rows.row_data(0).unwrap().spans.row_data(0).unwrap();
            assert!(color_count(&pixels, if dark { span.dark_color } else { span.color }) > 5);
            assert_eq!(rows.row_data(0).unwrap().selected_to, 8);
        }
    }

    // The full 20 px scrollbar lane must drag without starting text selection,
    // including above/below the visible 8 px thumb and with very long lines.
    full.set_output_columns(1000);
    let selection_events = Rc::new(Cell::new(0));
    let calls = selection_events.clone();
    full.global::<OutputBridge>()
        .on_selection_event(move |_, _, _, _, _| calls.set(calls.get() + 1));
    for y in [669.0, 678.0, 687.0] {
        full.invoke_scroll_to_end();
        let before = render(&full_window, "scrollbar-before-drag");
        full_window.dispatch_event(WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(350.0, y),
            button: PointerEventButton::Left,
        });
        full_window.dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(650.0, y),
        });
        full_window.dispatch_event(WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(650.0, y),
            button: PointerEventButton::Left,
        });
        let after = render(&full_window, "scrollbar-after-drag");
        let span = rows.row_data(0).unwrap().spans.row_data(0).unwrap();
        assert!(color_count(&before, span.dark_color) > 5);
        assert_eq!(
            color_count(&after, span.dark_color),
            0,
            "Scrollbar must move text at y={y}"
        );
        assert_eq!(
            selection_events.get(),
            0,
            "Scrollbar must not select text at y={y}"
        );
    }
    full.set_output_columns(18);
    full.invoke_scroll_to_end();

    let compact = CompactWindow::new().unwrap();
    let compact_window = windows.borrow().last().unwrap().clone();
    compact_window.set_size(PhysicalSize::new(360, 420));
    let icon_popup = IconPopup::new().unwrap();
    let icon_window = windows.borrow().last().unwrap().clone();
    icon_window.set_size(PhysicalSize::new(320, 420));
    let apps = ModelRc::new(VecModel::from(vec![AppRow {
        id: "preview".into(),
        name: "Preview".into(),
        initial: "P".into(),
        ..AppRow::default()
    }]));
    for bridge in [
        compact.global::<MiniBridge>(),
        icon_popup.global::<MiniBridge>(),
    ] {
        bridge.set_apps(apps.clone());
        bridge.set_output(ModelRc::from(rows.clone()));
        bridge.set_output_columns(18);
        bridge.set_output_title("Preview".into());
    }
    compact.invoke_set_theme(1);
    icon_popup.invoke_set_theme(1);
    compact.show().unwrap();
    icon_popup.show().unwrap();
    render(&compact_window, "compact-apps");
    render(&icon_window, "icon-apps");
    // The message action is in the app row, before the output popup is opened.
    click(&compact_window, 327.0, 86.0);
    click(&icon_window, 277.0, 58.0);
    for console_mode in [0, 1, 2] {
        for (window, theme, name) in [
            (&compact_window, compact.global::<Theme>(), "compact"),
            (&icon_window, icon_popup.global::<Theme>(), "icon"),
        ] {
            theme.set_console_mode(console_mode);
            let pixels = render(window, &format!("{name}-console-{console_mode}"));
            assert!(!theme.get_dark());
            assert_eq!(theme.get_console_dark(), console_mode == 2);
            assert!(
                color_count(&pixels, theme.get_console_background()) > 10_000,
                "{name} output popup should display the console palette"
            );
        }
    }

    // Dispatch directly into the software windows to verify the real hit targets.
    // This never sends operating-system input or changes desktop focus.
    for (window, bridge, x, y) in [
        (&full_window, full.global::<OutputBridge>(), 968.0, 66.0),
        (
            &compact_window,
            compact.global::<OutputBridge>(),
            329.0,
            112.0,
        ),
        (
            &icon_window,
            icon_popup.global::<OutputBridge>(),
            279.0,
            74.0,
        ),
    ] {
        let clicks = Rc::new(Cell::new(0));
        let count = clicks.clone();
        bridge.on_cycle_console_theme(move || count.set(count.get() + 1));
        click(window, x, y);
        assert_eq!(clicks.get(), 1, "console theme button must be clickable");
    }
}
