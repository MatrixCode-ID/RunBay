//! The app editor page: a draft of the app with its actions and scripts, committed on Save.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, Result, bail};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use super::{Controller, convert};
use crate::data::{
    ACTION_ICONS, ActionDef, ActionKind, AppDef, Group, Groups, Retention, Shell, WhenRunning,
    icon_extension, ids,
};
use crate::platform;
use crate::{EditorAction, EditorBridge, EnvRow};

/// Change of the icon in the draft.
#[derive(Clone)]
enum IconChange {
    Set(PathBuf),
    Remove,
}

/// The app being edited.
pub struct Draft {
    app: AppDef,
    is_new: bool,
    scripts: HashMap<String, String>,
    selected: usize,
    icon: Option<IconChange>,
    /// Pending custom image changes per action id.
    action_icons: HashMap<String, IconChange>,
    app_env: Rc<VecModel<EnvRow>>,
    action_env: Rc<VecModel<EnvRow>>,
}

impl Draft {
    fn add_action(&mut self, kind: ActionKind, settings: &crate::data::Settings) -> Result<()> {
        if kind != ActionKind::Custom && self.app.action_of(kind).is_some() {
            bail!("the app already has a {kind:?} action");
        }
        let mut action = ActionDef::new(kind, settings);
        action.id = ids::unique_slug(&action.id, |id| self.app.action(id).is_some());
        if kind != ActionKind::BuildRun {
            self.scripts.insert(action.id.clone(), String::new());
        }
        self.app.actions.push(action);
        self.selected = self.app.actions.len() - 1;
        Ok(())
    }

    fn remove_selected(&mut self) {
        if self.selected < self.app.actions.len() {
            let removed = self.app.actions.remove(self.selected);
            self.scripts.remove(&removed.id);
            self.action_icons.remove(&removed.id);
            self.selected = self.selected.min(self.app.actions.len().saturating_sub(1));
        }
    }
}

/// Parses the retention controls (`Off`, `Keep the last runs`, `Forever`).
pub fn parse_retention(index: i32, count: &str) -> Result<Retention> {
    Ok(match index {
        0 => Retention::Off,
        2 => Retention::Forever,
        _ => {
            let count: u32 = count
                .trim()
                .parse()
                .ok()
                .filter(|c| *c >= 1)
                .context("the number of runs to keep must be a whole number of at least 1")?;
            Retention::Last { count }
        }
    })
}

/// The retention controls for a value.
pub fn retention_controls(retention: Retention) -> (i32, String) {
    match retention {
        Retention::Off => (0, String::new()),
        Retention::Last { count } => (1, count.to_string()),
        Retention::Forever => (2, String::new()),
    }
}

/// `whenRunning` ↔ combo box index.
pub fn when_running_index(rule: WhenRunning) -> i32 {
    match rule {
        WhenRunning::Ask => 0,
        WhenRunning::Reject => 1,
        WhenRunning::Restart => 2,
        WhenRunning::Parallel => 3,
    }
}

/// Combo box index ↔ `whenRunning`.
pub fn when_running_of(index: i32) -> WhenRunning {
    match index {
        1 => WhenRunning::Reject,
        2 => WhenRunning::Restart,
        3 => WhenRunning::Parallel,
        _ => WhenRunning::Ask,
    }
}

/// Maps the group text of the editor to a group id, creating the group when it is new.
/// Matching is by id or by name, ignoring case; empty text is the default group.
pub fn resolve_group(groups: &mut Groups, text: &str) -> (String, bool) {
    let text = text.trim();
    if text.is_empty() {
        return (String::new(), false);
    }
    if let Some(g) = groups
        .groups
        .iter()
        .find(|g| g.name.eq_ignore_ascii_case(text) || g.id.eq_ignore_ascii_case(text))
    {
        return (g.id.clone(), false);
    }
    let id = ids::unique_slug(text, |s| groups.groups.iter().any(|g| g.id == s));
    groups.groups.push(Group {
        id: id.clone(),
        name: text.to_string(),
    });
    (id, true)
}

fn env_rows(env: &std::collections::BTreeMap<String, String>) -> Vec<EnvRow> {
    env.iter()
        .map(|(k, v)| EnvRow {
            key: k.into(),
            value: v.into(),
        })
        .collect()
}

/// Reads an env table; rows with an empty name are ignored, duplicate names are an error.
fn env_of(model: &VecModel<EnvRow>) -> Result<std::collections::BTreeMap<String, String>> {
    let mut env = std::collections::BTreeMap::new();
    for row in model.iter() {
        let key = row.key.trim().to_string();
        if key.is_empty() {
            continue;
        }
        if key.contains('=') {
            bail!("the variable name \"{key}\" contains '='");
        }
        if env.insert(key.clone(), row.value.to_string()).is_some() {
            bail!("the variable \"{key}\" is defined twice");
        }
    }
    Ok(env)
}

