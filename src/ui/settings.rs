//! The settings page and applying settings (theme, line limit, autostart, data folder).

use std::rc::Rc;

use slint::ComponentHandle;

use super::editor::{parse_retention, retention_controls, when_running_index, when_running_of};
use super::{Controller, convert};
use crate::SettingsBridge;
use crate::data::{DataLocation, IconSize, McpAccess, Settings, Store, ThemePref};
use crate::platform::{self, autostart};

impl Controller {
    pub(super) fn open_settings(&self) {
        let s = self.settings.read().unwrap().clone();
        let ui = self.ui();
        let g = ui.global::<SettingsBridge>();
        g.set_theme_index(match s.theme {
            ThemePref::System => 0,
            ThemePref::Light => 1,
            ThemePref::Dark => 2,
        });
        g.set_autostart(s.autostart);
        g.set_icon_size_index(match s.icon_size {
            IconSize::Small => 0,
            IconSize::Medium => 1,
            IconSize::Large => 2,
        });
        g.set_top_compact(s.always_on_top_compact);
        g.set_top_icon(s.always_on_top_icon);
        g.set_line_limit(s.output_line_limit.to_string().into());
        g.set_when_running_index(when_running_index(s.default_when_running));
        let r = &s.default_retention;
        let (i, c) = retention_controls(r.build);
        g.set_retention_build(i);
        g.set_count_build(c.into());
        let (i, c) = retention_controls(r.run);
        g.set_retention_run(i);
        g.set_count_run(c.into());
        let (i, c) = retention_controls(r.build_run);
        g.set_retention_build_run(i);
        g.set_count_build_run(c.into());
        let (i, c) = retention_controls(r.custom);
        g.set_retention_custom(i);
        g.set_count_custom(c.into());
        g.set_auto_update(s.auto_update);
        g.set_mcp_index(match s.mcp_access {
            McpAccess::Off => 0,
            McpAccess::AskFirst => 1,
            McpAccess::Unrestricted => 2,
        });
        self.refresh_data_location();
        ui.set_page("settings".into());
    }

    /// Shows the data location setting. An instance started with `--data-dir` cannot change
    /// it, because the file lives in the default data folder of the real instance.
    fn refresh_data_location(&self) {
        let ui = self.ui();
        let g = ui.global::<SettingsBridge>();
        let in_use = self.store.root().display().to_string();
        let locked = !self.restart_args.is_empty();
        let location = Store::load_location(&Store::default_root());
        let hint = if locked {
            format!("Set by --data-dir: {in_use}")
        } else {
            let next = location
                .custom_root()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(Store::default_root);
            if next == self.store.root() {
                format!("In use: {in_use}")
            } else {
                format!(
                    "In use: {in_use}. Restart RunBay to use {}; existing data is not moved.",
                    next.display()
                )
            }
        };
        g.set_data_locked(locked);
        g.set_data_custom(location.custom);
        g.set_data_hint(hint.into());
    }

    /// Saves the data location; it takes effect when RunBay starts again.
    fn save_data_location(&self, location: &DataLocation) {
        self.report(Store::save_location(&Store::default_root(), location));
        self.refresh_data_location();
    }

    /// Asks for the data folder; `None` when cancelled.
    fn pick_data_folder(&self) -> Option<std::path::PathBuf> {
        let owner = convert::hwnd(self.ui().window());
        platform::dialogs::pick_folder(owner, "Data folder")
    }

    /// Reads the page; invalid numbers keep the previous value until they are valid.
    fn settings_from_page(&self) -> Settings {
        let ui = self.ui();
        let g = ui.global::<SettingsBridge>();
        let mut s = self.settings.read().unwrap().clone();
        s.theme = match g.get_theme_index() {
            1 => ThemePref::Light,
            2 => ThemePref::Dark,
            _ => ThemePref::System,
        };
        s.autostart = g.get_autostart();
        s.icon_size = match g.get_icon_size_index() {
            0 => IconSize::Small,
            2 => IconSize::Large,
            _ => IconSize::Medium,
        };
        s.always_on_top_compact = g.get_top_compact();
        s.always_on_top_icon = g.get_top_icon();
        if let Ok(limit) = g.get_line_limit().trim().parse::<u32>()
            && limit >= 100
        {
            s.output_line_limit = limit;
        }
        s.default_when_running = when_running_of(g.get_when_running_index());
        let r = &mut s.default_retention;
        if let Ok(v) = parse_retention(g.get_retention_build(), &g.get_count_build()) {
            r.build = v;
        }
        if let Ok(v) = parse_retention(g.get_retention_run(), &g.get_count_run()) {
            r.run = v;
        }
        if let Ok(v) = parse_retention(g.get_retention_build_run(), &g.get_count_build_run()) {
            r.build_run = v;
        }
        if let Ok(v) = parse_retention(g.get_retention_custom(), &g.get_count_custom()) {
            r.custom = v;
        }
        s.auto_update = g.get_auto_update();
        s.mcp_access = match g.get_mcp_index() {
            0 => McpAccess::Off,
            2 => McpAccess::Unrestricted,
            _ => McpAccess::AskFirst,
        };
        s
    }

    /// Stores new settings and applies what changed.
    pub(super) fn apply_settings(&self, new: Settings) {
        let old = self.settings.read().unwrap().clone();
        if new == old {
            return;
        }
        if let Err(e) = self.store.save_settings(&new) {
            self.snackbar(format!("{e:#}"));
            return;
        }
        *self.settings.write().unwrap() = new.clone();
        if new.autostart != old.autostart {
            self.sync_autostart();
        }
        self.manager.set_line_limit(new.output_line_limit);
        if new.theme != old.theme {
            self.apply_theme();
        }
        self.apply_mode_settings();
        self.refresh_tray();
    }

    /// Makes the registry match the autostart setting.
    pub(super) fn sync_autostart(&self) {
        let enabled = self.settings.read().unwrap().autostart;
        let result = if enabled {
            autostart::command_line()
                .and_then(|cmd| autostart::set(autostart::VALUE_NAME, Some(&cmd)))
        } else {
            autostart::set(autostart::VALUE_NAME, None)
        };
        self.report(result);
    }

    pub(super) fn bind_settings(self: &Rc<Self>) {
        let ui = self.ui();
        let g = ui.global::<SettingsBridge>();
        let weak = Rc::downgrade(self);
        g.on_changed(move || {
            if let Some(c) = weak.upgrade() {
                let new = c.settings_from_page();
                c.apply_settings(new);
            }
        });
        let weak = Rc::downgrade(self);
        g.on_data_custom_toggled(move || {
            if let Some(c) = weak.upgrade() {
                let mut location = Store::load_location(&Store::default_root());
                location.custom = c.ui().global::<SettingsBridge>().get_data_custom();
                if location.custom && location.path.is_none() {
                    match c.pick_data_folder() {
                        Some(path) => location.path = Some(path),
                        None => return c.refresh_data_location(),
                    }
                }
                c.save_data_location(&location);
            }
        });
        let weak = Rc::downgrade(self);
        g.on_choose_data_folder(move || {
            if let Some(c) = weak.upgrade()
                && let Some(path) = c.pick_data_folder()
            {
                let location = DataLocation {
                    custom: true,
                    path: Some(path),
                };
                c.save_data_location(&location);
            }
        });
        let weak = Rc::downgrade(self);
        g.on_check_updates(move || {
            if let Some(c) = weak.upgrade() {
                c.check_updates(true);
            }
        });
    }
}
