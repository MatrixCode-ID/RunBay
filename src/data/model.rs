//! Serializable data model: apps, actions, settings, groups, and window state.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Which PowerShell runs the scripts of an app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Shell {
    /// `pwsh.exe` (7) when found, otherwise `powershell.exe` (5.1).
    #[default]
    Auto,
    /// Always `pwsh.exe`.
    Pwsh,
    /// Always `powershell.exe`.
    Powershell,
}

/// The role of an action within its app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    /// The app's build script.
    Build,
    /// The app's run script.
    Run,
    /// Runs the build script, then the run script when the build succeeded.
    BuildRun,
    /// A user-defined script.
    Custom,
}

/// What happens when an action is triggered while its app still has an active run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WhenRunning {
    /// Ask the user (or require `if_running` from an agent).
    #[default]
    Ask,
    /// Do not start; tell the caller the app is running.
    Reject,
    /// Stop the app's active runs, then start.
    Restart,
    /// Start alongside the active runs.
    Parallel,
}

/// How many finished runs of an action are kept on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum Retention {
    /// Nothing is written to disk; output lives in memory for the session only.
    Off,
    /// Keep the last `count` runs.
    Last {
        /// Number of runs to keep (at least 1).
        count: u32,
    },
    /// Never clean up automatically.
    Forever,
}

impl Retention {
    /// The default retention for a new action of `kind` (specification §2).
    pub fn default_for(kind: ActionKind) -> Self {
        match kind {
            ActionKind::Run => Retention::Last { count: 5 },
            _ => Retention::Last { count: 50 },
        }
    }
}

/// Material Symbols available for action buttons, with their editor labels.
pub const ACTION_ICONS: &[(&str, &str)] = &[
    ("build", "Build"),
    ("play_arrow", "Play"),
    ("rocket_launch", "Rocket"),
    ("terminal", "Terminal"),
    ("refresh", "Refresh"),
    ("folder", "Folder"),
    ("folder_open", "Open folder"),
    ("description", "Document"),
    ("settings", "Settings"),
    ("desktop_windows", "Desktop"),
    ("bubble_chart", "Bubbles"),
    ("code", "Code editor"),
    ("integration_instructions", "IDE"),
    ("code_blocks", "Code blocks"),
    ("data_object", "Data object"),
];

/// Image formats accepted for app and action icons.
pub const ICON_EXTENSIONS: &[&str] = &["png", "ico", "svg"];

/// The lowercase icon extension of `path`, if it is an accepted image format.
pub fn icon_extension(path: &std::path::Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| ICON_EXTENSIONS.contains(&e.as_str()))
}

/// The file name of an action's custom image, relative to the app folder.
pub fn action_icon_file(action_id: &str, ext: &str) -> String {
    format!("actions/{action_id}.icon.{ext}")
}

fn is_action_icon_file(action_id: &str, file: &str) -> bool {
    ICON_EXTENSIONS
        .iter()
        .any(|ext| file == action_icon_file(action_id, ext))
}

/// One action of an app, stored inside `app.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionDef {
    /// Stable slug, unique within the app.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Role of the action.
    pub kind: ActionKind,
    /// Material Symbol name, or `None` to use the action kind's default icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// User-chosen image relative to the app folder (`actions/<id>.icon.<ext>`). It
    /// replaces `icon` on buttons; brand logos are never bundled, so users add their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_file: Option<String>,
    /// Script path relative to the app folder (`actions/<id>.ps1`); `None` for
    /// `buildRun` and for build/run actions without a script.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// Overrides the app's working directory when set.
    #[serde(default)]
    pub working_directory: Option<String>,
    /// Overrides app environment variables with the same name.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Rule when the app still has an active run.
    #[serde(default)]
    pub when_running: WhenRunning,
    /// History retention.
    pub retention: Retention,
}

/// An app registered in RunBay, stored as `apps/<id>/app.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDef {
    /// Stable slug; also the folder name.
    pub id: String,
    /// Display name, unique among apps.
    pub name: String,
    /// Group id, or empty for the default group.
    #[serde(default)]
    pub group: String,
    /// Sort order within the group (ascending).
    #[serde(default)]
    pub order: i32,
    /// Icon file name inside the app folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Default working directory of the actions.
    #[serde(default)]
    pub working_directory: String,
    /// PowerShell to use.
    #[serde(default)]
    pub shell: Shell,
    /// Environment variables added to every action.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// At most one build, run, and buildRun action plus custom actions.
    pub actions: Vec<ActionDef>,
}

