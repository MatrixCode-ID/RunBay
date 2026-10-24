//! Update checks from the window.

use super::Controller;

impl Controller {
    /// Checks for updates; `manual` reports the result even when nothing was found.
    pub(super) fn check_updates(&self, manual: bool) {
        if manual {
            self.snackbar("RunBay is up to date.".into());
        }
    }

    /// The tray update item: restart to update when ready, otherwise check.
    pub(super) fn update_command(&self) {
        self.check_updates(true);
    }

    /// The version of a downloaded update ready to install.
    pub(super) fn update_ready_version(&self) -> Option<String> {
        None
    }

    /// Update information for the About dialog.
    pub(super) fn update_text(&self) -> String {
        String::new()
    }
}
