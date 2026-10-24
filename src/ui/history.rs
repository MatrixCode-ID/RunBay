//! The Action History flyout: runs of every app, newest first, with filters.

use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{Controller, convert};
use crate::data::{LoadedApp, RunStatus, Source};
use crate::ipc::tools::all_runs;
use crate::{HistoryBridge, HistoryRow};

/// An app id, its name, and its actions as `(id, name)`.
type AppInfo = (String, String, Vec<(String, String)>);

/// How many entries the flyout shows at most.
const MAX_ROWS: usize = 500;

impl Controller {
    pub(super) fn toggle_history(&self) {
        let ui = self.ui();
        let open = !ui.get_history_open();
        ui.set_history_open(open);
        if open {
            let g = ui.global::<HistoryBridge>();
            g.set_app_filter(0);
            g.set_action_filter(0);
            g.set_status_filter(0);
            g.set_source_filter(0);
            self.refresh_history();
        }
    }

    /// Rebuilds the flyout rows (when it is open).
    pub(super) fn refresh_history(&self) {
        let ui = self.ui();
        if !ui.get_history_open() {
            return;
        }
        let g = ui.global::<HistoryBridge>();
        let apps: Vec<AppInfo> = self
            .state
            .borrow()
            .apps
            .iter()
            .filter_map(|a| match a {
                LoadedApp::Ok(app) => Some((
                    app.id.clone(),
                    app.name.clone(),
                    app.actions
                        .iter()
                        .map(|x| (x.id.clone(), x.name.clone()))
                        .collect(),
                )),
                _ => None,
            })
            .collect();
        let mut action_names: Vec<String> = apps
            .iter()
            .flat_map(|(_, _, actions)| actions.iter().map(|(_, n)| n.clone()))
            .collect();
        action_names.sort();
        action_names.dedup();

        let mut app_filters = vec![SharedString::from("All apps")];
        app_filters.extend(apps.iter().map(|(_, n, _)| SharedString::from(n.as_str())));
        let mut action_filters = vec![SharedString::from("All actions")];
        action_filters.extend(action_names.iter().map(|n| SharedString::from(n.as_str())));
        g.set_app_filters(ModelRc::new(VecModel::from(app_filters)));
        g.set_action_filters(ModelRc::new(VecModel::from(action_filters)));

        let app_filter = usize::try_from(g.get_app_filter() - 1)
            .ok()
            .and_then(|i| apps.get(i))
            .map(|a| a.0.clone());
        let action_filter = usize::try_from(g.get_action_filter() - 1)
            .ok()
            .and_then(|i| action_names.get(i).cloned());
        let status_filter = match g.get_status_filter() {
            1 => Some(RunStatus::Running),
            2 => Some(RunStatus::Success),
            3 => Some(RunStatus::Failed),
            4 => Some(RunStatus::Stopped),
            _ => None,
        };
        let source_filter = match g.get_source_filter() {
            1 => Some(Source::Ui),
            2 => Some(Source::Mcp),
            _ => None,
        };
        let displayed = self.state.borrow().displayed.clone();

        let mut runs = all_runs(&self.host, app_filter.as_deref());
        runs.sort_by(|a, b| b.0.start.cmp(&a.0.start).then(b.0.run_id.cmp(&a.0.run_id)));
        let rows: Vec<HistoryRow> = runs
            .iter()
            .filter_map(|(r, app_id)| {
                let (_, app_name, actions) = apps.iter().find(|a| &a.0 == app_id)?;
                let action_name = actions
                    .iter()
                    .find(|(id, _)| id == &r.action_id)
                    .map_or(r.action_id.clone(), |(_, n)| n.clone());
                let keep = action_filter.as_ref().is_none_or(|f| f == &action_name)
                    && status_filter.is_none_or(|f| f == r.status)
                    && source_filter.is_none_or(|f| f == r.source);
                keep.then(|| HistoryRow {
                    run_id: r.run_id.clone().into(),
                    title: format!("{app_name} · {action_name}").into(),
                    status: r.status.as_str().into(),
                    busy: self
                        .manager
                        .find(&r.run_id)
                        .is_some_and(|run| run.info().is_busy()),
                    agent: r.agent.clone().unwrap_or_default().into(),
                    time: convert::time_label(&r.start).into(),
                    duration: r
                        .end
                        .map(|end| convert::duration_label(end - r.start))
                        .unwrap_or_default()
                        .into(),
                    selected: displayed.as_deref() == Some(r.run_id.as_str()),
                })
            })
            .take(MAX_ROWS)
            .collect();
        g.set_rows(ModelRc::new(VecModel::from(rows)));
    }

    pub(super) fn bind_history(self: &Rc<Self>) {
        let ui = self.ui();
        let g = ui.global::<HistoryBridge>();
        let weak = Rc::downgrade(self);
        g.on_filters_changed(move || {
            if let Some(c) = weak.upgrade() {
                c.refresh_history();
            }
        });
        let weak = Rc::downgrade(self);
        g.on_open_run(move |run_id| {
            if let Some(c) = weak.upgrade() {
                c.display(Some(run_id.to_string()));
                c.refresh_nav();
                c.refresh_history();
            }
        });
        let weak = Rc::downgrade(self);
        g.on_close(move || {
            if let Some(c) = weak.upgrade() {
                c.toggle_history();
            }
        });
    }
}