impl Controller {
    /// Opens the editor for an app, or for a new app when `id` is `None`.
    pub(super) fn open_editor(&self, id: Option<String>) {
        let settings = self.settings.read().unwrap().clone();
        let (app, is_new) = match id.and_then(|id| self.app(&id)) {
            Some(app) => (app, false),
            None => (AppDef::new("new", "", &settings), true),
        };
        let scripts = app
            .actions
            .iter()
            .filter(|a| a.kind != ActionKind::BuildRun)
            .map(|a| {
                let text = if is_new {
                    String::new()
                } else {
                    self.store
                        .read_script(&app, &a.id)
                        .trim_start_matches('\u{feff}')
                        .to_string()
                };
                (a.id.clone(), text)
            })
            .collect();
        let draft = Draft {
            app_env: Rc::new(VecModel::from(env_rows(&app.env))),
            action_env: Rc::new(VecModel::default()),
            app,
            is_new,
            scripts,
            selected: 0,
            icon: None,
            action_icons: HashMap::new(),
        };
        let ui = self.ui();
        let g = ui.global::<EditorBridge>();
        g.set_is_new(is_new);
        g.set_action_icon_options(ModelRc::new(VecModel::from(
            std::iter::once(SharedString::from("Default for action"))
                .chain(
                    ACTION_ICONS
                        .iter()
                        .map(|(_, label)| SharedString::from(*label)),
                )
                .collect::<Vec<_>>(),
        )));
        g.set_action_icon_names(ModelRc::new(VecModel::from(
            std::iter::once(SharedString::new())
                .chain(
                    ACTION_ICONS
                        .iter()
                        .map(|(name, _)| SharedString::from(*name)),
                )
                .collect::<Vec<_>>(),
        )));
        g.set_error(SharedString::new());
        g.set_name(draft.app.name.clone().into());
        let groups = self.store.load_groups();
        let group_name = groups
            .groups
            .iter()
            .find(|x| x.id == draft.app.group)
            .map_or(draft.app.group.clone(), |x| x.name.clone());
        g.set_group(group_name.into());
        g.set_groups(ModelRc::new(VecModel::from(
            groups
                .groups
                .iter()
                .map(|x| SharedString::from(x.name.as_str()))
                .collect::<Vec<_>>(),
        )));
        g.set_working_directory(draft.app.working_directory.clone().into());
        g.set_shell_index(match draft.app.shell {
            Shell::Auto => 0,
            Shell::Pwsh => 1,
            Shell::Powershell => 2,
        });
        g.set_app_env(ModelRc::from(draft.app_env.clone()));
        g.set_action_env(ModelRc::from(draft.action_env.clone()));
        *self.editor.borrow_mut() = Some(draft);
        self.refresh_editor_icon();
        self.load_selected_action();
        ui.set_page("editor".into());
    }

    fn refresh_editor_icon(&self) {
        let ui = self.ui();
        let g = ui.global::<EditorBridge>();
        let editor = self.editor.borrow();
        let Some(d) = editor.as_ref() else { return };
        let image = match &d.icon {
            Some(IconChange::Set(path)) => slint::Image::load_from_path(path).ok(),
            Some(IconChange::Remove) => None,
            None => d.app.icon.as_ref().and_then(|file| {
                slint::Image::load_from_path(&self.store.app_dir(&d.app.id).join(file)).ok()
            }),
        };
        g.set_has_icon(image.is_some());
        g.set_icon(image.unwrap_or_default());
        g.set_initial(convert::initial(&g.get_name()).into());
    }

    /// The custom image of a draft action, including a pending change.
    fn draft_action_image(&self, d: &Draft, action: &ActionDef) -> Option<slint::Image> {
        match d.action_icons.get(&action.id) {
            Some(IconChange::Set(path)) => slint::Image::load_from_path(path).ok(),
            Some(IconChange::Remove) => None,
            None => action.icon_file.as_ref().and_then(|file| {
                slint::Image::load_from_path(&self.store.app_dir(&d.app.id).join(file)).ok()
            }),
        }
    }

