//! Verifies dropdown arrow/accessibility state through offscreen popup lifecycles.

use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, PhysicalSize, Rgb8Pixel};

slint::slint! {
    import { Dropdown } from "ui/components.slint";
    export component DropdownWindow inherits Window {
        width: 400px;
        height: 400px;
        out property <bool> expanded: selector.expanded;
        out property <int> selected-index: selector.current-index;
        selector := Dropdown {
            x: 20px;
            y: 20px;
            width: 200px;
            model: ["One", "Two", "Three"];
        }
    }
}

struct Offscreen(Rc<MinimalSoftwareWindow>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
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

fn key(window: &MinimalSoftwareWindow, key: slint::platform::Key) {
    let text: slint::SharedString = key.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn expanded_state_tracks_every_popup_close_path() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let dropdown = DropdownWindow::new().unwrap();
    window.set_size(PhysicalSize::new(400, 400));
    dropdown.show().unwrap();
    let mut pixels = vec![Rgb8Pixel::default(); 400 * 400];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 400);
    });
    assert!(!dropdown.get_expanded());

    // The software renderer does not rotate images; inspect the shared state
    // driving both the arrow rotation and accessibility instead of its pixels.
    for _ in 0..3 {
        click(&window, 100.0, 40.0);
        assert!(dropdown.get_expanded());
        click(&window, 300.0, 300.0);
        assert!(!dropdown.get_expanded(), "outside dismissal must collapse");
    }

    click(&window, 100.0, 40.0);
    assert!(dropdown.get_expanded());
    key(&window, slint::platform::Key::Escape);
    assert!(!dropdown.get_expanded(), "Escape must collapse");

    click(&window, 100.0, 40.0);
    assert!(dropdown.get_expanded());
    click(&window, 100.0, 145.0);
    assert_eq!(dropdown.get_selected_index(), 1);
    assert!(!dropdown.get_expanded(), "pointer selection must collapse");

    click(&window, 100.0, 40.0);
    key(&window, slint::platform::Key::End);
    key(&window, slint::platform::Key::Return);
    assert_eq!(dropdown.get_selected_index(), 2);
    assert!(!dropdown.get_expanded(), "keyboard selection must collapse");

    key(&window, slint::platform::Key::Space);
    assert!(dropdown.get_expanded(), "the focused field must reopen");
    key(&window, slint::platform::Key::Tab);
    assert!(!dropdown.get_expanded(), "Tab must collapse");
}
