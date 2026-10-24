//! The data folder: settings, window state, groups, and registered apps.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::atomic::{read_json, write_atomic, write_json};
use super::history::History;
use super::ids::{is_slug, unique_slug};
use super::model::{ActionKind, AppDef, DataLocation, Groups, Settings, WindowState};

/// An app folder as loaded from disk: either a valid definition or an error message.
#[derive(Debug, Clone)]
pub enum LoadedApp {
    /// `app.json` was read and validated.
    Ok(AppDef),
    /// The app folder exists but its data is corrupted; the app is shown as errored.
    Errored {
        /// Folder name (the app id).
        id: String,
        /// What went wrong.
        error: String,
    },
}

impl LoadedApp {
    /// The app id.
    pub fn id(&self) -> &str {
        match self {
            LoadedApp::Ok(app) => &app.id,
            LoadedApp::Errored { id, .. } => id,
        }
    }
}

/// Access to the `data/` folder.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Opens (and creates) the data folder at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let store = Self { root: root.into() };
        fs::create_dir_all(store.meta_dir())
            .with_context(|| format!("cannot create {}", store.meta_dir().display()))?;
        fs::create_dir_all(store.apps_dir())?;
        Ok(store)
    }

    /// The default data folder: `data/` next to the executable.
    pub fn default_root() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("data")))
            .unwrap_or_else(|| PathBuf::from("data"))
    }

    /// The data folder chosen in Settings (`meta/data-location.json` of the default data
    /// folder), or the default one when none is chosen.
    pub fn configured_root() -> PathBuf {
        let default = Self::default_root();
        Self::load_location(&default)
            .custom_root()
            .map(Path::to_path_buf)
            .unwrap_or(default)
    }

    /// Reads the data location kept in the default data folder `default_root`; a missing or
    /// corrupted file gives the default location.
    pub fn load_location(default_root: &Path) -> DataLocation {
        let path = Self::location_file(default_root);
        if !path.exists() {
            return DataLocation::default();
        }
        read_json(&path).unwrap_or_else(|e| {
            log::error!("{e:#}");
            DataLocation::default()
        })
    }

    /// Writes the data location into the default data folder `default_root`.
    pub fn save_location(default_root: &Path, location: &DataLocation) -> Result<()> {
        let path = Self::location_file(default_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        write_json(&path, location)
    }

    fn location_file(default_root: &Path) -> PathBuf {
        default_root.join("meta").join("data-location.json")
    }

    /// The data folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `data/meta`.
    pub fn meta_dir(&self) -> PathBuf {
        self.root.join("meta")
    }

    /// `data/apps`.
    pub fn apps_dir(&self) -> PathBuf {
        self.root.join("apps")
    }

    /// `data/apps/<id>`.
    pub fn app_dir(&self, id: &str) -> PathBuf {
        self.apps_dir().join(id)
    }

    /// History of an app.
    pub fn history(&self, id: &str) -> History {
        History::new(&self.app_dir(id))
    }

    /// Loads settings; a missing file gives defaults, a corrupted one is logged and replaced
    /// by defaults in memory (the file is left untouched until the next save).
    pub fn load_settings(&self) -> Settings {
        self.load_meta("settings.json")
    }

    /// Saves settings.
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        write_json(&self.meta_dir().join("settings.json"), settings)
    }

    /// Loads the window state.
    pub fn load_window_state(&self) -> WindowState {
        self.load_meta("window-state.json")
    }

    /// Saves the window state.
    pub fn save_window_state(&self, state: &WindowState) -> Result<()> {
        write_json(&self.meta_dir().join("window-state.json"), state)
    }

    /// Loads groups.
    pub fn load_groups(&self) -> Groups {
        self.load_meta("groups.json")
    }

    /// Saves groups.
    pub fn save_groups(&self, groups: &Groups) -> Result<()> {
        write_json(&self.meta_dir().join("groups.json"), groups)
    }

    fn load_meta<T: serde::de::DeserializeOwned + Default>(&self, name: &str) -> T {
        let path = self.meta_dir().join(name);
        if !path.exists() {
            return T::default();
        }
        read_json(&path).unwrap_or_else(|e| {
            log::error!("{e:#}");
            T::default()
        })
    }

    /// Loads every app folder, sorted by group order is left to the caller; here by
    /// `order`, then name.
    pub fn load_apps(&self) -> Vec<LoadedApp> {
        let mut apps = Vec::new();
        let Ok(entries) = fs::read_dir(self.apps_dir()) else {
            return apps;
        };
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            apps.push(self.load_app(&id));
        }
        apps.sort_by(|a, b| match (a, b) {
            (LoadedApp::Ok(a), LoadedApp::Ok(b)) => a
                .order
                .cmp(&b.order)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            _ => a.id().cmp(b.id()),
        });
        apps
    }

    /// Loads and validates one app.
    pub fn load_app(&self, id: &str) -> LoadedApp {
        let result = (|| -> Result<AppDef> {
            let app: AppDef = read_json(&self.app_dir(id).join("app.json"))?;
            if app.id != id {
                bail!("app.json has id \"{}\" but its folder is \"{id}\"", app.id);
            }
            app.validate().map_err(anyhow::Error::msg)?;
            Ok(app)
        })();
        match result {
            Ok(app) => LoadedApp::Ok(app),
            Err(e) => LoadedApp::Errored {
                id: id.to_string(),
                error: format!("{e:#}"),
            },
        }
    }

    /// Saves `app.json` after validating it.
    pub fn save_app(&self, app: &AppDef) -> Result<()> {
        app.validate().map_err(anyhow::Error::msg)?;
        write_json(&self.app_dir(&app.id).join("app.json"), app)
    }

    /// Creates a new app with a unique id derived from `name`.
    pub fn create_app(&self, name: &str, settings: &Settings) -> Result<AppDef> {
        let name = name.trim();
        if name.is_empty() {
            bail!("the app name is empty");
        }
        self.ensure_unique_name(name, None)?;
        let id = unique_slug(name, |s| self.app_dir(s).exists());
        let mut app = AppDef::new(&id, name, settings);
        app.order = self
            .load_apps()
            .iter()
            .filter_map(|a| match a {
                LoadedApp::Ok(a) => Some(a.order),
                _ => None,
            })
            .max()
            .map_or(10, |o| o + 10);
        self.save_app(&app)?;
        Ok(app)
    }

    /// Fails when another app already uses `name` (case-insensitive).
    pub fn ensure_unique_name(&self, name: &str, except_id: Option<&str>) -> Result<()> {
        let taken = self.load_apps().into_iter().any(|a| match a {
            LoadedApp::Ok(a) => {
                Some(a.id.as_str()) != except_id && a.name.eq_ignore_ascii_case(name.trim())
            }
            _ => false,
        });
        if taken {
            bail!("an app named \"{}\" already exists", name.trim());
        }
        Ok(())
    }

    /// Deletes an app folder including its history.
    pub fn delete_app(&self, id: &str) -> Result<()> {
        if !is_slug(id) {
            bail!("invalid app id \"{id}\"");
        }
        let dir = self.app_dir(id);
        fs::remove_dir_all(&dir).with_context(|| format!("cannot delete {}", dir.display()))
    }

    /// Moves an app folder to `meta/trash/` (so an agent's removal can be undone) and returns
    /// where it went. The trash is emptied on the next start.
    pub fn trash_app(&self, id: &str) -> Result<PathBuf> {
        if !is_slug(id) {
            bail!("invalid app id \"{id}\"");
        }
        let trash = self.meta_dir().join("trash");
        fs::create_dir_all(&trash)?;
        let target = trash.join(format!(
            "{id}-{}",
            chrono::Local::now().format("%Y%m%d-%H%M%S%3f")
        ));
        fs::rename(self.app_dir(id), &target)
            .with_context(|| format!("cannot move the app \"{id}\" to the trash"))?;
        Ok(target)
    }

    /// Deletes everything in `meta/trash/`.
    pub fn empty_trash(&self) {
        let trash = self.meta_dir().join("trash");
        if trash.exists()
            && let Err(e) = fs::remove_dir_all(&trash)
        {
            log::warn!("cannot empty the trash: {e}");
        }
    }

    /// Copies an image file as the app icon (`icon.<ext>`), replacing the previous one.
    /// The caller saves `app` afterwards.
    pub fn set_icon(&self, app: &mut AppDef, source: &Path, ext: &str) -> Result<()> {
        let dir = self.app_dir(&app.id);
        let bytes =
            fs::read(source).with_context(|| format!("cannot read {}", source.display()))?;
        if let Some(old) = app.icon.take() {
            let _ = fs::remove_file(dir.join(old));
        }
        let name = format!("icon.{ext}");
        write_atomic(&dir.join(&name), &bytes)?;
        app.icon = Some(name);
        Ok(())
    }

    /// Copies an image file as the custom icon of an action, replacing the previous one.
    /// It lives next to the scripts so agent undo snapshots include it. The caller saves
    /// `app` afterwards.
    pub fn set_action_icon(
        &self,
        app: &mut AppDef,
        action_id: &str,
        source: &Path,
        ext: &str,
    ) -> Result<()> {
        let bytes =
            fs::read(source).with_context(|| format!("cannot read {}", source.display()))?;
        self.remove_action_icon(app, action_id);
        let dir = self.app_dir(&app.id);
        let action = app
            .action_mut(action_id)
            .with_context(|| format!("there is no action \"{action_id}\""))?;
        let name = super::model::action_icon_file(action_id, ext);
        write_atomic(&dir.join(&name), &bytes)?;
        action.icon_file = Some(name);
        Ok(())
    }

    /// Deletes the custom icon of an action, if it has one. The caller saves `app`.
    pub fn remove_action_icon(&self, app: &mut AppDef, action_id: &str) {
        let dir = self.app_dir(&app.id);
        if let Some(old) = app
            .action_mut(action_id)
            .and_then(|action| action.icon_file.take())
        {
            let _ = fs::remove_file(dir.join(old));
        }
    }

    /// Absolute path of an action's custom icon, if it has one.
    pub fn action_icon_path(&self, app: &AppDef, action_id: &str) -> Option<PathBuf> {
        let action = app.action(action_id)?;
        action
            .icon_file
            .as_ref()
            .map(|f| self.app_dir(&app.id).join(f))
    }

    /// Absolute path of an action's script, if it has one.
    pub fn script_path(&self, app: &AppDef, action_id: &str) -> Option<PathBuf> {
        let action = app.action(action_id)?;
        action
            .script
            .as_ref()
            .map(|s| self.app_dir(&app.id).join(s))
    }

    /// Reads the script of an action; empty when it has none.
    pub fn read_script(&self, app: &AppDef, action_id: &str) -> String {
        self.script_path(app, action_id)
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default()
    }

    /// Writes the script of an action to `actions/<action-id>.ps1` and records the path in
    /// the action. An empty script removes it (build/run buttons are then disabled).
    /// The caller saves `app` afterwards.
    pub fn write_script(&self, app: &mut AppDef, action_id: &str, script: &str) -> Result<()> {
        let app_dir = self.app_dir(&app.id);
        let action = app
            .action_mut(action_id)
            .with_context(|| format!("unknown action \"{action_id}\""))?;
        if action.kind == ActionKind::BuildRun {
            bail!("the Build + Run action has no script of its own");
        }
        let relative = format!("actions/{action_id}.ps1");
        let path = app_dir.join(&relative);
        if script.trim().is_empty() {
            let _ = fs::remove_file(&path);
            action.script = None;
        } else {
            // UTF-8 with BOM so Windows PowerShell 5.1 reads non-ASCII text correctly.
            let mut bytes = b"\xEF\xBB\xBF".to_vec();
            bytes.extend_from_slice(script.as_bytes());
            write_atomic(&path, &bytes)?;
            action.script = Some(relative);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_load_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("data")).unwrap();
        let settings = Settings::default();
        let a = store.create_app("My App", &settings).unwrap();
        assert_eq!(a.id, "my-app");
        assert!(store.create_app("my app", &settings).is_err());
        let b = store.create_app("Other", &settings).unwrap();
        assert!(b.order > a.order);
        let apps = store.load_apps();
        assert_eq!(apps.len(), 2);
        store.delete_app("my-app").unwrap();
        assert_eq!(store.load_apps().len(), 1);
    }

    #[test]
    fn data_location_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let default = dir.path().join("data");
        assert_eq!(Store::load_location(&default), DataLocation::default());
        let location = DataLocation {
            custom: true,
            path: Some(dir.path().join("repo").join("runbay")),
        };
        Store::save_location(&default, &location).unwrap();
        let loaded = Store::load_location(&default);
        assert_eq!(loaded, location);
        assert_eq!(loaded.custom_root(), location.path.as_deref());
        let off = DataLocation {
            custom: false,
            ..location
        };
        assert_eq!(off.custom_root(), None);
    }

    #[test]
    fn corrupted_app_is_errored() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        fs::create_dir_all(store.app_dir("bad")).unwrap();
        fs::write(store.app_dir("bad").join("app.json"), "{ nope").unwrap();
        let apps = store.load_apps();
        assert!(matches!(&apps[0], LoadedApp::Errored { id, .. } if id == "bad"));
    }

    #[test]
    fn scripts_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut app = store.create_app("A", &Settings::default()).unwrap();
        store
            .write_script(&mut app, "build", "Write-Output 'ü'")
            .unwrap();
        store.save_app(&app).unwrap();
        assert_eq!(
            app.action("build").unwrap().script.as_deref(),
            Some("actions/build.ps1")
        );
        assert!(
            store
                .read_script(&app, "build")
                .ends_with("Write-Output 'ü'")
        );
        assert!(store.write_script(&mut app, "build-run", "x").is_err());
        store.write_script(&mut app, "build", "  ").unwrap();
        assert!(app.action("build").unwrap().script.is_none());
    }

    #[test]
    fn window_state_round_trip() {
        use crate::data::{Mode, WindowRect, WindowState};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.load_window_state(), WindowState::default());
        let state = WindowState {
            mode: Mode::Compact,
            full: Some(WindowRect {
                x: -10,
                y: 20,
                width: 900,
                height: 600,
            }),
            full_maximized: true,
            compact: Some(WindowRect {
                x: 1,
                y: 2,
                width: 320,
                height: 420,
            }),
            icon: None,
        };
        store.save_window_state(&state).unwrap();
        assert_eq!(store.load_window_state(), state);
    }

    #[test]
    fn trash_and_icons() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut app = store.create_app("Iconic", &Settings::default()).unwrap();
        let png = dir.path().join("a.png");
        fs::write(&png, b"png").unwrap();
        store.set_icon(&mut app, &png, "png").unwrap();
        assert_eq!(app.icon.as_deref(), Some("icon.png"));
        assert!(store.app_dir(&app.id).join("icon.png").exists());
        let trash = store.trash_app(&app.id).unwrap();
        assert!(trash.join("app.json").exists());
        assert!(!store.app_dir(&app.id).exists());
        store.empty_trash();
        assert!(!trash.exists());
    }

    #[test]
    fn corrupted_settings_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        fs::write(store.meta_dir().join("settings.json"), "[").unwrap();
        assert_eq!(store.load_settings(), Settings::default());
    }
}