    /// Shows the selected action of the draft.
    fn load_selected_action(&self) {
        let ui = self.ui();
        let g = ui.global::<EditorBridge>();
        let editor = self.editor.borrow();
        let Some(d) = editor.as_ref() else { return };
        let actions = d
            .app
            .actions
            .iter()
            .map(|a| {
                let image = self.draft_action_image(d, a);
                EditorAction {
                    id: a.id.clone().into(),
                    name: a.name.clone().into(),
                    kind: format!("{:?}", a.kind).to_lowercase().into(),
                    icon_name: a.icon_name().into(),
                    has_image: image.is_some(),
                    image: image.unwrap_or_default(),
                    has_script: d.scripts.get(&a.id).is_some_and(|s| !s.trim().is_empty()),
                }
            })
            .collect::<Vec<_>>();
        // Keep delegates (and keyboard focus) alive while switching actions.
        let model = g.get_actions();
        if let Some(rows) = model.as_any().downcast_ref::<VecModel<EditorAction>>() {
            if rows.row_count() == actions.len() {
                for (index, action) in actions.into_iter().enumerate() {
                    if rows.row_data(index).as_ref() != Some(&action) {
                        rows.set_row_data(index, action);
                    }
                }
            } else {
                rows.set_vec(actions);
            }
        } else {
            g.set_actions(ModelRc::new(VecModel::from(actions)));
        }
        g.set_can_add_build(d.app.action_of(ActionKind::Build).is_none());
        g.set_can_add_run(d.app.action_of(ActionKind::Run).is_none());
        g.set_can_add_build_run(d.app.action_of(ActionKind::BuildRun).is_none());
        let Some(action) = d.app.actions.get(d.selected) else {
            g.set_selected_action(-1);
            g.set_action_is_custom(false);
            g.set_action_has_script_file(false);
            g.set_action_has_image(false);
            g.set_action_name(SharedString::new());
            g.set_script(SharedString::new());
            d.action_env.set_vec(Vec::new());
            return;
        };
        g.set_selected_action(d.selected as i32);
        g.set_action_name(action.name.clone().into());
        g.set_action_icon_index(
            action
                .icon
                .as_ref()
                .and_then(|icon| ACTION_ICONS.iter().position(|(name, _)| name == icon))
                .map_or(0, |i| i as i32 + 1),
        );
        let image = self.draft_action_image(d, action);
        g.set_action_has_image(image.is_some());
        g.set_action_image(image.unwrap_or_default());
        g.set_script(
            d.scripts
                .get(&action.id)
                .cloned()
                .unwrap_or_default()
                .into(),
        );
        g.set_action_working_directory(action.working_directory.clone().unwrap_or_default().into());
        d.action_env.set_vec(env_rows(&action.env));
        g.set_when_running_index(when_running_index(action.when_running));
        let (index, count) = retention_controls(action.retention);
        g.set_retention_index(index);
        g.set_retention_count(count.into());
        g.set_action_is_custom(action.kind == ActionKind::Custom);
        g.set_action_is_build_run(action.kind == ActionKind::BuildRun);
        g.set_action_has_script_file(
            !d.is_new && self.store.script_path(&d.app, &action.id).is_some(),
        );
    }

    /// Copies the action fields from the page into the draft.
    fn commit_action(&self) -> Result<()> {
        let ui = self.ui();
        let g = ui.global::<EditorBridge>();
        let mut editor = self.editor.borrow_mut();
        let Some(d) = editor.as_mut() else {
            return Ok(());
        };
        let Some(action) = d.app.actions.get_mut(d.selected) else {
            return Ok(());
        };
        let env = env_of(&d.action_env)?;
        let retention = parse_retention(g.get_retention_index(), &g.get_retention_count());
        action.icon = g
            .get_action_icon_index()
            .checked_sub(1)
            .and_then(|i| ACTION_ICONS.get(i as usize))
            .map(|(name, _)| name.to_string());
        let name = g.get_action_name().trim().to_string();
        if !name.is_empty() {
            action.name = name;
        }
        if action.kind != ActionKind::BuildRun {
            d.scripts
                .insert(action.id.clone(), g.get_script().to_string());
        }
        let dir = g.get_action_working_directory().trim().to_string();
        action.working_directory = (!dir.is_empty()).then_some(dir);
        action.env = env;
        action.when_running = when_running_of(g.get_when_running_index());
        action.retention = retention?;
        Ok(())
    }

    fn editor_error(&self, e: anyhow::Error) {
        self.ui()
            .global::<EditorBridge>()
            .set_error(format!("{e:#}").into());
    }