impl ActionDef {
    /// Creates an action with the standard name, id, and user-configured defaults
    /// for its role. Apps resolve any id collision before inserting it.
    pub fn new(kind: ActionKind, settings: &Settings) -> Self {
        let (id, name) = match kind {
            ActionKind::Build => ("build", "Build"),
            ActionKind::Run => ("run", "Run"),
            ActionKind::BuildRun => ("build-run", "Build + Run"),
            ActionKind::Custom => ("custom", "New action"),
        };
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            icon: None,
            icon_file: None,
            script: None,
            working_directory: None,
            env: BTreeMap::new(),
            when_running: settings.default_when_running,
            retention: settings.default_retention(kind),
        }
    }

    /// The action's chosen Material Symbol, falling back to its role's default.
    pub fn icon_name(&self) -> &str {
        self.icon.as_deref().unwrap_or(match self.kind {
            ActionKind::Build => "build",
            ActionKind::Run => "play_arrow",
            ActionKind::BuildRun => "rocket_launch",
            ActionKind::Custom => "terminal",
        })
    }
}

impl AppDef {
    /// Creates an app with the three standard actions (without scripts) using `settings`
    /// defaults for retention and `whenRunning`.
    pub fn new(id: &str, name: &str, settings: &Settings) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            group: String::new(),
            order: 0,
            icon: None,
            working_directory: String::new(),
            shell: Shell::Auto,
            env: BTreeMap::new(),
            actions: vec![
                ActionDef::new(ActionKind::Build, settings),
                ActionDef::new(ActionKind::Run, settings),
                ActionDef::new(ActionKind::BuildRun, settings),
            ],
        }
    }

    /// Finds an action by id.
    pub fn action(&self, id: &str) -> Option<&ActionDef> {
        self.actions.iter().find(|a| a.id == id)
    }

    /// Finds an action by id, mutably.
    pub fn action_mut(&mut self, id: &str) -> Option<&mut ActionDef> {
        self.actions.iter_mut().find(|a| a.id == id)
    }

    /// Finds the first action of `kind`.
    pub fn action_of(&self, kind: ActionKind) -> Option<&ActionDef> {
        self.actions.iter().find(|a| a.kind == kind)
    }

    /// Checks the structural rules of the specification (§4.1): at most one build, run,
    /// and buildRun action, unique slug ids, and no script on buildRun.
    pub fn validate(&self) -> Result<(), String> {
        if !super::ids::is_slug(&self.id) {
            return Err(format!("invalid app id \"{}\"", self.id));
        }
        if self.name.trim().is_empty() {
            return Err("the app name is empty".into());
        }
        for kind in [ActionKind::Build, ActionKind::Run, ActionKind::BuildRun] {
            let n = self.actions.iter().filter(|a| a.kind == kind).count();
            if n > 1 {
                return Err(format!("expected at most one {kind:?} action, found {n}"));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for action in &self.actions {
            if let Some(icon) = &action.icon
                && !ACTION_ICONS.iter().any(|(name, _)| name == icon)
            {
                return Err(format!("unknown action icon \"{icon}\""));
            }
            if let Some(file) = &action.icon_file
                && !is_action_icon_file(&action.id, file)
            {
                return Err(format!("invalid action icon file \"{file}\""));
            }
            if !super::ids::is_slug(&action.id) {
                return Err(format!("invalid action id \"{}\"", action.id));
            }
            if !seen.insert(action.id.as_str()) {
                return Err(format!("duplicate action id \"{}\"", action.id));
            }
            if let Retention::Last { count: 0 } = action.retention {
                return Err(format!(
                    "retention count of \"{}\" must be at least 1",
                    action.id
                ));
            }
        }
        Ok(())
    }
}

/// Display mode of RunBay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Regular resizable window.
    #[default]
    Full,
    /// Small app list window.
    Compact,
    /// Floating round icon.
    Icon,
}

/// Theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemePref {
    /// Follow the Windows app theme.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// Console theme, independent of the application's theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConsoleThemePref {
    /// Follow the theme selected for the application UI.
    #[default]
    Follow,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// Floating icon size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IconSize {
    /// 40 px.
    Small,
    /// 56 px.
    #[default]
    Medium,
    /// 72 px.
    Large,
}

impl IconSize {
    /// Size in logical pixels.
    pub fn px(self) -> u32 {
        match self {
            IconSize::Small => 40,
            IconSize::Medium => 56,
            IconSize::Large => 72,
        }
    }
}

/// What MCP agents may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpAccess {
    /// Every tool call fails.
    Off,
    /// Read-only tools run directly; others are confirmed in the GUI.
    #[default]
    AskFirst,
    /// Everything runs without confirmation (except `delete_app`).
    Unrestricted,
}

