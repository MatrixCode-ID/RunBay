//! The app editor page: a draft of the app with its actions and scripts, committed on Save.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, Result, bail};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use super::{Controller, convert};
use crate::data::{
    ActionDef, ActionKind, AppDef, Group, Groups, Retention, Shell, WhenRunning, ids,
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
    app_env: Rc<VecModel<EnvRow>>,
    action_env: Rc<VecModel<EnvRow>>,
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
        };
        let ui = self.ui();
        let g = ui.global::<EditorBridge>();
        g.set_is_new(is_new);
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

    /// Shows the selected action of the draft.
    fn load_selected_action(&self) {
        let ui = self.ui();
        let g = ui.global::<EditorBridge>();
        let editor = self.editor.borrow();
        let Some(d) = editor.as_ref() else { return };
        let action = &d.app.actions[d.selected];
        g.set_actions(ModelRc::new(VecModel::from(
            d.app
                .actions
                .iter()
                .map(|a| EditorAction {
                    id: a.id.clone().into(),
                    name: a.name.clone().into(),
                    kind: format!("{:?}", a.kind).to_lowercase().into(),
                    has_script: d.scripts.get(&a.id).is_some_and(|s| !s.trim().is_empty()),
                })
                .collect::<Vec<_>>(),
        )));
        g.set_selected_action(d.selected as i32);
        g.set_action_name(action.name.clone().into());
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
        let env = env_of(&d.action_env)?;
        let retention = parse_retention(g.get_retention_index(), &g.get_retention_count());
        let action = &mut d.app.actions[d.selected];
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
        g.on_choose_folder(with(|c| {
            let owner = convert::hwnd(c.ui().window());
            if let Some(path) = platform::dialogs::pick_folder(owner, "Working directory") {
                c.ui()
                    .global::<EditorBridge>()
                    .set_working_directory(path.to_string_lossy().as_ref().into());
            }
        }));
        g.on_add_action(with(|c| {
            if let Err(e) = c.commit_action() {
                return c.editor_error(e);
            }
            if let Some(d) = c.editor.borrow_mut().as_mut() {
                let settings = c.settings.read().unwrap().clone();
                let id = ids::unique_slug("custom", |s| d.app.action(s).is_some());
                d.app.actions.push(ActionDef {
                    id: id.clone(),
                    name: "New action".into(),
                    kind: ActionKind::Custom,
                    script: None,
                    working_directory: None,
                    env: Default::default(),
                    when_running: settings.default_when_running,
                    retention: settings.default_retention(ActionKind::Custom),
                });
                d.scripts.insert(id, String::new());
                d.selected = d.app.actions.len() - 1;
            }
            c.load_selected_action();
        }));
        g.on_remove_action(with(|c| {
            if let Some(d) = c.editor.borrow_mut().as_mut()
                && d.app.actions[d.selected].kind == ActionKind::Custom
            {
                let removed = d.app.actions.remove(d.selected);
                d.scripts.remove(&removed.id);
                d.selected = d.selected.saturating_sub(1);
            }
            c.load_selected_action();
        }));
        g.on_open_external(with(|c| {
            let result = (|| -> Result<()> {
                c.commit_action()?;
                let mut editor = c.editor.borrow_mut();
                let d = editor.as_mut().context("no draft")?;
                let action_id = d.app.actions[d.selected].id.clone();
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
            {
                let action_id = d.app.actions[d.selected].id.clone();
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
                    d.selected = (index.max(0) as usize).min(d.app.actions.len() - 1);
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
                    // Custom actions move among themselves, below the standard three.
                    if target >= 0
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
                a.script = previous.action(&a.id).and_then(|p| p.script.clone());
                a
            })
            .collect();
        app.validate().map_err(anyhow::Error::msg)?;
        for (action_id, text) in &d.scripts {
            if app.action(action_id).is_some() {
                let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
                self.store.write_script(&mut app, action_id, &text)?;
            }
        }
        // Scripts of removed actions.
        for old in &previous.actions {
            if app.action(&old.id).is_none()
                && let Some(path) = self.store.script_path(&previous, &old.id)
            {
                let _ = std::fs::remove_file(path);
            }
        }
        match d.icon.clone() {
            Some(IconChange::Set(path)) => {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .filter(|e| matches!(e.as_str(), "png" | "ico" | "svg"))
                    .context("the icon must be a .png, .ico, or .svg file")?;
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