    pub(super) fn bind_editor(self: &Rc<Self>) {
        let ui = self.ui();
        let g = ui.global::<EditorBridge>();
        let weak = Rc::downgrade(self);
        let with = move |f: fn(&Controller)| {
            let weak = weak.clone();
            move || {
                if let Some(c) = weak.upgrade() {
                    f(&c);
                }
            }
        };
        g.on_cancel(with(|c| {
            *c.editor.borrow_mut() = None;
            c.ui().set_page("".into());
        }));
        g.on_save(with(|c| {
            if let Err(e) = c.save_editor() {
                c.editor_error(e);
            }
        }));
        g.on_choose_icon(with(|c| {
            let owner = convert::hwnd(c.ui().window());
            if let Some(path) = platform::dialogs::pick_file(
                owner,
                "Choose an icon",
                "Images (png, ico, svg)",
                "*.png;*.ico;*.svg",
            ) {
                if let Some(d) = c.editor.borrow_mut().as_mut() {
                    d.icon = Some(IconChange::Set(path));
                }
                c.refresh_editor_icon();
            }
        }));
        g.on_remove_icon(with(|c| {
            if let Some(d) = c.editor.borrow_mut().as_mut() {
                d.icon = Some(IconChange::Remove);
            }
            c.refresh_editor_icon();
        }));
        g.on_choose_action_image(with(|c| {
            if let Err(e) = c.commit_action() {
                return c.editor_error(e);
            }
            let owner = convert::hwnd(c.ui().window());
            if let Some(path) = platform::dialogs::pick_file(
                owner,
                "Choose an action icon",
                "Images (png, ico, svg)",
                "*.png;*.ico;*.svg",
            ) {
                if icon_extension(&path).is_none() {
                    return c.editor_error(anyhow::anyhow!(
                        "the icon must be a .png, .ico, or .svg file"
                    ));
                }
                if let Some(d) = c.editor.borrow_mut().as_mut()
                    && let Some(action) = d.app.actions.get(d.selected)
                {
                    d.action_icons
                        .insert(action.id.clone(), IconChange::Set(path));
                }
                c.load_selected_action();
            }
        }));
        g.on_remove_action_image(with(|c| {
            if let Err(e) = c.commit_action() {
                return c.editor_error(e);
            }
            if let Some(d) = c.editor.borrow_mut().as_mut()
                && let Some(action) = d.app.actions.get(d.selected)
            {
                d.action_icons.insert(action.id.clone(), IconChange::Remove);
            }
            c.load_selected_action();
        }));
        g.on_choose_folder(with(|c| {
            let owner = convert::hwnd(c.ui().window());
            if let Some(path) = platform::dialogs::pick_folder(owner, "Working directory") {
                c.ui()
                    .global::<EditorBridge>()
                    .set_working_directory(path.to_string_lossy().as_ref().into());
            }
        }));
        let weak = Rc::downgrade(self);
        g.on_add_action(move |kind| {
            let Some(c) = weak.upgrade() else {
                return;
            };
            if let Err(e) = c.commit_action() {
                return c.editor_error(e);
            }
            let result = (|| -> Result<()> {
                let kind: ActionKind =
                    serde_json::from_value(serde_json::Value::String(kind.to_string()))?;
                let settings = c.settings.read().unwrap().clone();
                if let Some(d) = c.editor.borrow_mut().as_mut() {
                    d.add_action(kind, &settings)?;
                }
                Ok(())
            })();
            if let Err(e) = result {
                return c.editor_error(e);
            }
            c.load_selected_action();
        });
        g.on_remove_action(with(|c| {
            if let Some(d) = c.editor.borrow_mut().as_mut() {
                d.remove_selected();
            }
            c.load_selected_action();
        }));
        g.on_open_external(with(|c| {
            let result = (|| -> Result<()> {
                c.commit_action()?;
                let mut editor = c.editor.borrow_mut();
                let d = editor.as_mut().context("no draft")?;
                let action_id = d
                    .app
                    .actions
                    .get(d.selected)
                    .context("no selected action")?
                    .id
                    .clone();
                // The script must exist on disk for the external editor.
                let mut app = c.app(&d.app.id).context("save the app first")?;
                if app.action(&action_id).is_none() {
                    bail!("save the app first to create this action");
                }
                let text = d.scripts.get(&action_id).cloned().unwrap_or_default();
                let text = if text.trim().is_empty() {
                    "# PowerShell\r\n".to_string()
                } else {
                    text
                };
                c.store.write_script(&mut app, &action_id, &text)?;
                c.store.save_app(&app)?;
                d.scripts.insert(action_id.clone(), text);
                d.app.action_mut(&action_id).unwrap().script =
                    app.action(&action_id).unwrap().script.clone();
                let path = c
                    .store
                    .script_path(&app, &action_id)
                    .context("no script file")?;
                drop(editor);
                c.load_selected_action();
                platform::edit_path(&path)
            })();
            if let Err(e) = result {
                c.editor_error(e);
            }
        }));
        g.on_reload_script(with(|c| {
            let editor = c.editor.borrow();
            if let Some(d) = editor.as_ref()
                && let Some(app) = c.app(&d.app.id)
                && let Some(action) = d.app.actions.get(d.selected)
            {
                let action_id = action.id.clone();
                let text = c.store.read_script(&app, &action_id);
                c.ui()
                    .global::<EditorBridge>()
                    .set_script(text.trim_start_matches('\u{feff}').into());
            }
        }));

        let weak = Rc::downgrade(self);
        g.on_select_action(move |index| {
            if let Some(c) = weak.upgrade() {
                if let Err(e) = c.commit_action() {
                    return c.editor_error(e);
                }
                if let Some(d) = c.editor.borrow_mut().as_mut() {
                    d.selected = (index.max(0) as usize).min(d.app.actions.len().saturating_sub(1));
                }
                c.load_selected_action();
            }
        });
        let weak = Rc::downgrade(self);
        g.on_move_action(move |delta| {
            if let Some(c) = weak.upgrade() {
                if let Err(e) = c.commit_action() {
                    return c.editor_error(e);
                }
                if let Some(d) = c.editor.borrow_mut().as_mut() {
                    let target = d.selected as i64 + delta as i64;
                    // Custom actions move among adjacent custom actions.
                    if target >= 0
                        && d.app
                            .actions
                            .get(d.selected)
                            .is_some_and(|a| a.kind == ActionKind::Custom)
                        && (target as usize) < d.app.actions.len()
                        && d.app.actions[target as usize].kind == ActionKind::Custom
                    {
                        d.app.actions.swap(d.selected, target as usize);
                        d.selected = target as usize;
                    }
                }
                c.load_selected_action();
            }
        });
        for app_table in [true, false] {
            let weak = Rc::downgrade(self);
            let model = move |c: &Controller| {
                c.editor.borrow().as_ref().map(|d| {
                    if app_table {
                        d.app_env.clone()
                    } else {
                        d.action_env.clone()
                    }
                })
            };
            let edited = {
                let weak = weak.clone();
                move |i: i32, k: SharedString, v: SharedString| {
                    if let Some(c) = weak.upgrade()
                        && let Some(m) = model(&c)
                    {
                        m.set_row_data(i as usize, EnvRow { key: k, value: v });
                    }
                }
            };
            let removed = {
                let weak = weak.clone();
                move |i: i32| {
                    if let Some(c) = weak.upgrade()
                        && let Some(m) = model(&c)
                        && (i as usize) < m.row_count()
                    {
                        m.remove(i as usize);
                    }
                }
            };
            let added = move || {
                if let Some(c) = weak.upgrade()
                    && let Some(m) = model(&c)
                {
                    m.push(EnvRow::default());
                }
            };
            if app_table {
                g.on_app_env_edited(edited);
                g.on_app_env_removed(removed);
                g.on_app_env_added(added);
            } else {
                g.on_action_env_edited(edited);
                g.on_action_env_removed(removed);
                g.on_action_env_added(added);
            }
        }
    }

