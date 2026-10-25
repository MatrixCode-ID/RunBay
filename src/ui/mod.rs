//! The GUI process: window setup, the controller that maps data and runs to the Slint
//! models, and the pipe server for single instance activation and agents.

mod agent;
mod convert;
mod editor;
mod history;
mod lifecycle;
mod mcp_setup;
mod modes;
mod native;
mod output_selection;
mod selection;
mod settings;
mod updates;

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::ansi::AnsiParser;
use crate::data::{ActionKind, AppDef, Groups, LoadedApp, RunRecord, Settings, Store, ThemePref};
use crate::ipc::host::Host;
use crate::ipc::{self, Hello, pipe::PipeListener};
use crate::platform;
use crate::runner::{OutputLine, Run, RunEvent, RunManager, RunRequest, StartOutcome};
use crate::{ActionButton, AppRow, MainWindow, OutputRow, RunRow};

/// How RunBay was started.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The data folder.
    pub data_dir: PathBuf,
    /// Started with Windows (`--autostart`).
    pub autostart: bool,
    /// Development option for tests: serve the pipe without a window.
    pub headless: bool,
    /// `data_dir` was given with `--data-dir` (kept when restarting after an update).
    pub custom_data_dir: bool,
}

/// Selects the femtovg (OpenGL) renderer, or the software renderer when OpenGL is missing.
fn select_backend() {
    let femtovg = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("femtovg".into())
        .select();
    if let Err(e) = femtovg {
        log::warn!("femtovg renderer unavailable ({e}); using the software renderer");
        if let Err(e) = slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name("software".into())
            .select()
        {
            log::error!("cannot select the software renderer: {e}");
        }
    }
}

/// Runs the GUI until the window closes. `listener` is the bound single instance pipe.
pub fn run(options: Options, listener: PipeListener) -> Result<()> {
    let store = Store::open(&options.data_dir)?;
    store.empty_trash();
    let settings = store.load_settings();
    let manager = RunManager::new(store.clone(), settings.output_line_limit);
    manager.startup_cleanup();
    let settings = Arc::new(RwLock::new(settings));

    if options.headless {
        let host = Host::new(store, manager, settings, Arc::new(agent::HeadlessAgent));
        listener.serve(move |pipe| handle_connection(&host, pipe));
        return Ok(());
    }

    select_backend();
    let host = Host::new(
        store.clone(),
        manager.clone(),
        settings.clone(),
        Arc::new(agent::UiAgent),
    );
    let window = MainWindow::new().context("cannot create the window")?;
    window.set_version(crate::VERSION.into());
    let controller = Rc::new(Controller {
        window: window.as_weak(),
        manager: manager.clone(),
        store,
        settings,
        host: host.clone(),
        editor: RefCell::new(None),
        tray: RefCell::new(None),
        modes: modes::ModeWindows {
            compact: crate::CompactWindow::new().context("cannot create the Compact window")?,
            icon: crate::IconWindow::new().context("cannot create the Icon window")?,
            popup: crate::IconPopup::new().context("cannot create the Icon popup")?,
        },
        popup_timer: RefCell::new(None),
        update: RefCell::new(updates::UpdateUi::default()),
        update_timer: RefCell::new(None),
        clock_timer: RefCell::new(None),
        restart_args: if options.custom_data_dir {
            vec![
                "--data-dir".to_string(),
                options.data_dir.to_string_lossy().into_owned(),
            ]
        } else {
            Vec::new()
        },
        icon_timer: RefCell::new(None),
        state: RefCell::new(State {
            output: Rc::new(VecModel::default()),
            ..State::default()
        }),
    });
    controller.apply_theme();
    window.set_output(ModelRc::from(controller.state.borrow().output.clone()));
    CONTROLLER.with(|c| *c.borrow_mut() = Some(controller.clone()));
    controller.reload_apps();
    controller.bind_callbacks(&window);
    controller.bind_editor();
    controller.bind_app_drag();
    controller.bind_settings();
    controller.bind_history();
    controller.bind_close();
    controller.bind_native_windows();
    controller.bind_modes();
    controller.bind_selection();
    controller.bind_updates();
    controller.restore_full_window();
    controller.state.borrow_mut().mode = controller.store.load_window_state().mode;
    controller.create_tray();
    if controller.settings.read().unwrap().autostart {
        // Keeps the registry path right when the executable moved.
        controller.sync_autostart();
    }

    manager.subscribe(|event| {
        let event = event.clone();
        let _ = slint::invoke_from_event_loop(move || with_controller(|c| c.on_event(event)));
    });

    std::thread::Builder::new()
        .name("runbay-pipe".into())
        .spawn(move || listener.serve(move |pipe| handle_connection(&host, pipe)))
        .context("cannot start the pipe server")?;

    controller.show_window();
    controller.start_update_checks();
    slint::run_event_loop_until_quit().context("the event loop failed")?;
    manager.stop_all(Duration::from_secs(5));
    Ok(())
}

