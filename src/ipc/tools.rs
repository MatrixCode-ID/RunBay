//! Execution of the MCP tools inside the GUI process.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use super::host::{Host, Undo};
use crate::ansi;
use crate::data::{
    ActionDef, ActionKind, AppDef, IconSize, LoadedApp, Retention, RunRecord, RunStatus, Shell,
    Source, ThemePref, WhenRunning, ids,
};
use crate::runner::{Run, RunInfo, RunRequest, StartOutcome};

/// Default number of output lines returned with a finished run.
const DEFAULT_TAIL: usize = 200;
/// Default wait for `run_action` with `wait` and for `wait_run`.
const DEFAULT_TIMEOUT: u64 = 600;

fn arg_str<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("missing string argument \"{name}\""))
}

fn opt_str<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str)
}

fn opt_u64(args: &Value, name: &str) -> Option<u64> {
    args.get(name).and_then(Value::as_u64)
}

fn opt_bool(args: &Value, name: &str) -> Option<bool> {
    args.get(name).and_then(Value::as_bool)
}

fn opt_env(args: &Value, name: &str) -> Result<Option<BTreeMap<String, String>>> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => Ok(Some(
            serde_json::from_value(v.clone())
                .with_context(|| format!("\"{name}\" must map names to strings"))?,
        )),
    }
}

fn parse<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<Option<T>> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => Ok(Some(
            serde_json::from_value(v.clone())
                .with_context(|| format!("invalid value for \"{name}\""))?,
        )),
    }
}

fn load_app(host: &Host, id: &str) -> Result<AppDef> {
    match host.store.load_app(id) {
        LoadedApp::Ok(app) => Ok(app),
        LoadedApp::Errored { error, .. } if host.store.app_dir(id).exists() => {
            bail!("app \"{id}\" cannot be loaded: {error}")
        }
        LoadedApp::Errored { .. } => bail!("there is no app \"{id}\""),
    }
}

fn app_name(host: &Host, id: &str) -> String {
    match host.store.load_app(id) {
        LoadedApp::Ok(app) => app.name,
        _ => id.to_string(),
    }
}