/// Default retention per action kind for new actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultRetention {
    /// For build actions.
    pub build: Retention,
    /// For run actions.
    pub run: Retention,
    /// For build+run actions.
    pub build_run: Retention,
    /// For custom actions.
    pub custom: Retention,
}

impl Default for DefaultRetention {
    fn default() -> Self {
        Self {
            build: Retention::default_for(ActionKind::Build),
            run: Retention::default_for(ActionKind::Run),
            build_run: Retention::default_for(ActionKind::BuildRun),
            custom: Retention::default_for(ActionKind::Custom),
        }
    }
}

/// Where the data folder is, stored in `meta/data-location.json` of the default data folder
/// next to the executable, so it can be read before the data folder itself is opened.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DataLocation {
    /// Use `path` instead of the default data folder (e.g. a folder inside a Git repository).
    pub custom: bool,
    /// The chosen data folder; ignored while `custom` is off.
    pub path: Option<PathBuf>,
}

impl DataLocation {
    /// The data folder to use, or `None` for the default one.
    pub fn custom_root(&self) -> Option<&Path> {
        self.path.as_deref().filter(|_| self.custom)
    }
}

/// RunBay settings, stored in `meta/settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Theme preference.
    pub theme: ThemePref,
    /// Theme used by every console output view.
    pub console_theme: ConsoleThemePref,
    /// Load with Windows.
    pub autostart: bool,
    /// Floating icon size.
    pub icon_size: IconSize,
    /// Always on top in Compact mode.
    pub always_on_top_compact: bool,
    /// Always on top in Icon mode.
    pub always_on_top_icon: bool,
    /// Taskbar button for the Full window; the tray icon stays available when hidden.
    pub taskbar_full: bool,
    /// Taskbar button for the Compact window.
    pub taskbar_compact: bool,
    /// On-screen output line limit per run.
    pub output_line_limit: u32,
    /// Retention for new actions.
    pub default_retention: DefaultRetention,
    /// `whenRunning` for new actions.
    pub default_when_running: WhenRunning,
    /// Automatic update checks and downloads.
    pub auto_update: bool,
    /// MCP access level.
    pub mcp_access: McpAccess,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePref::System,
            console_theme: ConsoleThemePref::Follow,
            autostart: false,
            icon_size: IconSize::Medium,
            always_on_top_compact: false,
            always_on_top_icon: true,
            taskbar_full: true,
            taskbar_compact: true,
            output_line_limit: 10_000,
            default_retention: DefaultRetention::default(),
            default_when_running: WhenRunning::Ask,
            auto_update: true,
            mcp_access: McpAccess::AskFirst,
        }
    }
}

impl Settings {
    /// Default retention for a new action of `kind`.
    pub fn default_retention(&self, kind: ActionKind) -> Retention {
        match kind {
            ActionKind::Build => self.default_retention.build,
            ActionKind::Run => self.default_retention.run,
            ActionKind::BuildRun => self.default_retention.build_run,
            ActionKind::Custom => self.default_retention.custom,
        }
    }
}

/// A group of apps in the navigation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Stable slug referenced by `AppDef::group`.
    pub id: String,
    /// Display name.
    pub name: String,
}

/// Groups and their order, stored in `meta/groups.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Groups {
    /// Groups in display order.
    pub groups: Vec<Group>,
}

