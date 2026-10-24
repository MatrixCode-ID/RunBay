//! Window lifecycle: tray, hide to tray, exit, window state, About, and app order.

use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};

use super::{Controller, convert, with_controller};
use crate::data::{LoadedApp, Mode, WindowRect};
use crate::platform::{self, monitors, tray::Tray, tray::TrayCommand};

impl Controller {
    /// Creates the tray icon.
    pub(super) fn create_tray(&self) {
        let tray = Tray::new(|command| {
            let _ = slint::invoke_from_event_loop(move || {
                with_controller(|c| c.tray_command(command));
            });
        });
        match tray {
            Ok(tray) => *self.tray.borrow_mut() = Some(tray),
            Err(e) => log::error!("cannot create the tray icon: {e:#}"),
        }
        self.refresh_tray();
    }

    fn tray_command(&self, command: TrayCommand) {
        match command {
            TrayCommand::Show => self.show_window(),
            TrayCommand::Mode(mode) => self.switch_mode(mode),
            TrayCommand::AlwaysOnTop => self.toggle_always_on_top(),
            TrayCommand::Autostart => {
                let mut s = self.settings.read().unwrap().clone();
                s.autostart = !s.autostart;
                self.apply_settings(s);
            }
            TrayCommand::Update => self.update_command(),
            TrayCommand::Exit => self.request_exit(),
        }
    }

    /// Updates the tray badge, agent dot, tooltip, and check marks.
    pub(super) fn refresh_tray(&self) {
        let tray = self.tray.borrow();
        let Some(tray) = tray.as_ref() else { return };
        let st = self.state.borrow();
        let update = self.update_ready_version();
        tray.set_status(
            &st.aggregate,
            st.aggregate_busy,
            &st.agents,
            update.as_deref(),
        );
        let s = self.settings.read().unwrap();
        let on_top = match st.mode {
            Mode::Full => false,
            Mode::Compact => s.always_on_top_compact,
            Mode::Icon => s.always_on_top_icon,
        };
        tray.set_checks(st.mode, on_top, s.autostart);
    }

    /// Exit through the tray: asks first when actions are running.
    pub(super) fn request_exit(&self) {
        let active = self.manager.active();
        if active.is_empty() {
            self.exit_now();
            return;
        }
        let names: Vec<String> = active
            .iter()
            .map(|r| {
                let i = r.info();
                format!("{} · {}", i.app_name, i.action_name)
            })
            .collect();
        let ui = self.ui();
        ui.set_exit_text(
            format!(
                "{} action(s) are still running: {}. They will be stopped.",
                names.len(),
                names.join(", ")
            )
            .into(),
        );
        ui.set_exit_visible(true);
        self.switch_mode(Mode::Full);
    }

    pub(super) fn exit_answer(&self, answer: &str) {
        let ui = self.ui();
        ui.set_exit_visible(false);
        ui.set_exit_title("Exit RunBay?".into());
        ui.set_exit_button("Stop all and exit".into());
        let for_update = std::mem::take(&mut self.state.borrow_mut().exit_for_update);
        self.end_full_for_dialog();
        if answer == "exit" {
            if for_update {
                self.install_update();
            } else {
                self.exit_now();
            }
        }
    }

    fn exit_now(&self) {
        self.save_window_state();
        self.manager.stop_all(Duration::from_secs(5));
        *self.tray.borrow_mut() = None;
        let _ = slint::quit_event_loop();
    }

    /// Restores the Full window position and size (moved onto a monitor when needed).
    pub(super) fn restore_full_window(&self) {
        let state = self.store.load_window_state();
        let ui = self.ui();
        if let Some(rect) = state.full {
            let rect = monitors::fit(rect, &monitors::work_areas());
            ui.window()
                .set_position(PhysicalPosition::new(rect.x, rect.y));
            ui.window()
                .set_size(PhysicalSize::new(rect.width, rect.height));
        }
        self.state.borrow_mut().full_maximized = state.full_maximized;
    }

    /// Writes the position and size of the Full window and the current mode.
    pub(super) fn save_window_state(&self) {
        let ui = self.ui();
        let mut state = self.store.load_window_state();
        state.mode = self.state.borrow().mode;
        if ui.window().is_visible() {
            let maximized = ui.window().is_maximized();
            state.full_maximized = maximized;
            if !maximized && !ui.window().is_minimized() {
                let p = ui.window().position();
                let s = ui.window().size();
                state.full = Some(WindowRect {
                    x: p.x,
                    y: p.y,
                    width: s.width,
                    height: s.height,
                });
            }
        }
        self.save_mode_windows(&mut state);
        if let Err(e) = self.store.save_window_state(&state) {
            log::warn!("cannot save the window state: {e:#}");
        }
    }

    /// Hides the Full window to the tray instead of closing it.
    pub(super) fn bind_close(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.ui().window().on_close_requested(move || {
            if let Some(c) = weak.upgrade() {
                c.save_window_state();
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    pub(super) fn show_about(&self) {
        let ui = self.ui();
        ui.set_update_text(self.update_text().into());
        ui.set_about_visible(true);
    }

    /// Writes the bundled third-party notices next to the data and opens them.
    pub(super) fn open_notices(&self) {
        let path = self.store.meta_dir().join("THIRD-PARTY-NOTICES.md");
        let result = std::fs::write(&path, include_str!("../../THIRD-PARTY-NOTICES.md"))
            .map_err(anyhow::Error::from)
            .and_then(|_| platform::open_path(&path));
        self.report(result);
    }

    /// Moves an app up (`-1`) or down (`1`) within its group.
    pub(super) fn move_app(&self, id: &str, delta: i32) {
        let Some(app) = self.app(id) else { return };
        let mut same: Vec<_> = self
            .state
            .borrow()
            .apps
            .iter()
            .filter_map(|a| match a {
                LoadedApp::Ok(x) if x.group == app.group => Some(x.clone()),
                _ => None,
            })
            .collect();
        same.sort_by_key(|a| a.order);
        let Some(pos) = same.iter().position(|a| a.id == id) else {
            return;
        };
        let target = pos as i64 + delta as i64;
        if target < 0 || target as usize >= same.len() {
            return;
        }
        same.swap(pos, target as usize);
        for (i, a) in same.iter_mut().enumerate() {
            let order = (i as i32 + 1) * 10;
            if a.order != order {
                a.order = order;
                if let Err(e) = self.store.save_app(a) {
                    self.snackbar(format!("{e:#}"));
                }
            }
        }
        self.reload_apps();
    }

    /// Gives the Full window keyboard focus in front of other windows.
    pub(super) fn focus_full(&self) {
        let ui = self.ui();
        let _ = ui.show();
        self.apply_taskbar_buttons();
        if self.state.borrow().full_maximized {
            ui.window().set_maximized(true);
            self.state.borrow_mut().full_maximized = false;
        }
        if let Some(hwnd) = convert::hwnd(ui.window()) {
            platform::bring_to_front(hwnd);
        }
    }
}