/// The operation of a tool call in plain words, for the confirmation dialog.
pub fn describe(host: &Host, tool: &str, args: &Value) -> String {
    let app = || {
        opt_str(args, "app_id")
            .map(|id| app_name(host, id))
            .unwrap_or_default()
    };
    let action = || {
        let app_id = opt_str(args, "app_id").unwrap_or_default();
        let action_id = opt_str(args, "action_id").unwrap_or_default();
        match host.store.load_app(app_id) {
            LoadedApp::Ok(a) => a
                .action(action_id)
                .map(|x| x.name.clone())
                .unwrap_or_else(|| opt_str(args, "name").unwrap_or(action_id).to_string()),
            _ => action_id.to_string(),
        }
    };
    let run = || {
        opt_str(args, "run_id")
            .and_then(|id| host.manager.find(id))
            .map(|r| {
                let i = r.info();
                format!("{} › {}", i.app_name, i.action_name)
            })
            .unwrap_or_else(|| opt_str(args, "run_id").unwrap_or_default().to_string())
    };
    let fields = || {
        args.as_object()
            .map(|o| {
                o.keys()
                    .filter(|k| !matches!(k.as_str(), "app_id" | "action_id"))
                    .map(|k| k.replace('_', " "))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    match tool {
        "run_action" => format!("Run \"{}\" of {}", action(), app()),
        "wait_run" => format!("Wait for {}", run()),
        "stop_run" => format!("Stop {}", run()),
        "create_app" => format!(
            "Add the app \"{}\"",
            opt_str(args, "name").unwrap_or_default()
        ),
        "update_app" => format!("Change {} ({})", app(), fields()),
        "delete_app" => format!("Remove {} and its whole history", app()),
        "set_app_icon" => format!("Change the icon of {}", app()),
        "set_action_icon" => format!("Change the icon of {} › {}", app(), action()),
        "set_action" => format!("Change {} › {} ({})", app(), action(), fields()),
        "delete_action" => format!("Remove {} › {}", app(), action()),
        "update_settings" => format!("Change settings ({})", fields()),
        other => other.to_string(),
    }
}

/// Executes a tool; `progress` receives new output lines while a run is waited on.
pub fn execute(
    host: &Host,
    agent: &str,
    tool: &str,
    args: &Value,
    progress: &mut dyn FnMut(u64, Vec<String>),
) -> Result<Value> {
    match tool {
        "list_apps" => list_apps(host),
        "get_app" => get_app(host, arg_str(args, "app_id")?),
        "list_runs" => list_runs(host, args),
        "get_run" => {
            let id = arg_str(args, "run_id")?;
            let (record, app_id) = find_run(host, id)?;
            Ok(record_json(host, &app_id, &record))
        }
        "read_output" => read_output(host, args),
        "get_settings" => Ok(serde_json::to_value(&*host.settings.read().unwrap())?),
        "run_action" => run_action(host, agent, args, progress),
        "wait_run" => {
            let id = arg_str(args, "run_id")?;
            let run = host
                .manager
                .find(id)
                .with_context(|| format!("run \"{id}\" is not active or recent"))?;
            let timeout = opt_u64(args, "timeout_seconds").unwrap_or(DEFAULT_TIMEOUT);
            let tail = opt_u64(args, "tail").map_or(DEFAULT_TAIL, |t| t as usize);
            Ok(wait(&run, timeout, tail, progress))
        }
        "stop_run" => {
            let id = arg_str(args, "run_id")?;
            if !host.manager.stop(id) {
                bail!("run \"{id}\" is not running");
            }
            let run = host.manager.find(id).context("the run disappeared")?;
            run.wait(Duration::from_secs(10));
            Ok(run_json(&run, 0, run.is_done()))
        }
        "create_app" => create_app(host, agent, args),
        "update_app" => update_app(host, agent, args),
        "delete_app" => delete_app(host, agent, args),
        "set_app_icon" => set_app_icon(host, agent, args),
        "set_action_icon" => set_action_icon(host, agent, args),
        "set_action" => set_action(host, agent, args),
        "delete_action" => delete_action(host, agent, args),
        "update_settings" => update_settings(host, agent, args),
        other => bail!("unknown tool \"{other}\""),
    }
}

fn last_status(host: &Host, app_id: &str) -> Option<String> {
    if !host.manager.active_for_app(app_id).is_empty() {
        return Some("running".into());
    }
    let memory = host.manager.last_for_app(app_id).map(|r| r.info());
    let disk = host
        .store
        .history(app_id)
        .read()
        .ok()
        .and_then(|r| r.last().cloned());
    match (memory, disk) {
        (Some(m), Some(d)) if d.start > m.start => Some(d.status.as_str().into()),
        (Some(m), _) => Some(m.status.as_str().into()),
        (None, Some(d)) => Some(d.status.as_str().into()),
        (None, None) => None,
    }
}

fn list_apps(host: &Host) -> Result<Value> {
    let apps: Vec<Value> = host
        .store
        .load_apps()
        .into_iter()
        .map(|loaded| match loaded {
            LoadedApp::Ok(app) => json!({
                "id": app.id,
                "name": app.name,
                "group": app.group,
                "order": app.order,
                "last_status": last_status(host, &app.id),
                "actions": app.actions.iter().map(|a| json!({
                    "id": a.id,
                    "name": a.name,
                    "kind": a.kind,
                    "icon": a.icon,
                    "has_script": a.script.is_some(),
                })).collect::<Vec<_>>(),
            }),
            LoadedApp::Errored { id, error } => json!({ "id": id, "error": error }),
        })
        .collect();
    Ok(json!({ "apps": apps }))
}

fn get_app(host: &Host, id: &str) -> Result<Value> {
    let app = load_app(host, id)?;
    let mut value = serde_json::to_value(&app)?;
    let scripts: Map<String, Value> = app
        .actions
        .iter()
        .filter(|a| a.script.is_some())
        .map(|a| {
            let text = host.store.read_script(&app, &a.id);
            (
                a.id.clone(),
                Value::String(text.trim_start_matches('\u{feff}').to_string()),
            )
        })
        .collect();
    value["scripts"] = Value::Object(scripts);
    value["dataFolder"] = Value::String(host.store.app_dir(id).to_string_lossy().into_owned());
    Ok(value)
}

fn info_record(info: &RunInfo) -> RunRecord {
    RunRecord {
        run_id: info.run_id.clone(),
        action_id: info.action_id.clone(),
        start: info.start,
        end: info.end,
        exit_code: info.exit_code,
        status: info.status,
        source: info.source,
        agent: info.agent.clone(),
        log: String::new(),
    }
}

/// Finds a run in memory or in any app's history. Returns the record and its app id.
fn find_run(host: &Host, run_id: &str) -> Result<(RunRecord, String)> {
    if let Some(run) = host.manager.find(run_id) {
        let info = run.info();
        let mut record = info_record(&info);
        if let Some(path) = &info.log_path {
            record.log = path.to_string_lossy().into_owned();
        }
        return Ok((record, info.app_id));
    }
    for loaded in host.store.load_apps() {
        let id = loaded.id().to_string();
        if let Ok(records) = host.store.history(&id).read()
            && let Some(r) = records.into_iter().find(|r| r.run_id == run_id)
        {
            return Ok((r, id));
        }
    }
    bail!("there is no run \"{run_id}\"")
}

fn record_json(host: &Host, app_id: &str, r: &RunRecord) -> Value {
    let log = if r.log.is_empty() {
        None
    } else if std::path::Path::new(&r.log).is_absolute() {
        Some(r.log.clone())
    } else {
        Some(
            host.store
                .history(app_id)
                .log_path(r)
                .to_string_lossy()
                .into_owned(),
        )
    };
    json!({
        "run_id": r.run_id,
        "app_id": app_id,
        "action_id": r.action_id,
        "status": r.status,
        "exit_code": r.exit_code,
        "start": r.start,
        "end": r.end,
        "source": r.source,
        "agent": r.agent,
        "log_path": log,
    })
}

/// Every run known on disk or in memory (memory wins: it has the live status and includes
/// runs without history), with its app id. `app_filter` limits it to one app.
pub(crate) fn all_runs(host: &Host, app_filter: Option<&str>) -> Vec<(RunRecord, String)> {
    let mut runs: Vec<(RunRecord, String)> = Vec::new();
    for loaded in host.store.load_apps() {
        let id = loaded.id().to_string();
        if app_filter.is_some_and(|f| f != id) {
            continue;
        }
        for r in host.store.history(&id).read().unwrap_or_default() {
            runs.push((r, id.clone()));
        }
    }
    // Runs in memory have the live status and include runs without history (retention off).
    let mut memory: Vec<Run> = host.manager.active();
    for loaded in host.store.load_apps() {
        if let Some(r) = host.manager.last_for_app(loaded.id()) {
            memory.push(r);
        }
    }
    for run in memory {
        let info = run.info();
        if app_filter.is_some_and(|f| f != info.app_id) {
            continue;
        }
        let mut record = info_record(&info);
        if let Some(p) = &info.log_path {
            record.log = p.to_string_lossy().into_owned();
        }
        match runs.iter_mut().find(|(r, _)| r.run_id == info.run_id) {
            Some(existing) => existing.0 = record,
            None => runs.push((record, info.app_id.clone())),
        }
    }
    runs
}

fn list_runs(host: &Host, args: &Value) -> Result<Value> {
    let app_filter = opt_str(args, "app_id");
    let action_filter = opt_str(args, "action_id");
    let status_filter = opt_str(args, "status")
        .map(|s| RunStatus::parse(s).with_context(|| format!("unknown status \"{s}\"")));
    let status_filter = status_filter.transpose()?;
    let source_filter: Option<Source> = parse(args, "source")?;
    let limit = opt_u64(args, "limit").unwrap_or(50) as usize;

    let mut runs = all_runs(host, app_filter);
    runs.retain(|(r, _)| {
        action_filter.is_none_or(|f| f == r.action_id)
            && status_filter.is_none_or(|f| f == r.status)
            && source_filter.is_none_or(|f| f == r.source)
    });
    runs.sort_by(|a, b| b.0.start.cmp(&a.0.start).then(b.0.run_id.cmp(&a.0.run_id)));
    runs.truncate(limit);
    Ok(json!({
        "runs": runs.iter().map(|(r, app)| record_json(host, app, r)).collect::<Vec<_>>()
    }))
}

fn read_output(host: &Host, args: &Value) -> Result<Value> {
    let id = arg_str(args, "run_id")?;
    let tail = opt_u64(args, "tail").map(|t| t as usize);
    let offset = opt_u64(args, "offset");
    let limit = opt_u64(args, "limit").map(|l| l as usize);

    if let Some(run) = host.manager.find(id) {
        let total = run.total_lines();
        let (first, _) = run.lines();
        let in_memory = offset.is_none_or(|o| o >= first);
        if in_memory {
            let (start, lines) = match (tail, offset) {
                (_, Some(o)) => (o, run.range(o, limit.unwrap_or(DEFAULT_TAIL))),
                (Some(t), None) => (total.saturating_sub(t as u64), run.tail(t)),
                (None, None) => (
                    total.saturating_sub(DEFAULT_TAIL as u64),
                    run.tail(DEFAULT_TAIL),
                ),
            };
            return Ok(json!({
                "run_id": id,
                "status": run.info().status,
                "offset": start,
                "total": total,
                "lines": lines.iter().map(|l| ansi::strip(&l.text)).collect::<Vec<_>>(),
            }));
        }
    }
    let (record, app_id) = find_run(host, id)?;
    let path = if std::path::Path::new(&record.log).is_absolute() {
        std::path::PathBuf::from(&record.log)
    } else {
        host.store.history(&app_id).log_path(&record)
    };
    if record.log.is_empty() || !path.exists() {
        bail!("the output of run \"{id}\" is no longer available (history retention)");
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let all: Vec<&str> = text.lines().collect();
    let total = all.len() as u64;
    let (start, slice) = match (tail, offset) {
        (_, Some(o)) => {
            let o = (o as usize).min(all.len());
            let end = (o + limit.unwrap_or(DEFAULT_TAIL)).min(all.len());
            (o, &all[o..end])
        }
        (t, None) => {
            let t = t.unwrap_or(DEFAULT_TAIL).min(all.len());
            (all.len() - t, &all[all.len() - t..])
        }
    };
    Ok(json!({
        "run_id": id,
        "status": record.status,
        "offset": start,
        "total": total,
        "lines": slice,
    }))
}

fn run_json(run: &Run, tail: usize, finished: bool) -> Value {
    let info = run.info();
    let mut value = json!({
        "run_id": info.run_id,
        "app_id": info.app_id,
        "action_id": info.action_id,
        "status": info.status,
        "exit_code": info.exit_code,
        "duration_seconds": info.duration().num_milliseconds() as f64 / 1000.0,
        "finished": finished,
        "log_path": info.log_path,
    });
    if tail > 0 {
        value["tail"] = Value::from(
            run.tail(tail)
                .iter()
                .map(|l| ansi::strip(&l.text))
                .collect::<Vec<_>>(),
        );
    }
    if !finished {
        value["message"] =
            Value::from("The run is still running; use wait_run or read_output to follow it.");
    }
    value
}

/// Waits for a run, sending new lines to `progress` every 250 ms.
fn wait(
    run: &Run,
    timeout_seconds: u64,
    tail: usize,
    progress: &mut dyn FnMut(u64, Vec<String>),
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    let mut sent = run.total_lines();
    loop {
        let finished = run.wait(Duration::from_millis(250));
        let total = run.total_lines();
        if total > sent {
            let lines = run
                .range(sent, (total - sent) as usize)
                .iter()
                .map(|l| ansi::strip(&l.text))
                .collect();
            progress(total, lines);
            sent = total;
        }
        if finished || Instant::now() >= deadline {
            return run_json(run, tail, finished);
        }
    }
}

fn run_action(
    host: &Host,
    agent: &str,
    args: &Value,
    progress: &mut dyn FnMut(u64, Vec<String>),
) -> Result<Value> {
    let decision = match opt_str(args, "if_running") {
        None => None,
        Some("restart") => Some(WhenRunning::Restart),
        Some("parallel") => Some(WhenRunning::Parallel),
        Some(other) => bail!("if_running must be \"restart\" or \"parallel\", not \"{other}\""),
    };
    let request = RunRequest {
        app_id: arg_str(args, "app_id")?.to_string(),
        action_id: arg_str(args, "action_id")?.to_string(),
        source: Source::Mcp,
        agent: Some(agent.to_string()),
        decision,
    };
    let active_list = |active: &[RunInfo]| {
        active
            .iter()
            .map(|i| format!("{} ({})", i.run_id, i.action_name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let run = match host.manager.start(request)? {
        StartOutcome::Started(run) => run,
        StartOutcome::NeedsDecision { active } => bail!(
            "app is running: {}. Pass if_running \"restart\" or \"parallel\".",
            active_list(&active)
        ),
        StartOutcome::Rejected { active } => bail!(
            "app is running and this action rejects starting while it runs: {}",
            active_list(&active)
        ),
    };
    if opt_bool(args, "wait").unwrap_or(false) {
        let timeout = opt_u64(args, "timeout_seconds").unwrap_or(DEFAULT_TIMEOUT);
        let tail = opt_u64(args, "tail").map_or(DEFAULT_TAIL, |t| t as usize);
        Ok(wait(&run, timeout, tail, progress))
    } else {
        Ok(json!({ "run_id": run.id(), "status": "running" }))
    }
}

fn create_app(host: &Host, agent: &str, args: &Value) -> Result<Value> {
    let settings = host.settings.read().unwrap().clone();
    let mut app = host.store.create_app(arg_str(args, "name")?, &settings)?;
    let result = (|| -> Result<()> {
        apply_app_fields(&mut app, args)?;
        host.store.save_app(&app)
    })();
    if let Err(e) = result {
        let _ = host.store.delete_app(&app.id);
        return Err(e);
    }
    host.changed(
        agent,
        format!("{agent} added the app {}", app.name),
        Undo::App {
            app_id: app.id.clone(),
            before: None,
        },
    );
    Ok(json!({ "app": app }))
}

fn apply_app_fields(app: &mut AppDef, args: &Value) -> Result<()> {
    if let Some(group) = opt_str(args, "group") {
        app.group = group.trim().to_string();
    }
    if let Some(order) = args.get("order").and_then(Value::as_i64) {
        app.order = order as i32;
    }
    if let Some(dir) = opt_str(args, "working_directory") {
        app.working_directory = dir.trim().to_string();
    }
    if let Some(shell) = parse::<Shell>(args, "shell")? {
        app.shell = shell;
    }
    if let Some(env) = opt_env(args, "env")? {
        app.env = env;
    }
    Ok(())
}

fn update_app(host: &Host, agent: &str, args: &Value) -> Result<Value> {
    let id = arg_str(args, "app_id")?;
    let mut app = load_app(host, id)?;
    let before = host.snapshot(id)?;
    if let Some(name) = opt_str(args, "name") {
        let name = name.trim();
        if name.is_empty() {
            bail!("the app name is empty");
        }
        host.store.ensure_unique_name(name, Some(id))?;
        app.name = name.to_string();
    }
    apply_app_fields(&mut app, args)?;
    host.store.save_app(&app)?;
    host.changed(
        agent,
        format!("{agent} updated {}", app.name),
        Undo::App {
            app_id: id.to_string(),
            before: Some(before),
        },
    );
    Ok(json!({ "app": app }))
}

fn delete_app(host: &Host, agent: &str, args: &Value) -> Result<Value> {
    let id = arg_str(args, "app_id")?;
    let name = app_name(host, id);
    if !host.store.app_dir(id).exists() {
        bail!("there is no app \"{id}\"");
    }
    for run in host.manager.active_for_app(id) {
        run.stop();
        run.wait(Duration::from_secs(10));
    }
    let trash = host.store.trash_app(id)?;
    host.changed(
        agent,
        format!("{agent} removed {name}"),
        Undo::Deleted {
            app_id: id.to_string(),
            trash,
        },
    );
    Ok(json!({ "deleted": id }))
}

fn set_app_icon(host: &Host, agent: &str, args: &Value) -> Result<Value> {
    let id = arg_str(args, "app_id")?;
    let source = std::path::PathBuf::from(arg_str(args, "path")?);
    let ext = crate::data::icon_extension(&source)
        .context("the icon must be a .png, .ico, or .svg file")?;
    let mut app = load_app(host, id)?;
    let before = host.snapshot(id)?;
    host.store.set_icon(&mut app, &source, &ext)?;
    host.store.save_app(&app)?;
    host.changed(
        agent,
        format!("{agent} changed the icon of {}", app.name),
        Undo::App {
            app_id: id.to_string(),
            before: Some(before),
        },
    );
    Ok(json!({ "icon": app.icon }))
}

fn set_action_icon(host: &Host, agent: &str, args: &Value) -> Result<Value> {
    let app_id = arg_str(args, "app_id")?;
    let action_id = arg_str(args, "action_id")?;
    let mut app = load_app(host, app_id)?;
    let action_name = app
        .action(action_id)
        .with_context(|| format!("{} has no action \"{action_id}\"", app.name))?
        .name
        .clone();
    let before = host.snapshot(app_id)?;
    match opt_str(args, "path").filter(|p| !p.trim().is_empty()) {
        Some(path) => {
            let source = std::path::PathBuf::from(path);
            let ext = crate::data::icon_extension(&source)
                .context("the icon must be a .png, .ico, or .svg file")?;
            host.store
                .set_action_icon(&mut app, action_id, &source, &ext)?;
        }
        None => host.store.remove_action_icon(&mut app, action_id),
    }
    host.store.save_app(&app)?;
    host.changed(
        agent,
        format!("{agent} changed the icon of {} › {action_name}", app.name),
        Undo::App {
            app_id: app_id.to_string(),
            before: Some(before),
        },
    );
    Ok(json!({ "iconFile": app.action(action_id).and_then(|a| a.icon_file.clone()) }))
}

fn set_action(host: &Host, agent: &str, args: &Value) -> Result<Value> {
    let app_id = arg_str(args, "app_id")?;
    let mut app = load_app(host, app_id)?;
    let before = host.snapshot(app_id)?;
    let settings = host.settings.read().unwrap().clone();

    let requested = opt_str(args, "action_id").map(str::to_string);
    let requested_kind = parse::<ActionKind>(args, "kind")?;
    let exists = requested
        .as_deref()
        .is_some_and(|id| app.action(id).is_some());
    let action_id = if exists {
        if requested_kind
            .is_some_and(|kind| app.action(requested.as_deref().unwrap()).unwrap().kind != kind)
        {
            bail!("an existing action's kind cannot be changed");
        }
        requested.unwrap()
    } else {
        let kind = requested_kind.unwrap_or(match requested.as_deref() {
            Some("build") => ActionKind::Build,
            Some("run") => ActionKind::Run,
            Some("build-run") => ActionKind::BuildRun,
            _ => ActionKind::Custom,
        });
        if kind != ActionKind::Custom && app.action_of(kind).is_some() {
            bail!("the app already has a {kind:?} action");
        }
        let default = ActionDef::new(kind, &settings);
        let name = opt_str(args, "name")
            .or(requested.as_deref())
            .or(if kind == ActionKind::Custom {
                None
            } else {
                Some(default.name.as_str())
            })
            .context("a new custom action needs a name or an action_id")?
            .to_string();
        let id = match requested {
            Some(id) if ids::is_slug(&id) => id,
            Some(id) => {
                bail!("\"{id}\" is not a valid action id (lowercase letters, digits, dashes)")
            }
            None => ids::unique_slug(&name, |s| app.action(s).is_some()),
        };
        app.actions.push(ActionDef {
            id: id.clone(),
            name,
            kind,
            icon: None,
            icon_file: None,
            script: None,
            working_directory: None,
            env: BTreeMap::new(),
            when_running: settings.default_when_running,
            retention: settings.default_retention(kind),
        });
        id
    };
    let mut changed = Vec::new();
    {
        let action = app.action_mut(&action_id).expect("the action exists");
        if let Some(icon) = args.get("icon") {
            action.icon = match icon {
                Value::Null => None,
                Value::String(name) if name.is_empty() => None,
                Value::String(name) => Some(name.clone()),
                _ => bail!("icon must be a Material Symbol name or null"),
            };
            changed.push("icon");
        }
        if let Some(name) = opt_str(args, "name") {
            if name.trim().is_empty() {
                bail!("the action name is empty");
            }
            action.name = name.trim().to_string();
            changed.push("name");
        }
        if let Some(dir) = args.get("working_directory") {
            action.working_directory = dir
                .as_str()
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map(str::to_string);
            changed.push("working directory");
        }
        if let Some(env) = opt_env(args, "env")? {
            action.env = env;
            changed.push("environment");
        }
        if let Some(rule) = parse::<WhenRunning>(args, "when_running")? {
            action.when_running = rule;
            changed.push("when running");
        }
        if let Some(retention) = parse::<Retention>(args, "retention")? {
            action.retention = retention;
            changed.push("retention");
        }
    }
    app.validate().map_err(anyhow::Error::msg)?;
    if let Some(script) = opt_str(args, "script") {
        host.store.write_script(&mut app, &action_id, script)?;
        changed.push("script");
    }
    host.store.save_app(&app)?;
    if changed.contains(&"retention") {
        // A smaller retention shrinks the history right away.
        let _ = host.store.history(&app.id).apply_retention(&app);
    }
    let action = app.action(&action_id).expect("the action exists");
    let what = if exists {
        changed.join(", ")
    } else {
        "new action".to_string()
    };
    host.changed(
        agent,
        format!("{agent} updated {} › {} ({what})", app.name, action.name),
        Undo::App {
            app_id: app_id.to_string(),
            before: Some(before),
        },
    );
    Ok(json!({ "action": action }))
}

fn delete_action(host: &Host, agent: &str, args: &Value) -> Result<Value> {
    let app_id = arg_str(args, "app_id")?;
    let action_id = arg_str(args, "action_id")?;
    let mut app = load_app(host, app_id)?;
    let action = app
        .action(action_id)
        .with_context(|| format!("{} has no action \"{action_id}\"", app.name))?
        .clone();
    if host.manager.active_for_app(app_id).iter().any(|run| {
        let info = run.info();
        info.action_id == action_id
            || (info.kind == ActionKind::BuildRun
                && matches!(action.kind, ActionKind::Build | ActionKind::Run))
    }) {
        bail!("stop the action before removing it");
    }
    let before = host.snapshot(app_id)?;
    if action.script.is_some() {
        host.store.write_script(&mut app, action_id, "")?;
    }
    host.store.remove_action_icon(&mut app, action_id);
    app.actions.retain(|a| a.id != action_id);
    host.store.save_app(&app)?;
    host.changed(
        agent,
        format!("{agent} removed {} › {}", app.name, action.name),
        Undo::App {
            app_id: app_id.to_string(),
            before: Some(before),
        },
    );
    Ok(json!({ "deleted": action_id }))
}

fn update_settings(host: &Host, agent: &str, args: &Value) -> Result<Value> {
    if args.get("mcp_access").is_some() {
        bail!("MCP access can only be changed in RunBay itself");
    }
    let before = host.settings.read().unwrap().clone();
    let mut s = before.clone();
    if let Some(v) = parse::<ThemePref>(args, "theme")? {
        s.theme = v;
    }
    if let Some(v) = parse::<crate::data::ConsoleThemePref>(args, "console_theme")? {
        s.console_theme = v;
    }
    if let Some(v) = opt_bool(args, "autostart") {
        s.autostart = v;
    }
    if let Some(v) = parse::<IconSize>(args, "icon_size")? {
        s.icon_size = v;
    }
    if let Some(v) = opt_bool(args, "always_on_top_compact") {
        s.always_on_top_compact = v;
    }
    if let Some(v) = opt_bool(args, "always_on_top_icon") {
        s.always_on_top_icon = v;
    }
    if let Some(v) = opt_bool(args, "taskbar_full") {
        s.taskbar_full = v;
    }
    if let Some(v) = opt_bool(args, "taskbar_compact") {
        s.taskbar_compact = v;
    }
    if let Some(v) = opt_u64(args, "output_line_limit") {
        if v < 100 {
            bail!("output_line_limit must be at least 100");
        }
        s.output_line_limit = v as u32;
    }
    if let Some(v) = parse::<WhenRunning>(args, "default_when_running")? {
        s.default_when_running = v;
    }
    if let Some(v) = opt_bool(args, "auto_update") {
        s.auto_update = v;
    }
    host.store.save_settings(&s)?;
    host.manager.set_line_limit(s.output_line_limit);
    *host.settings.write().unwrap() = s.clone();
    host.changed(
        agent,
        format!("{agent} changed the settings"),
        Undo::Settings(Box::new(before)),
    );
    Ok(serde_json::to_value(&s)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_and_rules_parse_from_agent_json() {
        let args =
            json!({ "retention": { "mode": "last", "count": 3 }, "when_running": "restart" });
        assert_eq!(
            parse::<Retention>(&args, "retention").unwrap(),
            Some(Retention::Last { count: 3 })
        );
        assert_eq!(
            parse::<WhenRunning>(&args, "when_running").unwrap(),
            Some(WhenRunning::Restart)
        );
        assert!(parse::<Shell>(&json!({ "shell": "bash" }), "shell").is_err());
    }
}
