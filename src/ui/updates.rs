//! Update checks, downloads, the *Update ready* badges, and *Restart to update*.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use slint::SharedString;

use super::{Controller, with_controller};
use crate::platform;
use crate::update::{self, Release, UpdateState};

/// Interval of the automatic checks while RunBay runs.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Update state of the window.
#[derive(Default)]
pub struct UpdateUi {
    /// A downloaded and verified update.
    pub ready: Option<(Release, PathBuf)>,
    /// A check is running.
    pub checking: bool,
}

/// The release API; `RUNBAY_UPDATE_API` overrides it for development and tests.
fn api() -> String {
    std::env::var("RUNBAY_UPDATE_API").unwrap_or_else(|_| update::GITHUB_API.to_string())
}

impl Controller {
    /// Starts the startup check and the periodic checks (when auto update is on).
    pub(super) fn start_update_checks(self: &Rc<Self>) {
        if self.settings.read().unwrap().auto_update {
            self.check_updates(false);
        }
        let timer = Rc::new(slint::Timer::default());
        timer.start(slint::TimerMode::Repeated, CHECK_INTERVAL, || {
            with_controller(|c| {
                if c.settings.read().unwrap().auto_update {
                    c.check_updates(false);
                }
            });
        });
        *self.update_timer.borrow_mut() = Some(timer);
    }

    /// Checks for updates in the background and downloads a newer version. A manual check
    /// reports its result; an automatic one only logs failures.
    pub(super) fn check_updates(&self, manual: bool) {
        {
            let mut u = self.update.borrow_mut();
            if u.checking {
                return;
            }
            if manual && let Some((release, _)) = &u.ready {
                let version = release.version.clone();
                drop(u);
                self.snackbar(format!("RunBay {version} is ready to install."));
                return;
            }
            u.checking = true;
        }
        if manual {
            self.snackbar("Checking for updates…".into());
        }
        let meta = self.store.meta_dir();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Option<(Release, PathBuf)>> {
                let mut state = UpdateState::load(&meta);
                let found = update::check(&api(), &mut state, crate::VERSION);
                state.save(&meta)?;
                match found? {
                    Some(release) => {
                        let exe = update::download(&release, &meta.join("update"))?;
                        Ok(Some((release, exe)))
                    }
                    None => Ok(None),
                }
            })();
            let _ = slint::invoke_from_event_loop(move || {
                with_controller(|c| c.update_checked(result, manual));
            });
        });
    }

    fn update_checked(&self, result: anyhow::Result<Option<(Release, PathBuf)>>, manual: bool) {
        self.update.borrow_mut().checking = false;
        match result {
            Ok(Some(ready)) => {
                let version = ready.0.version.clone();
                self.update.borrow_mut().ready = Some(ready);
                self.refresh_update_badges();
                if manual {
                    self.snackbar(format!("RunBay {version} is ready to install."));
                }
            }
            Ok(None) => {
                if manual {
                    self.snackbar(format!("RunBay {} is up to date.", crate::VERSION));
                }
            }
            Err(e) => {
                log::warn!("update check failed: {e:#}");
                if manual {
                    self.snackbar(format!("Update check failed: {e:#}"));
                }
            }
        }
    }

    /// Shows or hides the *Update ready* badges next to the version and in the tray.
    pub(super) fn refresh_update_badges(&self) {
        let badge = if self.update.borrow().ready.is_some() {
            "Update ready"
        } else {
            ""
        };
        let ui = self.ui();
        ui.set_update_badge(badge.into());
        ui.set_update_ready(!badge.is_empty());
        ui.set_update_text(self.update_text().into());
        self.each_mini(|g| g.set_update_badge(badge.into()));
        self.refresh_tray();
    }

    /// The tray update item: restart to update when ready, otherwise check.
    pub(super) fn update_command(&self) {
        if self.update.borrow().ready.is_some() {
            self.restart_to_update();
        } else {
            self.check_updates(true);
        }
    }

    /// The version of a downloaded update ready to install.
    pub(super) fn update_ready_version(&self) -> Option<String> {
        self.update
            .borrow()
            .ready
            .as_ref()
            .map(|(r, _)| r.version.clone())
    }

    /// Update information for the About dialog.
    pub(super) fn update_text(&self) -> String {
        match &self.update.borrow().ready {
            Some((release, _)) => format!("RunBay {} is ready to install.", release.version),
            None => String::new(),
        }
    }

    /// Restarts into the downloaded update; asks first when actions are running.
    pub(super) fn restart_to_update(&self) {
        let active = self.manager.active();
        if active.is_empty() {
            self.install_update();
            return;
        }
        let ui = self.ui();
        ui.set_exit_title("Restart to update?".into());
        ui.set_exit_button("Stop all and update".into());
        ui.set_exit_text(
            format!(
                "{} action(s) are still running. They will be stopped before RunBay restarts.",
                active.len()
            )
            .into(),
        );
        self.state.borrow_mut().exit_for_update = true;
        ui.set_about_visible(false);
        ui.set_exit_visible(true);
        self.show_full_for_dialog();
    }

    /// Swaps the executable and restarts with `--updated` in the same mode and position.
    pub(super) fn install_update(&self) {
        let Some((release, exe)) = self.update.borrow().ready.clone() else {
            return;
        };
        let result = (|| -> anyhow::Result<()> {
            let current = std::env::current_exe()?;
            self.save_window_state();
            self.manager.stop_all(Duration::from_secs(10));
            update::swap(&current, &exe)?;
            let mut args = vec!["--updated".to_string()];
            args.extend(self.restart_args.iter().cloned());
            if let Err(e) = std::process::Command::new(&current).args(&args).spawn() {
                // The new executable is in place but did not start; put the old one back.
                let _ = std::fs::rename(update::old_path(&current), &current);
                return Err(e.into());
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                log::info!("restarting into {}", release.version);
                *self.tray.borrow_mut() = None;
                // Release the pipe so the new instance can take it over.
                let _ = slint::quit_event_loop();
            }
            Err(e) => {
                let folder = exe.parent().map(|p| p.display().to_string());
                self.snackbar(format!(
                    "Update failed: {e:#}. The new version is in {}; replace RunBay.exe manually.",
                    folder.unwrap_or_default()
                ));
            }
        }
    }

    /// Dismisses the ready update until a newer version is published.
    pub(super) fn skip_update(&self) {
        let ready = self.update.borrow_mut().ready.take();
        if let Some((release, exe)) = ready {
            let meta = self.store.meta_dir();
            let mut state = UpdateState::load(&meta);
            state.skipped = Some(release.version.clone());
            self.report(state.save(&meta));
            let _ = std::fs::remove_file(exe);
        }
        self.refresh_update_badges();
    }

    pub(super) fn open_release_notes(&self) {
        let url = self
            .update
            .borrow()
            .ready
            .as_ref()
            .map(|(r, _)| r.html_url.clone());
        if let Some(url) = url {
            self.report(platform::open_url(&url));
        }
    }

    pub(super) fn bind_updates(self: &Rc<Self>) {
        let ui = self.ui();
        let weak = Rc::downgrade(self);
        ui.on_restart_update(move || {
            if let Some(c) = weak.upgrade() {
                c.restart_to_update();
            }
        });
        let weak = Rc::downgrade(self);
        ui.on_skip_update(move || {
            if let Some(c) = weak.upgrade() {
                c.skip_update();
            }
        });
        let weak = Rc::downgrade(self);
        ui.on_release_notes(move || {
            if let Some(c) = weak.upgrade() {
                c.open_release_notes();
            }
        });
        ui.set_update_badge(SharedString::new());
    }
}