/// Position and size of one mode's window, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowRect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// Last mode and window rectangles, stored in `meta/window-state.json`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowState {
    /// Mode shown last.
    pub mode: Mode,
    /// Full mode window.
    pub full: Option<WindowRect>,
    /// Full mode window was maximized.
    pub full_maximized: bool,
    /// Compact mode window.
    pub compact: Option<WindowRect>,
    /// Floating icon position (size comes from settings).
    pub icon: Option<WindowRect>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_json_matches_specification_example() {
        let json = r#"{
          "id": "my-app", "name": "My App", "group": "work", "order": 10, "icon": "icon.png",
          "workingDirectory": "C:\\src\\my-app", "shell": "auto",
          "env": { "DOTNET_ENVIRONMENT": "Development" },
          "actions": [
            { "id": "build", "name": "Build", "kind": "build", "script": "actions/build.ps1",
              "workingDirectory": null, "env": {}, "whenRunning": "ask",
              "retention": { "mode": "last", "count": 50 } },
            { "id": "run", "name": "Run", "kind": "run", "script": "actions/run.ps1",
              "whenRunning": "ask", "retention": { "mode": "last", "count": 5 } },
            { "id": "build-run", "name": "Build + Run", "kind": "buildRun",
              "whenRunning": "restart", "retention": { "mode": "last", "count": 50 } }
          ]
        }"#;
        let app: AppDef = serde_json::from_str(json).unwrap();
        app.validate().unwrap();
        assert_eq!(app.actions[2].when_running, WhenRunning::Restart);
        assert_eq!(app.actions[1].retention, Retention::Last { count: 5 });
        let back = serde_json::to_value(&app).unwrap();
        assert_eq!(back["actions"][0]["retention"]["mode"], "last");
        assert_eq!(back["actions"][2]["kind"], "buildRun");
    }

    #[test]
    fn new_app_has_standard_actions_and_defaults() {
        let app = AppDef::new("a", "A", &Settings::default());
        app.validate().unwrap();
        assert_eq!(
            app.action("run").unwrap().retention,
            Retention::Last { count: 5 }
        );
        assert_eq!(app.action("build").unwrap().when_running, WhenRunning::Ask);
    }

    #[test]
    fn action_icons_preserve_defaults_and_round_trip_choices() {
        let mut app = AppDef::new("a", "A", &Settings::default());
        assert_eq!(app.actions[0].icon_name(), "build");
        assert_eq!(app.actions[1].icon_name(), "play_arrow");
        assert_eq!(app.actions[2].icon_name(), "rocket_launch");
        for (icon, _) in ACTION_ICONS {
            app.actions[0].icon = Some(icon.to_string());
            app.validate().unwrap();
            let json = serde_json::to_string(&app).unwrap();
            let restored: AppDef = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.actions[0].icon_name(), *icon);
            assert_eq!(restored.actions[1].icon_name(), "play_arrow");
            assert!(restored.icon.is_none());
        }
        app.actions[0].icon = Some("unknown_symbol".into());
        assert!(app.validate().is_err());
        app.actions[0].icon = None;
        app.validate().unwrap();
        assert_eq!(app.actions[0].icon_name(), "build");
    }

    #[test]
    fn action_icon_files_stay_next_to_their_action() {
        let mut app = AppDef::new("a", "A", &Settings::default());
        app.actions[0].icon_file = Some(action_icon_file("build", "png"));
        app.validate().unwrap();
        let json = serde_json::to_string(&app).unwrap();
        assert!(json.contains("\"iconFile\":\"actions/build.icon.png\""));
        for bad in [
            "actions/run.icon.png",
            "../logo.png",
            "actions/build.icon.exe",
        ] {
            app.actions[0].icon_file = Some(bad.into());
            assert!(app.validate().is_err(), "{bad}");
        }
        assert_eq!(
            icon_extension(std::path::Path::new("C:/x/Logo.SVG")).as_deref(),
            Some("svg")
        );
        assert!(icon_extension(std::path::Path::new("logo.txt")).is_none());
    }

    #[test]
    fn validation_rejects_duplicates() {
        let mut app = AppDef::new("a", "A", &Settings::default());
        let mut extra = app.actions[0].clone();
        extra.kind = ActionKind::Custom;
        app.actions.push(extra);
        assert!(app.validate().is_err());
    }

    #[test]
    fn standard_actions_are_optional_but_roles_remain_unique() {
        let mut app = AppDef::new("a", "A", &Settings::default());
        app.actions.retain(|a| a.kind == ActionKind::Build);
        app.validate().unwrap();
        let restored: AppDef = serde_json::from_str(&serde_json::to_string(&app).unwrap()).unwrap();
        assert_eq!(restored.actions.len(), 1);
        app.actions.clear();
        app.validate().unwrap();
        app.actions
            .push(ActionDef::new(ActionKind::Run, &Settings::default()));
        let mut duplicate = app.actions[0].clone();
        duplicate.id = "another-run".into();
        app.actions.push(duplicate);
        assert!(app.validate().unwrap_err().contains("at most one"));
    }

    #[test]
    fn settings_fill_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(s.theme, ThemePref::Dark);
        assert_eq!(s.console_theme, ConsoleThemePref::Follow);
        assert_eq!(s.output_line_limit, 10_000);
        assert_eq!(s.mcp_access, McpAccess::AskFirst);
    }

    #[test]
    fn console_theme_round_trips_independently_of_ui_theme() {
        for console_theme in [
            ConsoleThemePref::Follow,
            ConsoleThemePref::Light,
            ConsoleThemePref::Dark,
        ] {
            let settings = Settings {
                theme: ThemePref::Light,
                console_theme,
                ..Settings::default()
            };
            let json = serde_json::to_string(&settings).unwrap();
            let restored: Settings = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, settings);
        }
    }
}
