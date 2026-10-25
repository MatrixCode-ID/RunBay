//! Exercises real filter and keyboard bindings without native windows or input.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use runbay::{ActionButton, AppRow, HistoryBridge, HistoryRow, MainWindow, SettingsBridge, Theme};
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

fn snapshot(window: &MinimalSoftwareWindow, name: &str) -> Vec<Rgb8Pixel> {
    render(window);
    std::thread::sleep(std::time::Duration::from_millis(150));
    slint::platform::update_timers_and_animations();
    let pixels = render(window);
    let folder =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.artefacts/RunBay/gui/menus");
    std::fs::create_dir_all(&folder).unwrap();
    let bytes: Vec<_> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    image::save_buffer(
        folder.join(format!("{name}.png")),
        &bytes,
        1100,
        736,
        image::ColorType::Rgb8,
    )
    .unwrap();
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
    snapshot(&window, "history-filters");

    // Status indices remain 1..4; each selected chip toggles back to 0.
    for (index, x) in [(1, 752.0), (2, 830.0), (3, 900.0), (4, 971.0)] {
        click(&window, x, 175.0);
        assert_eq!(history.get_status_filter(), index);
        render(&window);
        click(&window, x, 175.0);
        assert_eq!(history.get_status_filter(), 0);
        render(&window);
    }
    for (index, x) in [(1, 750.0), (2, 820.0)] {
        click(&window, x, 215.0);
        assert_eq!(history.get_source_filter(), index);
        render(&window);
        click(&window, x, 215.0);
        assert_eq!(history.get_source_filter(), 0);
        render(&window);
    }

    // Tab reaches the first History row; its inward secondary ring is visible.
    // Do not activate intermediate controls, including the dropdowns.
    let secondary = full.global::<Theme>().get_secondary();
    let mut reached_row = false;
    for _ in 0..50 {
        key(&window, slint::platform::Key::Tab);
        std::thread::sleep(std::time::Duration::from_millis(130));
        slint::platform::update_timers_and_animations();
        let pixels = render(&window);
        let ring_pixels = (240..284)
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

    // Exercise the actual Settings selector and saved-index callback through
    // pointer and keyboard input, with the popup rendered by the software backend.
    full.set_history_open(false);
    full.set_page("settings".into());
    let settings = full.global::<SettingsBridge>();
    settings.set_theme_index(0);
    let changed = Rc::new(Cell::new(0));
    let calls = changed.clone();
    settings.on_changed(move || calls.set(calls.get() + 1));
    snapshot(&window, "settings-light");
    click(&window, 820.0, 280.0);
    let pixels = snapshot(&window, "theme-menu-light");
    let selected_color = full.global::<Theme>().get_tertiary_container();
    assert!(
        pixels
            .iter()
            .filter(|p| (p.r, p.g, p.b)
                == (
                    selected_color.red(),
                    selected_color.green(),
                    selected_color.blue()
                ))
            .count()
            > 1000,
        "the open menu must visibly mark the current choice"
    );
    key(&window, slint::platform::Key::DownArrow);
    key(&window, slint::platform::Key::Escape);
    assert_eq!(
        settings.get_theme_index(),
        0,
        "Escape must cancel navigation"
    );
    assert_eq!(changed.get(), 0);
    click(&window, 820.0, 280.0);
    key(&window, slint::platform::Key::End);
    key(&window, slint::platform::Key::Return);
    assert_eq!(settings.get_theme_index(), 2);
    assert_eq!(changed.get(), 1);
    full.invoke_set_theme(2);
    click(&window, 820.0, 280.0);
    snapshot(&window, "theme-menu-dark");
    click(&window, 400.0, 60.0);
    assert_eq!(settings.get_theme_index(), 2, "outside click must cancel");
    assert_eq!(changed.get(), 1);

    full.set_page("".into());
    full.set_history_open(true);
    history.set_app_filters(ModelRc::new(VecModel::from(
        (0..20)
            .map(|i| format!("Application {i}").into())
            .collect::<Vec<_>>(),
    )));
    let filters_changed = Rc::new(Cell::new(0));
    let calls = filters_changed.clone();
    history.on_filters_changed(move || calls.set(calls.get() + 1));
    std::thread::sleep(std::time::Duration::from_millis(400));
    slint::platform::update_timers_and_animations();
    render(&window);
    click(&window, 780.0, 120.0);
    key(&window, slint::platform::Key::End);
    snapshot(&window, "long-menu-last-choice");
    key(&window, slint::platform::Key::Return);
    assert_eq!(
        history.get_app_filter(),
        19,
        "End must reveal the final row"
    );
    assert_eq!(filters_changed.get(), 1);
    // Keyboard selection restores focus to the same field, allowing reopening.
    key(&window, slint::platform::Key::Space);
    key(&window, slint::platform::Key::Home);
    key(&window, slint::platform::Key::Return);
    assert_eq!(history.get_app_filter(), 0);
    assert_eq!(filters_changed.get(), 2);

    full.set_history_open(false);
    full.invoke_set_theme(1);
    full.set_apps(ModelRc::new(VecModel::from(vec![AppRow {
        id: "preview".into(),
        name: "Preview application".into(),
        initial: "P".into(),
        actions: ModelRc::new(VecModel::from(
            (0..10)
                .map(|i| ActionButton {
                    id: format!("action-{i}").into(),
                    name: format!("Action {i}").into(),
                    enabled: i != 9,
                    ..ActionButton::default()
                })
                .collect::<Vec<_>>(),
        )),
        ..AppRow::default()
    }])));
    let edits = Rc::new(Cell::new(0));
    let calls = edits.clone();
    full.on_edit_app(move |_| calls.set(calls.get() + 1));
    std::thread::sleep(std::time::Duration::from_millis(400));
    slint::platform::update_timers_and_animations();
    snapshot(&window, "app-menu-closed");
    click(&window, 270.0, 170.0);
    snapshot(&window, "app-menu-light");
    click(&window, 110.0, 202.0);
    snapshot(&window, "app-menu-after-edit");
    assert_eq!(edits.get(), 1, "app menu must retain its Edit action");
    let actions = Rc::new(RefCell::new(Vec::new()));
    let calls = actions.clone();
    full.on_run_action(move |_, id| calls.borrow_mut().push(id.to_string()));
    click(&window, 254.0, 208.0);
    snapshot(&window, "overflow-menu-light");
    click(&window, 110.0, 400.0);
    assert!(actions.borrow().is_empty(), "disabled actions must not run");
    // The default popup policy may dismiss the disabled click; clear it first.
    click(&window, 400.0, 60.0);
    click(&window, 254.0, 208.0);
    click(&window, 110.0, 250.0);
    assert_eq!(actions.borrow().as_slice(), ["action-6"]);
}
