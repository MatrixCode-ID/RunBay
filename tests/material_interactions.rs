//! Exercises real filter and keyboard bindings without native windows or input.

use std::cell::RefCell;
use std::rc::Rc;

use runbay::{HistoryBridge, HistoryRow, MainWindow, Theme};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, PhysicalSize, Rgb8Pixel, VecModel};

struct Offscreen(Rc<MinimalSoftwareWindow>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn render(window: &MinimalSoftwareWindow) -> Vec<Rgb8Pixel> {
    let mut pixels = vec![Rgb8Pixel::default(); 1100 * 736];
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1100);
    });
    pixels
}

fn click(window: &MinimalSoftwareWindow, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    for event in [
        WindowEvent::PointerMoved { position },
        WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        },
    ] {
        window.dispatch_event(event);
    }
}

fn key(window: &MinimalSoftwareWindow, key: slint::platform::Key) {
    let text: slint::SharedString = key.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn chips_keep_filter_indices_and_history_rows_support_keyboard_selection() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let full = MainWindow::new().unwrap();
    window.set_size(PhysicalSize::new(1100, 736));
    full.invoke_set_theme(1);
    full.set_history_open(true);
    let history = full.global::<HistoryBridge>();
    history.set_app_filters(ModelRc::new(VecModel::from(vec!["All apps".into()])));
    history.set_action_filters(ModelRc::new(VecModel::from(vec!["All actions".into()])));
    history.set_rows(ModelRc::new(VecModel::from(vec![
        HistoryRow {
            run_id: "first".into(),
            title: "First run".into(),
            status: "success".into(),
            ..HistoryRow::default()
        },
        HistoryRow {
            run_id: "second".into(),
            title: "Second run".into(),
            status: "failed".into(),
            ..HistoryRow::default()
        },
    ])));
    let opened = Rc::new(RefCell::new(Vec::new()));
    let calls = opened.clone();
    history.on_open_run(move |id| calls.borrow_mut().push(id.to_string()));
    full.show().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(400));
    slint::platform::update_timers_and_animations();
    render(&window);

    // Status indices remain 1..4; each selected chip toggles back to 0.
    for (index, x) in [(1, 752.0), (2, 830.0), (3, 900.0), (4, 971.0)] {
        click(&window, x, 151.0);
        assert_eq!(history.get_status_filter(), index);
        render(&window);
        click(&window, x, 151.0);
        assert_eq!(history.get_status_filter(), 0);
        render(&window);
    }
    for (index, x) in [(1, 750.0), (2, 820.0)] {
        click(&window, x, 191.0);
        assert_eq!(history.get_source_filter(), index);
        render(&window);
        click(&window, x, 191.0);
        assert_eq!(history.get_source_filter(), 0);
        render(&window);
    }

    // Tab reaches the first History row; its inward secondary ring is visible.
    // Do not activate intermediate controls, including the native dropdowns.
    let secondary = full.global::<Theme>().get_secondary();
    let mut reached_row = false;
    for _ in 0..50 {
        key(&window, slint::platform::Key::Tab);
        std::thread::sleep(std::time::Duration::from_millis(130));
        slint::platform::update_timers_and_animations();
        let pixels = render(&window);
        let ring_pixels = (216..260)
            .flat_map(|y| (724..1076).map(move |x| y * 1100 + x))
            .filter(|&i| {
                let p = pixels[i];
                (p.r, p.g, p.b) == (secondary.red(), secondary.green(), secondary.blue())
            })
            .count();
        if ring_pixels > 200 {
            reached_row = true;
            break;
        }
    }
    assert!(reached_row, "Tab must reach a visibly focused History row");
    key(&window, slint::platform::Key::Return);
    assert_eq!(opened.borrow().last().unwrap(), "first");
    key(&window, slint::platform::Key::DownArrow);
    assert_eq!(opened.borrow().last().unwrap(), "second");
    key(&window, slint::platform::Key::UpArrow);
    assert_eq!(opened.borrow().last().unwrap(), "first");
    key(&window, slint::platform::Key::Space);
    assert_eq!(opened.borrow().last().unwrap(), "first");
}