fn handle_connection(host: &Arc<Host>, pipe: std::fs::File) {
    let mut reader = BufReader::new(pipe);
    match ipc::read_hello(&mut reader) {
        Ok(Some(Hello::Activate)) => {
            let _ = slint::invoke_from_event_loop(|| with_controller(|c| c.show_window()));
            let _ = reader.get_mut().write_all(b"{\"ok\":true}\n");
        }
        Ok(Some(hello)) => host.serve(hello, reader),
        Ok(None) => {}
        Err(e) => log::warn!("bad pipe message: {e:#}"),
    }
}

thread_local! {
    static CONTROLLER: RefCell<Option<Rc<Controller>>> = const { RefCell::new(None) };
}

fn with_controller(f: impl FnOnce(&Rc<Controller>)) {
    let controller = CONTROLLER.with(|c| c.borrow().clone());
    if let Some(c) = controller {
        f(&c);
    }
}

#[derive(Default)]
struct State {
    apps: Vec<LoadedApp>,
    groups: Groups,
    /// History records per app, newest last (cache).
    history: HashMap<String, Vec<RunRecord>>,
    /// The run shown in the output panel.
    displayed: Option<String>,
    output: Rc<VecModel<OutputRow>>,
    selection: selection::Selection,
    selection_click: Option<(std::time::Instant, bool, i32, i32, u8)>,
    parser: AnsiParser,
    columns: usize,
    /// A request waiting for the "ask" dialog.
    pending_ask: Option<RunRequest>,
    /// The app waiting for the remove confirmation.
    confirm_app: Option<String>,
    /// Log file of the displayed run when it is not in memory.
    displayed_log: Option<PathBuf>,
    snackbar_generation: u64,
    /// Undo of the agent change shown in the snackbar.
    undo: Option<crate::ipc::host::Undo>,
    /// Agent confirmations waiting to be shown; the first one is visible.
    confirmations: std::collections::VecDeque<agent::PendingConfirmation>,
    /// Aggregated badge of all apps: running > failed > success.
    aggregate: String,
    /// At least one active app is building or running a custom action.
    aggregate_busy: bool,
    /// Connected agents.
    agents: Vec<String>,
    /// The active display mode.
    mode: crate::data::Mode,
    /// Maximize the Full window when it is shown the first time.
    full_maximized: bool,
    /// The output popup of Compact and Icon mode.
    popup: modes::PopupState,
    /// The Full window is shown only for a dialog while another mode is active.
    full_for_dialog: bool,
    /// The floating icon sits on the right edge (its popup opens to the left).
    icon_right: bool,
    /// The exit dialog is the "restart to update" question.
    exit_for_update: bool,
}

struct Controller {
    window: slint::Weak<MainWindow>,
    manager: RunManager,
    store: Store,
    settings: Arc<RwLock<Settings>>,
    host: Arc<Host>,
    state: RefCell<State>,
    editor: RefCell<Option<editor::Draft>>,
    tray: RefCell<Option<platform::tray::Tray>>,
    modes: modes::ModeWindows,
    icon_timer: RefCell<Option<Rc<slint::Timer>>>,
    popup_timer: RefCell<Option<Rc<slint::Timer>>>,
    update: RefCell<updates::UpdateUi>,
    update_timer: RefCell<Option<Rc<slint::Timer>>>,
    /// Advances `Clock.now` once a second while runs are active; `None` while idle.
    clock_timer: RefCell<Option<slint::Timer>>,
    /// Arguments passed again when RunBay restarts itself.
    restart_args: Vec<String>,
}

impl Controller {
    fn ui(&self) -> MainWindow {
        self.window
            .upgrade()
            .expect("the window lives as long as the controller")
    }

