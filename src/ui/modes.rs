//! Display modes. Full mode only until the Compact and Icon windows exist.

use super::Controller;
use crate::data::{Mode, WindowState};

impl Controller {
    /// Switches to `mode` and shows its window.
    pub(super) fn switch_mode(&self, _mode: Mode) {
        self.state.borrow_mut().mode = Mode::Full;
        self.focus_full();
        self.refresh_tray();
    }

    /// Shows the window of the active mode.
    pub(super) fn show_window(&self) {
        self.focus_full();
    }

    /// Toggles always on top for the active mode.
    pub(super) fn toggle_always_on_top(&self) {}

    /// Applies mode-related settings (always on top, icon size) to the mode windows.
    pub(super) fn apply_mode_settings(&self) {}

    /// Records the rectangles of the Compact and Icon windows.
    pub(super) fn save_mode_windows(&self, _state: &mut WindowState) {}
}