    /// Validates the draft and writes it to disk.
    fn save_editor(&self) -> Result<()> {
        self.commit_action()?;
        let ui = self.ui();
        let g = ui.global::<EditorBridge>();
        let mut editor = self.editor.borrow_mut();
        let d = editor.as_mut().context("no draft")?;
        let name = g.get_name().trim().to_string();
        if name.is_empty() {
            bail!("the app name is empty");
        }
        let env = env_of(&d.app_env)?;
        let mut groups = self.store.load_groups();
        let (group, new_group) = resolve_group(&mut groups, &g.get_group());

        let mut app = if d.is_new {
            let settings = self.settings.read().unwrap().clone();
            self.store.create_app(&name, &settings)?
        } else {
            self.store.ensure_unique_name(&name, Some(&d.app.id))?;
            self.app(&d.app.id).context("the app no longer exists")?
        };
        // Keep the draft actions but the real id and the scripts known on disk.
        let previous = app.clone();
        app.name = name;
        app.group = group;
        app.working_directory = g.get_working_directory().trim().to_string();
        app.shell = match g.get_shell_index() {
            1 => Shell::Pwsh,
            2 => Shell::Powershell,
            _ => Shell::Auto,
        };
        app.env = env;
        app.actions = d
            .app
            .actions
            .iter()
            .map(|a| {
                let mut a = a.clone();
                let old = previous.action(&a.id);
                a.script = old.and_then(|p| p.script.clone());
                a.icon_file = old.and_then(|p| p.icon_file.clone());
                a
            })
            .collect();
        app.validate().map_err(anyhow::Error::msg)?;
        for old in &previous.actions {
            if app.action(&old.id).is_none()
                && self.manager.active_for_app(&app.id).iter().any(|run| {
                    let info = run.info();
                    info.action_id == old.id
                        || (info.kind == ActionKind::BuildRun
                            && matches!(old.kind, ActionKind::Build | ActionKind::Run))
                })
            {
                bail!("stop {} before removing it", old.name);
            }
        }
        for (action_id, text) in &d.scripts {
            if app.action(action_id).is_some() {
                let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
                self.store.write_script(&mut app, action_id, &text)?;
            }
        }
        // Scripts and images of removed actions.
        for old in &previous.actions {
            if app.action(&old.id).is_none() {
                if let Some(path) = self.store.script_path(&previous, &old.id) {
                    let _ = std::fs::remove_file(path);
                }
                if let Some(path) = self.store.action_icon_path(&previous, &old.id) {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        for (action_id, change) in &d.action_icons {
            if app.action(action_id).is_none() {
                continue;
            }
            match change {
                IconChange::Set(path) => {
                    let ext = icon_extension(path)
                        .context("the icon must be a .png, .ico, or .svg file")?;
                    self.store
                        .set_action_icon(&mut app, action_id, path, &ext)?;
                }
                IconChange::Remove => self.store.remove_action_icon(&mut app, action_id),
            }
        }
        match d.icon.clone() {
            Some(IconChange::Set(path)) => {
                let ext =
                    icon_extension(&path).context("the icon must be a .png, .ico, or .svg file")?;
                self.store.set_icon(&mut app, &path, &ext)?;
            }
            Some(IconChange::Remove) => {
                if let Some(old) = app.icon.take() {
                    let _ = std::fs::remove_file(self.store.app_dir(&app.id).join(old));
                }
            }
            None => {}
        }
        if new_group {
            self.store.save_groups(&groups)?;
        }
        self.store.save_app(&app)?;
        let _ = self.store.history(&app.id).apply_retention(&app);
        *editor = None;
        drop(editor);
        ui.set_page("".into());
        self.reload_apps();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_keeps_single_clicks_and_keyboard_focus() {
        use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
        use slint::platform::{Key, Platform, PointerEventButton, WindowAdapter, WindowEvent};
        use slint::{LogicalPosition, PhysicalSize, Rgb8Pixel};
        use std::cell::RefCell;

        thread_local! {
            static TEST_WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
            static TEST_WINDOWS: RefCell<Vec<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
        }
        struct Offscreen;
        impl Platform for Offscreen {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
                let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
                TEST_WINDOWS.with(|windows| windows.borrow_mut().push(window.clone()));
                TEST_WINDOW.with(|slot| {
                    if slot.borrow().is_none() {
                        *slot.borrow_mut() = Some(window.clone());
                    }
                });
                Ok(window)
            }
        }
        fn render(ui: &crate::MainWindow) -> Vec<Rgb8Pixel> {
            std::thread::sleep(std::time::Duration::from_millis(140));
            slint::platform::update_timers_and_animations();
            let mut pixels = vec![Rgb8Pixel::default(); 1100 * 736];
            ui.window().request_redraw();
            TEST_WINDOW.with(|slot| {
                slot.borrow().as_ref().unwrap().draw_if_needed(|renderer| {
                    renderer.render(&mut pixels, 1100);
                });
            });
            let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../.artefacts/RunBay/gui/navigation");
            std::fs::create_dir_all(&folder).unwrap();
            let rgb: Vec<_> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
            image::save_buffer(
                folder.join("latest.png"),
                &rgb,
                1100,
                736,
                image::ColorType::Rgb8,
            )
            .unwrap();
            pixels
        }
        fn key(ui: &crate::MainWindow, key: Key) {
            let text: SharedString = key.into();
            ui.window()
                .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            ui.window()
                .dispatch_event(WindowEvent::KeyReleased { text });
        }
        fn click(ui: &crate::MainWindow, x: f32, y: f32) {
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
                ui.window().dispatch_event(event);
            }
            ui.window().dispatch_event(WindowEvent::PointerMoved {
                position: LogicalPosition::new(180.0, 100.0),
            });
        }
        fn matches(pixel: Rgb8Pixel, color: slint::Color) -> bool {
            (pixel.r, pixel.g, pixel.b) == (color.red(), color.green(), color.blue())
        }
        fn selected_y(pixels: &[Rgb8Pixel], color: slint::Color, x: usize) -> f32 {
            let ys: Vec<_> = (110..650)
                .filter(|y| matches(pixels[y * 1100 + x], color))
                .collect();
            assert!(!ys.is_empty(), "Selected navigation row must be visible");
            ys.iter().sum::<usize>() as f32 / ys.len() as f32
        }
        fn focus_row(ui: &crate::MainWindow, x: std::ops::Range<usize>, y: std::ops::Range<usize>) {
            let color = ui.global::<crate::Theme>().get_secondary();
            for _ in 0..80 {
                key(ui, Key::Tab);
                let pixels = render(ui);
                let count = y
                    .clone()
                    .flat_map(|y| x.clone().map(move |x| y * 1100 + x))
                    .filter(|&i| matches(pixels[i], color))
                    .count();
                if count > 150 {
                    return;
                }
            }
            panic!("Tab must reach a navigation row");
        }

        slint::platform::set_platform(Box::new(Offscreen)).unwrap();
        let ui = crate::MainWindow::new().unwrap();
        TEST_WINDOW.with(|slot| {
            slot.borrow()
                .as_ref()
                .unwrap()
                .set_size(PhysicalSize::new(1100, 736))
        });
        ui.invoke_set_theme(1);
        let folder = tempfile::tempdir().unwrap();
        let store = crate::data::Store::open(folder.path()).unwrap();
        let settings =
            std::sync::Arc::new(std::sync::RwLock::new(crate::data::Settings::default()));
        let manager = crate::runner::RunManager::new(store.clone(), 1000);
        let host = crate::ipc::host::Host::new(
            store.clone(),
            manager.clone(),
            settings.clone(),
            std::sync::Arc::new(crate::ui::agent::HeadlessAgent),
        );
        let controller = Rc::new(Controller {
            window: ui.as_weak(),
            store,
            manager,
            settings,
            host,
            state: RefCell::new(super::super::State::default()),
            editor: RefCell::new(None),
            tray: RefCell::new(None),
            modes: super::super::modes::ModeWindows {
                compact: crate::CompactWindow::new().unwrap(),
                icon: crate::IconWindow::new().unwrap(),
                popup: crate::IconPopup::new().unwrap(),
            },
            icon_timer: RefCell::new(None),
            popup_timer: RefCell::new(None),
            update: RefCell::new(super::super::updates::UpdateUi::default()),
            update_timer: RefCell::new(None),
            clock_timer: RefCell::new(None),
            restart_args: Vec::new(),
        });
        controller.bind_editor();
        ui.show().unwrap();
        ui.set_page("settings".into());
        render(&ui);
        let selected = ui.global::<crate::Theme>().get_secondary_container();
        let first_y = selected_y(&render(&ui), selected, 100);
        for index in [3, 0, 4, 1, 2] {
            let target = first_y + index as f32 * 52.0;
            click(&ui, 100.0, target);
            assert!(
                (selected_y(&render(&ui), selected, 100) - target).abs() < 2.0,
                "Settings category {index} must select with one click"
            );
        }
        focus_row(&ui, 20..176, 110..390);
        key(&ui, Key::Return);
        key(&ui, Key::DownArrow);
        assert!((selected_y(&render(&ui), selected, 100) - (first_y + 52.0)).abs() < 2.0);
        key(&ui, Key::UpArrow);
        key(&ui, Key::Space);
        assert!((selected_y(&render(&ui), selected, 100) - first_y).abs() < 2.0);

        controller.open_editor(None);
        let bridge = ui.global::<EditorBridge>();
        let first_y = selected_y(&render(&ui), selected, 550);
        click(&ui, 500.0, first_y + 40.0);
        assert_eq!(
            bridge.get_selected_action(),
            1,
            "Action must select with one click"
        );
        for _ in 0..20 {
            bridge.invoke_add_action("custom".into());
        }
        // Return selection to the first row without changing the list model.
        bridge.invoke_select_action(0);
        render(&ui);
        focus_row(&ui, 380..564, 110..650);
        key(&ui, Key::Return);
        assert_eq!(bridge.get_selected_action(), 0);
        let model = bridge.get_actions();
        for index in 1..23 {
            key(&ui, Key::DownArrow);
            assert_eq!(
                bridge.get_selected_action(),
                index,
                "Down must retain focus"
            );
            assert_eq!(
                bridge.get_actions(),
                model,
                "Selecting must preserve the model"
            );
            assert!(selected_y(&render(&ui), selected, 550) < 650.0);
        }
        key(&ui, Key::DownArrow);
        assert_eq!(bridge.get_selected_action(), 22);
        for index in (0..22).rev() {
            key(&ui, Key::UpArrow);
            assert_eq!(bridge.get_selected_action(), index, "Up must retain focus");
            render(&ui);
        }
        key(&ui, Key::UpArrow);
        key(&ui, Key::Space);
        assert_eq!(bridge.get_selected_action(), 0);
        assert_eq!(bridge.get_error(), "");

        // Exercise real icon hit targets and persistence through the controller.
        controller.bind_app_drag();
        let settings = crate::data::Settings::default();
        for (index, name) in ["Alpha", "Beta", "Gamma"].into_iter().enumerate() {
            let mut app = AppDef::new(&name.to_lowercase(), name, &settings);
            app.order = (index as i32 + 1) * 10;
            controller.store.save_app(&app).unwrap();
        }
        ui.set_page("".into());
        controller.reload_apps();
        let pixels = render(&ui);
        let icon_color = ui.global::<crate::Theme>().get_primary_container();
        let mut icon_centers = Vec::new();
        let mut band = Vec::new();
        for y in 100..650 {
            let count = (22..54)
                .filter(|x| matches(pixels[y * 1100 + x], icon_color))
                .count();
            if count > 10 {
                band.push(y);
            } else if !band.is_empty() {
                icon_centers.push(band.iter().sum::<usize>() as f32 / band.len() as f32);
                band.clear();
            }
        }
        assert_eq!(
            icon_centers.len(),
            3,
            "Each card must have one icon drag handle"
        );
        let order = || {
            ui.get_apps()
                .iter()
                .map(|a| a.id.to_string())
                .collect::<Vec<_>>()
        };
        let drag = |x: f32, from: f32, to: f32| {
            ui.window().dispatch_event(WindowEvent::PointerPressed {
                position: LogicalPosition::new(x, from),
                button: PointerEventButton::Left,
            });
            ui.window().dispatch_event(WindowEvent::PointerMoved {
                position: LogicalPosition::new(x, to),
            });
            render(&ui);
            ui.window().dispatch_event(WindowEvent::PointerReleased {
                position: LogicalPosition::new(x, to),
                button: PointerEventButton::Left,
            });
            render(&ui);
        };
        drag(110.0, icon_centers[0], icon_centers[2] + 20.0);
        assert_eq!(
            order(),
            ["alpha", "beta", "gamma"],
            "Dragging text must not reorder"
        );
        click(&ui, 38.0, icon_centers[0]);
        assert_eq!(
            order(),
            ["alpha", "beta", "gamma"],
            "An icon click must not reorder"
        );
        drag(38.0, icon_centers[0], icon_centers[2] + 20.0);
        assert_eq!(order(), ["beta", "gamma", "alpha"]);
        controller.reload_apps();
        assert_eq!(
            order(),
            ["beta", "gamma", "alpha"],
            "Dropped order must survive reload"
        );
        render(&ui);
        drag(38.0, icon_centers[2], icon_centers[0] - 10.0);
        assert_eq!(order(), ["alpha", "beta", "gamma"]);
        drag(38.0, icon_centers[0], 730.0);
        assert_eq!(
            order(),
            ["alpha", "beta", "gamma"],
            "Outside drop must cancel"
        );
        let mut cards = ui.get_apps().iter().collect::<Vec<_>>();
        cards[0].agent = "A coding agent with a very long display name".into();
        cards[0].name = "An application with a very long display name".into();
        cards[0].status = "failed".into();
        cards[1].status = "stopped".into();
        ui.set_apps(ModelRc::new(VecModel::from(cards)));
        let pixels = render(&ui);
        let color = ui.global::<crate::Theme>().get_on_surface_variant();
        let more_x = |center: f32| {
            let xs = ((center as usize - 14)..(center as usize + 14))
                .flat_map(|y| (245..295).map(move |x| (x, y)))
                .filter(|&(x, y)| matches(pixels[y * 1100 + x], color))
                .map(|(x, _)| x)
                .collect::<Vec<_>>();
            assert!(!xs.is_empty(), "More icon must stay visible");
            xs.iter().sum::<usize>() as f32 / xs.len() as f32
        };
        // Both headers keep their More icon at the same trailing position.
        // The second card shifts down by 8 px because the first now has a chip.
        assert!((more_x(icon_centers[0]) - more_x(icon_centers[1] + 8.0)).abs() < 1.0);

        controller.modes.compact.invoke_set_theme(1);
        controller.modes.popup.invoke_set_theme(1);
        controller.modes.compact.show().unwrap();
        controller.modes.popup.show().unwrap();
        for index in [1, 3] {
            let window = TEST_WINDOWS.with(|windows| windows.borrow()[index].clone());
            window.set_size(PhysicalSize::new(360, 420));
            let draw = || {
                slint::platform::update_timers_and_animations();
                let mut pixels = vec![Rgb8Pixel::default(); 360 * 420];
                window.request_redraw();
                window.draw_if_needed(|renderer| {
                    renderer.render(&mut pixels, 360);
                });
                pixels
            };
            let pixels = draw();
            let mut centers = Vec::new();
            let mut band = Vec::new();
            for y in 36..380 {
                let count = (8..50)
                    .filter(|x| matches(pixels[y * 360 + x], icon_color))
                    .count();
                if count > 10 {
                    band.push(y);
                } else if !band.is_empty() {
                    centers.push(band.iter().sum::<usize>() as f32 / band.len() as f32);
                    band.clear();
                }
            }
            assert_eq!(
                centers.len(),
                3,
                "Mini app list must expose three icon handles"
            );
            let mut expected = order();
            expected.rotate_left(1);
            window.dispatch_event(WindowEvent::PointerPressed {
                position: LogicalPosition::new(30.0, centers[0]),
                button: PointerEventButton::Left,
            });
            window.dispatch_event(WindowEvent::PointerMoved {
                position: LogicalPosition::new(30.0, centers[2] + 25.0),
            });
            draw();
            window.dispatch_event(WindowEvent::PointerReleased {
                position: LogicalPosition::new(30.0, centers[2] + 25.0),
                button: PointerEventButton::Left,
            });
            draw();
            assert_eq!(
                order(),
                expected,
                "Mini icon drag must persist and synchronize app order"
            );
        }
    }

    #[test]
    fn remove_last_action_and_restore_defaults_without_duplicate_roles() {
        let settings = crate::data::Settings::default();
        let mut draft = Draft {
            app: AppDef::new("a", "A", &settings),
            is_new: true,
            scripts: HashMap::from([("build".into(), "old script".into())]),
            selected: 0,
            icon: None,
            action_icons: HashMap::new(),
            app_env: Rc::new(VecModel::default()),
            action_env: Rc::new(VecModel::default()),
        };
        for _ in 0..3 {
            draft.remove_selected();
        }
        assert!(draft.app.actions.is_empty());
        assert!(draft.scripts.is_empty());
        draft.remove_selected();
        draft.app.validate().unwrap();
        for kind in [ActionKind::Build, ActionKind::Run, ActionKind::BuildRun] {
            draft.add_action(kind, &settings).unwrap();
            let action = draft.app.action_of(kind).unwrap();
            assert_eq!(action.retention, settings.default_retention(kind));
            assert_eq!(action.when_running, settings.default_when_running);
            assert!(action.script.is_none());
            assert!(draft.add_action(kind, &settings).is_err());
        }
        draft.add_action(ActionKind::Custom, &settings).unwrap();
        draft.app.actions[draft.selected].id = "custom-build".into();
        draft.app.validate().unwrap();
    }

    #[test]
    fn retention_controls_round_trip() {
        assert_eq!(parse_retention(0, "").unwrap(), Retention::Off);
        assert_eq!(parse_retention(2, "x").unwrap(), Retention::Forever);
        assert_eq!(
            parse_retention(1, " 7 ").unwrap(),
            Retention::Last { count: 7 }
        );
        assert!(parse_retention(1, "0").is_err());
        assert!(parse_retention(1, "abc").is_err());
        assert_eq!(
            retention_controls(Retention::Last { count: 3 }),
            (1, "3".into())
        );
    }

    #[test]
    fn when_running_mapping() {
        for rule in [
            WhenRunning::Ask,
            WhenRunning::Reject,
            WhenRunning::Restart,
            WhenRunning::Parallel,
        ] {
            assert_eq!(when_running_of(when_running_index(rule)), rule);
        }
    }

    #[test]
    fn groups_are_matched_or_created() {
        let mut groups = Groups {
            groups: vec![Group {
                id: "work".into(),
                name: "Work".into(),
            }],
        };
        assert_eq!(resolve_group(&mut groups, ""), (String::new(), false));
        assert_eq!(resolve_group(&mut groups, "work"), ("work".into(), false));
        assert_eq!(resolve_group(&mut groups, "WORK"), ("work".into(), false));
        assert_eq!(
            resolve_group(&mut groups, "Side Projects"),
            ("side-projects".into(), true)
        );
        assert_eq!(groups.groups.len(), 2);
    }

    #[test]
    fn env_table_validation() {
        let model = VecModel::from(vec![
            EnvRow {
                key: "A".into(),
                value: "1".into(),
            },
            EnvRow {
                key: " ".into(),
                value: "ignored".into(),
            },
        ]);
        assert_eq!(env_of(&model).unwrap().len(), 1);
        model.push(EnvRow {
            key: "A".into(),
            value: "2".into(),
        });
        assert!(env_of(&model).is_err());
    }
}