    fn apply_theme(&self) {
        let ui = self.ui();
        let mode = match self.settings.read().unwrap().theme {
            ThemePref::System => 0,
            ThemePref::Light => 1,
            ThemePref::Dark => 2,
        };
        ui.set_theme_mode(mode);
        self.modes.compact.set_theme_mode(mode);
        ui.global::<crate::SettingsBridge>().set_theme_index(mode);
        ui.invoke_set_theme(mode);
        self.modes.compact.invoke_set_theme(mode);
        self.modes.icon.invoke_set_theme(mode);
        self.modes.popup.invoke_set_theme(mode);
        self.apply_console_theme();
    }

    fn bind_callbacks(self: &Rc<Self>, ui: &MainWindow) {
        let weak = Rc::downgrade(self);
        let c = move || weak.upgrade();
        {
            let c = c.clone();
            ui.on_cycle_theme(move || {
                if let Some(c) = c() {
                    c.cycle_theme();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_run_action(move |app, action| {
                if let Some(c) = c() {
                    c.start(RunRequest::ui(&app, &action));
                }
            });
        }
        {
            let c = c.clone();
            ui.on_select_run(move |run| {
                if let Some(c) = c() {
                    c.display(Some(run.to_string()));
                    c.refresh_nav();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_stop_run(move |run| {
                if let Some(c) = c() {
                    c.manager.stop(&run);
                }
            });
        }
        {
            let c = c.clone();
            ui.on_stop_app(move |app| {
                if let Some(c) = c() {
                    c.stop_app(&app);
                }
            });
        }
        {
            let c = c.clone();
            ui.on_copy_output(move || {
                if let Some(c) = c() {
                    c.copy_output();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_open_log(move || {
                if let Some(c) = c() {
                    c.open_log();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_clear_output(move || {
                if let Some(c) = c() {
                    let mut st = c.state.borrow_mut();
                    st.output.set_vec(Vec::new());
                    let next = st.selection.next();
                    st.selection.reset(next);
                    st.columns = 0;
                    drop(st);
                    c.ui().set_output_columns(0);
                    c.refresh_selection(false);
                    c.ui().invoke_scroll_to_end();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_add_app(move || {
                if let Some(c) = c() {
                    c.open_editor(None);
                }
            });
        }
        {
            let c = c.clone();
            ui.on_edit_app(move |id| {
                if let Some(c) = c() {
                    c.open_editor(Some(id.to_string()));
                }
            });
        }
        {
            let c = c.clone();
            ui.on_remove_app(move |id| {
                if let Some(c) = c() {
                    c.ask_remove(&id);
                }
            });
        }
        {
            let c = c.clone();
            ui.on_confirm_remove(move || {
                if let Some(c) = c() {
                    c.remove_confirmed();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_ask_answer(move |answer| {
                if let Some(c) = c() {
                    c.answer_ask(&answer);
                }
            });
        }
        {
            let c = c.clone();
            ui.on_open_working_directory(move |id| {
                if let Some(c) = c()
                    && let Some(app) = c.app(&id)
                {
                    let dir = PathBuf::from(app.working_directory.trim());
                    c.report(platform::open_path(&dir));
                }
            });
        }
        {
            let c = c.clone();
            ui.on_open_data_folder(move |id| {
                if let Some(c) = c() {
                    let dir = c.store.app_dir(&id);
                    c.report(platform::open_path(&dir));
                }
            });
        }
        {
            let c = c.clone();
            ui.on_version_clicked(move || {
                if let Some(c) = c() {
                    c.show_about();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_about_notices(move || {
                if let Some(c) = c() {
                    c.open_notices();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_check_updates(move || {
                if let Some(c) = c() {
                    c.check_updates(true);
                }
            });
        }
        {
            let c = c.clone();
            ui.on_open_settings(move || {
                if let Some(c) = c() {
                    c.open_settings();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_toggle_history(move || {
                if let Some(c) = c() {
                    c.toggle_history();
                }
            });
        }
        {
            let c = c.clone();
            ui.on_exit_answer(move |answer| {
                if let Some(c) = c() {
                    c.exit_answer(&answer);
                }
            });
        }
        {
            let c = c.clone();
            ui.on_move_app(move |id, delta| {
                if let Some(c) = c() {
                    c.move_app(&id, delta);
                }
            });
        }
        {
            let c = c.clone();
            ui.on_agent_ask_answer(move |answer| {
                if let Some(c) = c() {
                    c.answer_confirmation(&answer);
                }
            });
        }
        {
            let c = c.clone();
            ui.on_undo_clicked(move || {
                if let Some(c) = c() {
                    c.undo();
                }
            });
        }
    }

    fn report(&self, result: Result<()>) {
        if let Err(e) = result {
            self.snackbar(format!("{e:#}"));
        }
    }

    /// Shows a short message at the bottom of the window for a few seconds.
    fn snackbar(&self, text: String) {
        self.snackbar_for(text, Duration::from_secs(4));
    }

    /// Shows a snackbar for `duration`; returns its generation.
    fn snackbar_for(&self, text: String, duration: Duration) -> u64 {
        let generation = {
            let mut st = self.state.borrow_mut();
            st.snackbar_generation += 1;
            st.snackbar_generation
        };
        self.state.borrow_mut().undo = None;
        let ui = self.ui();
        ui.set_snackbar_undo(false);
        ui.set_snackbar(text.clone().into());
        self.each_mini(|g| {
            g.set_snackbar(text.clone().into());
            g.set_snackbar_undo(false);
        });
        let icon_mode = self.state.borrow().mode == crate::data::Mode::Icon;
        if icon_mode
            && !(self.modes.popup.window().is_visible() && self.modes.popup.get_view() == "apps")
        {
            self.show_icon_popup("snackbar");
        }
        let weak = self.window.clone();
        slint::Timer::single_shot(duration, move || {
            with_controller(|c| {
                let current = c.state.borrow().snackbar_generation == generation;
                if current && let Some(ui) = weak.upgrade() {
                    ui.set_snackbar(SharedString::new());
                    ui.set_snackbar_undo(false);
                    c.each_mini(|g| {
                        g.set_snackbar(SharedString::new());
                        g.set_snackbar_undo(false);
                    });
                    if c.modes.popup.get_view() == "snackbar" {
                        c.hide_icon_popup();
                    }
                    c.state.borrow_mut().undo = None;
                }
            });
        });
        generation
    }

    fn app(&self, id: &str) -> Option<AppDef> {
        self.state.borrow().apps.iter().find_map(|a| match a {
            LoadedApp::Ok(app) if app.id == id => Some(app.clone()),
            _ => None,
        })
    }

    /// Reloads apps and groups from disk and refreshes the navigation.
    fn reload_apps(&self) {
        let apps = self.store.load_apps();
        let groups = self.store.load_groups();
        let mut history = HashMap::new();
        for app in &apps {
            if let LoadedApp::Ok(app) = app {
                history.insert(
                    app.id.clone(),
                    self.store.history(&app.id).read().unwrap_or_default(),
                );
            }
        }
        {
            let mut st = self.state.borrow_mut();
            st.apps = apps;
            st.groups = groups;
            st.history = history;
        }
        self.refresh_nav();
    }

    fn start(&self, request: RunRequest) {
        match self.manager.start(request.clone()) {
            Ok(StartOutcome::Started(run)) => {
                self.display(Some(run.id()));
            }
            Ok(StartOutcome::NeedsDecision { active }) => {
                let names: Vec<String> = active.iter().map(|r| r.action_name.clone()).collect();
                let app_name = active
                    .first()
                    .map(|r| r.app_name.clone())
                    .unwrap_or_default();
                self.state.borrow_mut().pending_ask = Some(request);
                self.show_full_for_dialog();
                let ui = self.ui();
                ui.set_ask_text(
                    format!(
                        "{app_name} is still running ({}). Stop it and run, or run in parallel?",
                        names.join(", ")
                    )
                    .into(),
                );
                ui.set_ask_visible(true);
            }
            Ok(StartOutcome::Rejected { active }) => {
                let app_name = active
                    .first()
                    .map(|r| r.app_name.clone())
                    .unwrap_or_default();
                self.snackbar(format!("{app_name} is still running."));
            }
            Err(e) => self.snackbar(format!("{e:#}")),
        }
        self.refresh_nav();
    }

    fn stop_app(&self, app_id: &str) {
        for run in self.manager.active_for_app(app_id) {
            run.stop();
        }
    }

    fn answer_ask(&self, answer: &str) {
        let ui = self.ui();
        ui.set_ask_visible(false);
        let pending = self.state.borrow_mut().pending_ask.take();
        self.end_full_for_dialog();
        let Some(mut request) = pending else {
            return;
        };
        request.decision = match answer {
            "restart" => Some(crate::data::WhenRunning::Restart),
            "parallel" => Some(crate::data::WhenRunning::Parallel),
            _ => return,
        };
        self.start(request);
    }

    fn on_event(&self, event: RunEvent) {
        match event {
            RunEvent::Started(info) => {
                if self.state.borrow().displayed.is_none() {
                    self.display(Some(info.run_id.clone()));
                }
                self.refresh_nav();
                self.refresh_header();
                self.refresh_history();
            }
            RunEvent::Output { run_id, .. } => {
                if self.state.borrow().displayed.as_deref() == Some(run_id.as_str()) {
                    self.append_pending_lines(&run_id, false);
                }
                if self.state.borrow().popup.run_id.as_deref() == Some(run_id.as_str()) {
                    self.append_pending_lines(&run_id, true);
                }
            }
            RunEvent::PhaseChanged(_) => {
                self.refresh_nav();
                self.refresh_header();
                self.refresh_popup_header();
                self.refresh_history();
            }
            RunEvent::Finished(info) => {
                if info.log_path.is_some() {
                    let records = self.store.history(&info.app_id).read().unwrap_or_default();
                    self.state
                        .borrow_mut()
                        .history
                        .insert(info.app_id.clone(), records);
                }
                self.refresh_nav();
                self.refresh_header();
                self.refresh_history();
                self.refresh_popup_header();
            }
        }
    }

    fn append_lines(&self, lines: &[OutputLine]) {
        let mut st = self.state.borrow_mut();
        let limit = self.settings.read().unwrap().output_line_limit.max(100) as usize;
        let mut columns = st.columns;
        let rows: Vec<OutputRow> = lines
            .iter()
            .map(|line| {
                let index = st.selection.next();
                let row = convert::output_row(&mut st.parser, line, index);
                columns = columns.max(row.plain_text.chars().count());
                st.selection.lines.push_back(row.plain_text.to_string());
                row
            })
            .collect();
        st.columns = columns;
        st.selection.trim(limit);
        let model = st.output.clone();
        drop(st);
        for row in rows {
            model.push(row);
        }
        let excess = model.row_count().saturating_sub(limit);
        for _ in 0..excess {
            model.remove(0);
        }
        self.ui().set_output_columns(columns as i32);
        self.refresh_selection(false);
    }

    /// Shows the output of a run (live when active) in the output panel.
    fn display(&self, run_id: Option<String>) {
        {
            let mut st = self.state.borrow_mut();
            st.displayed = run_id.clone();
            st.output.set_vec(Vec::new());
            st.parser = AnsiParser::new();
            st.selection.reset(0);
            st.columns = 0;
            st.displayed_log = None;
        }
        self.ui().set_output_columns(0);
        if let Some(id) = &run_id {
            if let Some(run) = self.manager.find(id) {
                let (first, lines) = run.lines();
                self.state.borrow_mut().selection.reset(first);
                self.append_lines(&lines);
            } else if let Some((record, path)) = self.find_record(id) {
                let _ = record;
                match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        let lines: Vec<OutputLine> = text
                            .lines()
                            .map(|l| OutputLine {
                                text: l.to_string(),
                                stderr: false,
                            })
                            .collect();
                        let limit = self.settings.read().unwrap().output_line_limit as usize;
                        let skip = lines.len().saturating_sub(limit);
                        self.state.borrow_mut().selection.reset(skip as u64);
                        self.append_lines(&lines[skip..]);
                    }
                    Err(e) => self.snackbar(format!("cannot read {}: {e}", path.display())),
                }
                self.state.borrow_mut().displayed_log = Some(path);
            }
        }
        self.refresh_header();
        self.refresh_selection(false);
        self.ui()
            .set_output_generation(self.ui().get_output_generation().wrapping_add(1));
        self.ui().invoke_scroll_to_end();
    }

    fn find_record(&self, run_id: &str) -> Option<(RunRecord, PathBuf)> {
        let st = self.state.borrow();
        for (app_id, records) in &st.history {
            if let Some(r) = records.iter().find(|r| r.run_id == run_id) {
                let path = self.store.history(app_id).log_path(r);
                return Some((r.clone(), path));
            }
        }
        None
    }

    fn displayed_run(&self) -> Option<Run> {
        let id = self.state.borrow().displayed.clone()?;
        self.manager.find(&id)
    }

    /// Sets `Clock.now` in every window, and keeps it ticking only while runs are active so
    /// an idle RunBay neither polls nor redraws.
    fn sync_clock(&self) {
        let set_clock = {
            let full = self.window.clone();
            let compact = self.modes.compact.as_weak();
            let popup = self.modes.popup.as_weak();
            move || {
                let now = convert::clock_now();
                if let Some(w) = full.upgrade() {
                    w.global::<crate::Clock>().set_now(now);
                }
                if let Some(w) = compact.upgrade() {
                    w.global::<crate::Clock>().set_now(now);
                }
                if let Some(w) = popup.upgrade() {
                    w.global::<crate::Clock>().set_now(now);
                }
            }
        };
        set_clock();
        let mut timer = self.clock_timer.borrow_mut();
        if self.manager.active().is_empty() {
            *timer = None;
        } else if timer.is_none() {
            let t = slint::Timer::default();
            t.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_secs(1),
                set_clock,
            );
            *timer = Some(t);
        }
    }

    /// Updates the output header and the tab strip.
    fn refresh_header(&self) {
        let ui = self.ui();
        let displayed = self.state.borrow().displayed.clone();
        let (title, status, agent, has_log, duration, started) = match self.displayed_run() {
            Some(run) => {
                let info = run.info();
                (
                    format!("{} · {}", info.app_name, info.action_name),
                    info.status.as_str().to_string(),
                    info.agent.clone().unwrap_or_default(),
                    info.log_path.as_ref().is_some_and(|p| p.exists()),
                    convert::final_duration(&info.start, info.end.as_ref()),
                    convert::clock_seconds(&info.start),
                )
            }
            None => match displayed.as_deref().and_then(|id| self.find_record(id)) {
                Some((record, path)) => {
                    let app_name = self
                        .app_name_of_record(&record)
                        .unwrap_or_else(|| "?".into());
                    (
                        format!("{app_name} · {}", record.action_id),
                        record.status.as_str().to_string(),
                        record.agent.clone().unwrap_or_default(),
                        path.exists(),
                        convert::final_duration(&record.start, record.end.as_ref()),
                        convert::clock_seconds(&record.start),
                    )
                }
                None => Default::default(),
            },
        };
        ui.set_output_title(title.into());
        ui.set_output_duration(duration.into());
        ui.set_output_started(started);
        ui.set_output_status(status.into());
        ui.set_output_busy(self.displayed_run().is_some_and(|run| run.info().is_busy()));
        ui.set_output_agent(agent.into());
        ui.set_output_has_log(has_log);
        ui.set_output_run_id(displayed.clone().unwrap_or_default().into());

        let mut tabs: Vec<RunRow> = self
            .manager
            .active()
            .iter()
            .map(|r| convert::run_row(&r.info(), displayed.as_deref(), true))
            .collect();
        if let Some(id) = &displayed
            && !tabs.iter().any(|t| t.run_id.as_str() == id)
            && let Some(run) = self.manager.find(id)
        {
            tabs.push(convert::run_row(&run.info(), displayed.as_deref(), true));
        }
        ui.set_tabs(ModelRc::new(VecModel::from(tabs)));
    }

    fn app_name_of_record(&self, record: &RunRecord) -> Option<String> {
        let st = self.state.borrow();
        st.history.iter().find_map(|(app_id, records)| {
            records
                .iter()
                .any(|r| r.run_id == record.run_id)
                .then(|| {
                    st.apps.iter().find_map(|a| match a {
                        LoadedApp::Ok(app) if &app.id == app_id => Some(app.name.clone()),
                        _ => None,
                    })
                })
                .flatten()
        })
    }

    /// Rebuilds the navigation model.
    fn refresh_nav(&self) {
        let st = self.state.borrow();
        let displayed = st.displayed.clone();
        let group_name = |id: &str| -> String {
            st.groups
                .groups
                .iter()
                .find(|g| g.id == id)
                .map(|g| g.name.clone())
                .unwrap_or_else(|| id.to_string())
        };
        let group_rank = |id: &str| -> usize {
            if id.is_empty() {
                return 0;
            }
            st.groups
                .groups
                .iter()
                .position(|g| g.id == id)
                .map_or(usize::MAX, |p| p + 1)
        };
        let mut apps: Vec<&LoadedApp> = st.apps.iter().collect();
        apps.sort_by_key(|a| match a {
            LoadedApp::Ok(app) => (group_rank(&app.group), app.order),
            LoadedApp::Errored { .. } => (usize::MAX, i32::MAX),
        });
        let mut rows = Vec::new();
        let mut last_group: Option<String> = None;
        for loaded in apps {
            let row = match loaded {
                LoadedApp::Ok(app) => {
                    let group = app.group.clone();
                    let header = if last_group.as_deref() != Some(group.as_str()) {
                        last_group = Some(group.clone());
                        if group.is_empty() && rows.is_empty() {
                            String::new()
                        } else if group.is_empty() {
                            "Other".to_string()
                        } else {
                            group_name(&group)
                        }
                    } else {
                        String::new()
                    };
                    let history = st.history.get(&app.id).map(Vec::as_slice).unwrap_or(&[]);
                    self.app_row(app, header, history, displayed.as_deref())
                }
                LoadedApp::Errored { id, error } => AppRow {
                    id: id.into(),
                    name: id.into(),
                    initial: convert::initial(id).into(),
                    error: format!("This app cannot be loaded: {error}").into(),
                    ..AppRow::default()
                },
            };
            rows.push(row);
        }
        drop(st);
        let aggregate = ["running", "failed", "success"]
            .into_iter()
            .find(|status| rows.iter().any(|r| r.status.as_str() == *status))
            .unwrap_or_default();
        self.state.borrow_mut().aggregate = aggregate.to_string();
        let busy = rows.iter().any(|row| row.status_busy);
        self.state.borrow_mut().aggregate_busy = busy;
        let model = ModelRc::new(VecModel::from(rows));
        self.ui().set_apps(model.clone());
        self.each_mini(|g| g.set_apps(model.clone()));
        self.sync_clock();
        let agent_running = self
            .manager
            .active()
            .iter()
            .any(|r| r.info().agent.is_some());
        self.modes.icon.set_status(aggregate.into());
        self.modes.icon.set_status_busy(busy);
        self.modes.icon.set_status_agent(agent_running);
        self.refresh_tray();
    }

    fn app_row(
        &self,
        app: &AppDef,
        group_header: String,
        history: &[RunRecord],
        displayed: Option<&str>,
    ) -> AppRow {
        let active = self.manager.active_for_app(&app.id);
        let active_infos: Vec<_> = active.iter().map(Run::info).collect();
        let button = |action: Option<&crate::data::ActionDef>| -> ActionButton {
            let Some(action) = action else {
                return ActionButton::default();
            };
            let running = active_infos.iter().find(|r| r.action_id == action.id);
            let enabled = match action.kind {
                ActionKind::BuildRun => [ActionKind::Build, ActionKind::Run]
                    .iter()
                    .all(|k| app.action_of(*k).is_some_and(|a| a.script.is_some())),
                _ => action.script.is_some(),
            };
            let image = self
                .store
                .action_icon_path(app, &action.id)
                .and_then(|path| slint::Image::load_from_path(&path).ok());
            ActionButton {
                id: action.id.clone().into(),
                name: action.name.clone().into(),
                kind: format!("{:?}", action.kind).to_lowercase().into(),
                icon_name: action.icon_name().into(),
                has_image: image.is_some(),
                image: image.unwrap_or_default(),
                enabled,
                status: if running.is_some() { "running" } else { "" }.into(),
                busy: active_infos
                    .iter()
                    .any(|r| r.action_id == action.id && r.is_busy()),
                agent: running.is_some_and(|r| r.agent.is_some()),
            }
        };
        let actions: Vec<ActionButton> = app.actions.iter().map(|a| button(Some(a))).collect();

        // Runs below the app: every active run, then the newest finished ones, up to 3.
        let mut runs: Vec<RunRow> = active_infos
            .iter()
            .map(|i| convert::run_row(i, displayed, false))
            .collect();
        let mut seen: Vec<String> = active_infos.iter().map(|i| i.run_id.clone()).collect();
        if let Some(last) = self.manager.last_for_app(&app.id) {
            let info = last.info();
            if !seen.contains(&info.run_id) {
                seen.push(info.run_id.clone());
                runs.push(convert::run_row(&info, displayed, false));
            }
        }
        for record in history.iter().rev() {
            if runs.len() >= 3.max(active_infos.len()) {
                break;
            }
            if seen.contains(&record.run_id) {
                continue;
            }
            let action_name = app
                .action(&record.action_id)
                .map_or(record.action_id.clone(), |a| a.name.clone());
            runs.push(convert::record_row(record, &action_name, displayed));
        }
        runs.truncate(3.max(active_infos.len()));

        let badge_info = active_infos
            .iter()
            .find(|info| info.is_busy())
            .or_else(|| active_infos.last());
        let (status, status_agent, agent) = match badge_info {
            Some(info) => (
                "running".to_string(),
                info.agent.is_some(),
                info.agent.clone(),
            ),
            None => match runs.first() {
                Some(r) => (
                    r.status.to_string(),
                    !r.agent.is_empty(),
                    Some(r.agent.to_string()),
                ),
                None => (String::new(), false, None),
            },
        };

        let (icon, has_icon) = match &app.icon {
            Some(file) => {
                match slint::Image::load_from_path(&self.store.app_dir(&app.id).join(file)) {
                    Ok(image) => (image, true),
                    Err(_) => (slint::Image::default(), false),
                }
            }
            None => (slint::Image::default(), false),
        };

        let status_detail = if let Some(info) = badge_info {
            format!(
                "{}: {} ({} active)",
                info.action_name,
                if info.is_busy() {
                    "Working"
                } else {
                    "Application running"
                },
                active_infos.len()
            )
        } else if let Some(run) = runs.first() {
            let result = match run.status.as_str() {
                "success" => "Succeeded",
                "failed" => "Failed",
                "stopped" => "Stopped",
                _ => run.status.as_str(),
            };
            format!("Last run: {} · {result} · {}", run.title, run.time)
        } else {
            String::new()
        };

        AppRow {
            id: app.id.clone().into(),
            name: app.name.clone().into(),
            initial: convert::initial(&app.name).into(),
            icon,
            has_icon,
            group_header: group_header.into(),
            group_id: app.group.clone().into(),
            status: status.into(),
            status_agent,
            status_busy: active_infos.iter().any(|info| info.is_busy()),
            status_detail: status_detail.into(),
            agent: agent
                .filter(|a| !a.is_empty() && status_agent)
                .unwrap_or_default()
                .into(),
            error: SharedString::new(),
            actions: ModelRc::new(VecModel::from(actions)),
            runs: ModelRc::new(VecModel::from(runs)),
        }
    }

    fn copy_output(&self) {
        let text = match self.displayed_run() {
            Some(run) => run
                .lines()
                .1
                .iter()
                .map(|l| crate::ansi::strip(&l.text))
                .collect::<Vec<_>>()
                .join("\r\n"),
            None => {
                let path = self.state.borrow().displayed_log.clone();
                path.and_then(|p| std::fs::read_to_string(p).ok())
                    .unwrap_or_default()
            }
        };
        self.report(platform::copy_text(&text));
    }

    fn open_log(&self) {
        let path = match self.displayed_run() {
            Some(run) => run.info().log_path,
            None => self.state.borrow().displayed_log.clone(),
        };
        if let Some(path) = path {
            self.report(platform::open_path(&path));
        }
    }

    fn ask_remove(&self, id: &str) {
        let name = self.app(id).map_or(id.to_string(), |a| a.name);
        self.state.borrow_mut().confirm_app = Some(id.to_string());
        let ui = self.ui();
        ui.set_confirm_text(
            format!(
                "\"{name}\" and its whole run history will be deleted. Active runs are stopped."
            )
            .into(),
        );
        ui.set_confirm_visible(true);
    }

    fn remove_confirmed(&self) {
        let ui = self.ui();
        ui.set_confirm_visible(false);
        let Some(id) = self.state.borrow_mut().confirm_app.take() else {
            return;
        };
        for run in self.manager.active_for_app(&id) {
            run.stop();
            run.wait(Duration::from_secs(10));
        }
        self.report(self.store.delete_app(&id));
        self.reload_apps();
    }
}
