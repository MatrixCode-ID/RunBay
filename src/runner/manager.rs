//! The run manager: starts runs on their own threads, tracks active and recent runs, and
//! reports events to listeners.

use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, Local, SubsecRound};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use super::job::Job;
use super::shell;
use crate::ansi;
use crate::data::{
    ActionDef, ActionKind, AppDef, History, LoadedApp, Retention, RunRecord, RunStatus, Source,
    Store, WhenRunning, ids,
};

/// Interval at which output lines are delivered to listeners.
const BATCH_INTERVAL: Duration = Duration::from_millis(50);
/// Finished runs kept in memory per app (their output is the "last run").
const RECENT_PER_APP: usize = 5;
/// How long a restart waits for the app's active runs to end.
const RESTART_WAIT: Duration = Duration::from_secs(10);

/// One line of output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLine {
    /// The text, possibly with ANSI escape sequences.
    pub text: String,
    /// The line came from stderr (or is a RunBay error message).
    pub stderr: bool,
}

impl OutputLine {
    fn out(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            stderr: false,
        }
    }

    fn err(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            stderr: true,
        }
    }
}

/// A snapshot of a run's state.
#[derive(Debug, Clone, PartialEq)]
pub struct RunInfo {
    /// App id.
    pub app_id: String,
    /// App display name at start.
    pub app_name: String,
    /// Action id.
    pub action_id: String,
    /// Action display name at start.
    pub action_name: String,
    /// Action kind.
    pub kind: ActionKind,
    /// Run id.
    pub run_id: String,
    /// Status.
    pub status: RunStatus,
    /// Who started it.
    pub source: Source,
    /// Agent name for MCP runs.
    pub agent: Option<String>,
    /// Start time.
    pub start: DateTime<FixedOffset>,
    /// End time once finished.
    pub end: Option<DateTime<FixedOffset>>,
    /// Exit code once finished (absent when the process did not start).
    pub exit_code: Option<i32>,
    /// Absolute log path, absent when retention is off.
    pub log_path: Option<PathBuf>,
}

impl RunInfo {
    fn record(&self, history: &History) -> RunRecord {
        RunRecord {
            run_id: self.run_id.clone(),
            action_id: self.action_id.clone(),
            start: self.start,
            end: self.end,
            exit_code: self.exit_code,
            status: self.status,
            source: self.source,
            agent: self.agent.clone(),
            log: self
                .log_path
                .as_ref()
                .and_then(|p| p.strip_prefix(history.dir()).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| History::relative_log(&self.run_id, &self.start)),
        }
    }

    /// Duration until now or until the end.
    pub fn duration(&self) -> chrono::Duration {
        let end = self.end.unwrap_or_else(|| Local::now().fixed_offset());
        end - self.start
    }
}

/// Something that happened to a run.
#[derive(Debug, Clone)]
pub enum RunEvent {
    /// A run started.
    Started(RunInfo),
    /// New output lines (batched).
    Output {
        /// App id.
        app_id: String,
        /// Run id.
        run_id: String,
        /// The new lines, in arrival order.
        lines: Vec<OutputLine>,
    },
    /// A run finished; the info carries its final status.
    Finished(RunInfo),
}

struct OutputBuffer {
    lines: VecDeque<OutputLine>,
    /// Index (from the start of the run) of `lines[0]`.
    first: u64,
    limit: usize,
}

impl OutputBuffer {
    fn push(&mut self, line: OutputLine) {
        self.lines.push_back(line);
        while self.lines.len() > self.limit {
            self.lines.pop_front();
            self.first += 1;
        }
    }

    fn total(&self) -> u64 {
        self.first + self.lines.len() as u64
    }
}

struct RunShared {
    info: Mutex<RunInfo>,
    output: Mutex<OutputBuffer>,
    job: Mutex<Option<Arc<Job>>>,
    stop: AtomicBool,
    done: Mutex<bool>,
    done_cv: Condvar,
}

/// A handle to an active or recently finished run.
#[derive(Clone)]
pub struct Run(Arc<RunShared>);

impl Run {
    /// Current state.
    pub fn info(&self) -> RunInfo {
        self.0.info.lock().unwrap().clone()
    }

    /// Run id.
    pub fn id(&self) -> String {
        self.0.info.lock().unwrap().run_id.clone()
    }

    /// Number of lines produced so far (including lines dropped from memory).
    pub fn total_lines(&self) -> u64 {
        self.0.output.lock().unwrap().total()
    }

    /// Lines still held in memory, with the index of the first one.
    pub fn lines(&self) -> (u64, Vec<OutputLine>) {
        let out = self.0.output.lock().unwrap();
        (out.first, out.lines.iter().cloned().collect())
    }

    /// The last `n` lines held in memory.
    pub fn tail(&self, n: usize) -> Vec<OutputLine> {
        let out = self.0.output.lock().unwrap();
        let skip = out.lines.len().saturating_sub(n);
        out.lines.iter().skip(skip).cloned().collect()
    }

    /// Lines `offset .. offset + limit`, as far as they are still in memory.
    pub fn range(&self, offset: u64, limit: usize) -> Vec<OutputLine> {
        let out = self.0.output.lock().unwrap();
        let start = offset.saturating_sub(out.first) as usize;
        out.lines.iter().skip(start).take(limit).cloned().collect()
    }

    /// True once the run has finished.
    pub fn is_done(&self) -> bool {
        *self.0.done.lock().unwrap()
    }

    /// Waits until the run finishes or `timeout` passes; returns true when finished.
    pub fn wait(&self, timeout: Duration) -> bool {
        let done = self.0.done.lock().unwrap();
        let (done, _) = self
            .0
            .done_cv
            .wait_timeout_while(done, timeout, |d| !*d)
            .unwrap();
        *done
    }

    /// Requests a stop: kills the whole process tree and prevents later steps.
    pub fn stop(&self) {
        self.0.stop.store(true, Ordering::SeqCst);
        if let Some(job) = self.0.job.lock().unwrap().clone() {
            job.terminate();
        }
    }

    fn stopping(&self) -> bool {
        self.0.stop.load(Ordering::SeqCst)
    }
}

/// A request to start an action.
#[derive(Debug, Clone)]
pub struct RunRequest {
    /// App id.
    pub app_id: String,
    /// Action id.
    pub action_id: String,
    /// Who starts it.
    pub source: Source,
    /// Agent name for MCP requests.
    pub agent: Option<String>,
    /// Answer to an `ask` rule: `Restart` or `Parallel`. Ignored for other rules.
    pub decision: Option<WhenRunning>,
}

impl RunRequest {
    /// A request from a click in the GUI.
    pub fn ui(app_id: &str, action_id: &str) -> Self {
        Self {
            app_id: app_id.into(),
            action_id: action_id.into(),
            source: Source::Ui,
            agent: None,
            decision: None,
        }
    }
}

/// Result of [`RunManager::start`].
#[derive(Clone)]
pub enum StartOutcome {
    /// The run started.
    Started(Run),
    /// The action's rule is `ask` and no decision was given.
    NeedsDecision {
        /// The app's active runs.
        active: Vec<RunInfo>,
    },
    /// The action's rule is `reject` and the app is running.
    Rejected {
        /// The app's active runs.
        active: Vec<RunInfo>,
    },
}

type Listener = Arc<dyn Fn(&RunEvent) + Send + Sync>;

struct Inner {
    store: Store,
    line_limit: AtomicU32,
    active: Mutex<Vec<Run>>,
    recent: Mutex<VecDeque<Run>>,
    listeners: Mutex<Vec<Listener>>,
    history_lock: Mutex<()>,
}

/// Starts and tracks runs. Cheap to clone.
#[derive(Clone)]
pub struct RunManager {
    inner: Arc<Inner>,
}

impl RunManager {
    /// Creates a manager over `store` with an on-screen line limit per run.
    pub fn new(store: Store, line_limit: u32) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                line_limit: AtomicU32::new(line_limit.max(100)),
                active: Mutex::new(Vec::new()),
                recent: Mutex::new(VecDeque::new()),
                listeners: Mutex::new(Vec::new()),
                history_lock: Mutex::new(()),
            }),
        }
    }

    /// The data store.
    pub fn store(&self) -> &Store {
        &self.inner.store
    }

    /// Changes the line limit for runs started from now on.
    pub fn set_line_limit(&self, limit: u32) {
        self.inner
            .line_limit
            .store(limit.max(100), Ordering::Relaxed);
    }

    /// Registers a listener called (on runner threads) for every event.
    pub fn subscribe(&self, listener: impl Fn(&RunEvent) + Send + Sync + 'static) {
        self.inner
            .listeners
            .lock()
            .unwrap()
            .push(Arc::new(listener));
    }

    /// Repairs stale `running` history entries of every app and applies retention.
    pub fn startup_cleanup(&self) {
        for app in self.inner.store.load_apps() {
            if let LoadedApp::Ok(app) = app {
                let history = self.inner.store.history(&app.id);
                let _guard = self.inner.history_lock.lock().unwrap();
                if let Err(e) = history.fix_stale(&|id| self.find(id).is_some_and(|r| !r.is_done()))
                {
                    log::warn!("cannot repair the history of {}: {e:#}", app.id);
                }
                if let Err(e) = history.apply_retention(&app) {
                    log::warn!("cannot apply retention for {}: {e:#}", app.id);
                }
            }
        }
    }

    /// Active runs of every app, oldest first.
    pub fn active(&self) -> Vec<Run> {
        self.inner.active.lock().unwrap().clone()
    }

    /// Active runs of one app.
    pub fn active_for_app(&self, app_id: &str) -> Vec<Run> {
        self.active()
            .into_iter()
            .filter(|r| r.0.info.lock().unwrap().app_id == app_id)
            .collect()
    }

    /// Finds an active or recent run.
    pub fn find(&self, run_id: &str) -> Option<Run> {
        let matches = |r: &&Run| r.0.info.lock().unwrap().run_id == run_id;
        if let Some(r) = self.inner.active.lock().unwrap().iter().find(matches) {
            return Some(r.clone());
        }
        self.inner
            .recent
            .lock()
            .unwrap()
            .iter()
            .find(matches)
            .cloned()
    }

    /// The most recent run of an app (active first), if any is in memory.
    pub fn last_for_app(&self, app_id: &str) -> Option<Run> {
        if let Some(r) = self.active_for_app(app_id).pop() {
            return Some(r);
        }
        self.inner
            .recent
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|r| r.0.info.lock().unwrap().app_id == app_id)
            .cloned()
    }

    /// Stops a run; returns false when it is not active.
    pub fn stop(&self, run_id: &str) -> bool {
        match self.find(run_id) {
            Some(r) if !r.is_done() => {
                r.stop();
                true
            }
            _ => false,
        }
    }

    /// Stops every active run and waits up to `timeout` for them to end.
    pub fn stop_all(&self, timeout: Duration) {
        let runs = self.active();
        for r in &runs {
            r.stop();
        }
        let deadline = Instant::now() + timeout;
        for r in &runs {
            r.wait(deadline.saturating_duration_since(Instant::now()));
        }
    }

    /// Starts an action, applying its `whenRunning` rule.
    pub fn start(&self, request: RunRequest) -> Result<StartOutcome> {
        let app = match self.inner.store.load_app(&request.app_id) {
            LoadedApp::Ok(app) => app,
            LoadedApp::Errored { error, .. } => {
                bail!("app \"{}\" cannot be loaded: {error}", request.app_id)
            }
        };
        let action = app
            .action(&request.action_id)
            .with_context(|| format!("app \"{}\" has no action \"{}\"", app.id, request.action_id))?
            .clone();
        let steps = self.steps(&app, &action)?;

        let active = self.active_for_app(&app.id);
        if !active.is_empty() {
            let infos = || active.iter().map(Run::info).collect::<Vec<_>>();
            let rule = match action.when_running {
                WhenRunning::Ask => match request.decision {
                    Some(d @ (WhenRunning::Restart | WhenRunning::Parallel)) => d,
                    _ => return Ok(StartOutcome::NeedsDecision { active: infos() }),
                },
                rule => rule,
            };
            match rule {
                WhenRunning::Reject => return Ok(StartOutcome::Rejected { active: infos() }),
                WhenRunning::Restart => {
                    for r in &active {
                        r.stop();
                    }
                    for r in &active {
                        if !r.wait(RESTART_WAIT) {
                            bail!("the active runs of \"{}\" did not stop in time", app.name);
                        }
                    }
                }
                _ => {}
            }
        }

        let run = self.spawn(app, action, steps, &request)?;
        Ok(StartOutcome::Started(run))
    }

    /// The scripts an action runs, in order.
    fn steps(&self, app: &AppDef, action: &ActionDef) -> Result<Vec<(String, PathBuf)>> {
        let script_of = |a: &ActionDef| -> Result<PathBuf> {
            self.inner
                .store
                .script_path(app, &a.id)
                .with_context(|| format!("{} › {} has no script", app.name, a.name))
        };
        Ok(match action.kind {
            ActionKind::BuildRun => {
                let build = app
                    .action_of(ActionKind::Build)
                    .context("no build action")?;
                let run = app.action_of(ActionKind::Run).context("no run action")?;
                vec![
                    (build.name.clone(), script_of(build)?),
                    (run.name.clone(), script_of(run)?),
                ]
            }
            _ => vec![(action.name.clone(), script_of(action)?)],
        })
    }

    fn spawn(
        &self,
        app: AppDef,
        action: ActionDef,
        steps: Vec<(String, PathBuf)>,
        request: &RunRequest,
    ) -> Result<Run> {
        let now = Local::now();
        let run_id = ids::run_id(now, &action.id);
        let start = now.fixed_offset().trunc_subsecs(0);
        let history = self.inner.store.history(&app.id);
        let log_path = (action.retention != Retention::Off)
            .then(|| history.dir().join(History::relative_log(&run_id, &start)));
        let info = RunInfo {
            app_id: app.id.clone(),
            app_name: app.name.clone(),
            action_id: action.id.clone(),
            action_name: action.name.clone(),
            kind: action.kind,
            run_id,
            status: RunStatus::Running,
            source: request.source,
            agent: request.agent.clone(),
            start,
            end: None,
            exit_code: None,
            log_path,
        };
        let run = Run(Arc::new(RunShared {
            info: Mutex::new(info.clone()),
            output: Mutex::new(OutputBuffer {
                lines: VecDeque::new(),
                first: 0,
                limit: self.inner.line_limit.load(Ordering::Relaxed) as usize,
            }),
            job: Mutex::new(None),
            stop: AtomicBool::new(false),
            done: Mutex::new(false),
            done_cv: Condvar::new(),
        }));

        if info.log_path.is_some() {
            let _guard = self.inner.history_lock.lock().unwrap();
            if let Err(e) = history.append(&info.record(&history)) {
                log::warn!("cannot record the run in the history: {e:#}");
            }
        }
        self.inner.active.lock().unwrap().push(run.clone());
        self.emit(&RunEvent::Started(info));

        let manager = self.clone();
        let thread_run = run.clone();
        std::thread::Builder::new()
            .name("runbay-run".into())
            .spawn(move || manager.execute(thread_run, app, action, steps))
            .context("cannot start the run thread")?;
        Ok(run)
    }

    fn emit(&self, event: &RunEvent) {
        let listeners = self.inner.listeners.lock().unwrap().clone();
        for listener in listeners {
            listener(event);
        }
    }

    /// Body of the run thread.
    fn execute(&self, run: Run, app: AppDef, action: ActionDef, steps: Vec<(String, PathBuf)>) {
        let info = run.info();
        let mut sink = Sink::new(self, &run, info.log_path.as_deref());
        let result = self.execute_steps(&run, &app, &action, &steps, &mut sink);
        let (status, exit_code) = match result {
            Ok(outcome) => outcome,
            Err(e) => {
                sink.line(OutputLine::err(format!("RunBay: {e:#}")));
                (RunStatus::Failed, None)
            }
        };
        let status = if run.stopping() && status != RunStatus::Success {
            RunStatus::Stopped
        } else {
            status
        };
        sink.flush();
        sink.close();

        let final_info = {
            let mut info = run.0.info.lock().unwrap();
            info.status = status;
            info.exit_code = exit_code;
            info.end = Some(Local::now().fixed_offset().trunc_subsecs(0));
            info.clone()
        };
        if final_info.log_path.is_some() {
            let history = self.inner.store.history(&app.id);
            let _guard = self.inner.history_lock.lock().unwrap();
            // Retention follows the app as it is now; a deleted app has no history left.
            if let LoadedApp::Ok(current) = self.inner.store.load_app(&app.id)
                && let Err(e) = history.finish(&final_info.record(&history), &current)
            {
                log::warn!("cannot update the history: {e:#}");
            }
        }

        {
            let mut active = self.inner.active.lock().unwrap();
            active.retain(|r| !Arc::ptr_eq(&r.0, &run.0));
            let mut recent = self.inner.recent.lock().unwrap();
            recent.push_back(run.clone());
            let same_app = recent
                .iter()
                .filter(|r| r.0.info.lock().unwrap().app_id == app.id)
                .count();
            if same_app > RECENT_PER_APP
                && let Some(pos) = recent
                    .iter()
                    .position(|r| r.0.info.lock().unwrap().app_id == app.id)
            {
                recent.remove(pos);
            }
        }
        *run.0.done.lock().unwrap() = true;
        run.0.done_cv.notify_all();
        self.emit(&RunEvent::Finished(final_info));
    }

    fn execute_steps(
        &self,
        run: &Run,
        app: &AppDef,
        action: &ActionDef,
        steps: &[(String, PathBuf)],
        sink: &mut Sink,
    ) -> Result<(RunStatus, Option<i32>)> {
        let app_dir = self.inner.store.app_dir(&app.id);
        let working_dir = match action.working_directory.as_deref().map(str::trim) {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ if !app.working_directory.trim().is_empty() => {
                PathBuf::from(app.working_directory.trim())
            }
            _ => app_dir.clone(),
        };
        if !working_dir.is_dir() {
            bail!(
                "the working directory \"{}\" does not exist",
                working_dir.display()
            );
        }
        let shell_path = shell::resolve(app.shell)?;
        let mut env = app.env.clone();
        env.extend(action.env.clone());
        env.insert("RUNBAY_APP_ID".into(), app.id.clone());
        env.insert("RUNBAY_ACTION_ID".into(), action.id.clone());
        env.insert("RUNBAY_RUN_ID".into(), run.id());
        env.insert(
            "RUNBAY_APP_DIR".into(),
            app_dir.to_string_lossy().into_owned(),
        );

        let sections = steps.len() > 1;
        let mut last_code = None;
        for (index, (name, script)) in steps.iter().enumerate() {
            if run.stopping() {
                return Ok((RunStatus::Stopped, last_code));
            }
            if !script.is_file() {
                bail!("the script \"{}\" does not exist", script.display());
            }
            if sections {
                sink.line(OutputLine::out(format!("── {name} ──")));
            }
            let code = self.run_process(run, &shell_path, script, &working_dir, &env, sink)?;
            last_code = Some(code);
            if run.stopping() {
                return Ok((RunStatus::Stopped, last_code));
            }
            if code != 0 {
                if sections && index + 1 < steps.len() {
                    sink.line(OutputLine::err(format!(
                        "── {} skipped: {name} failed with exit code {code} ──",
                        steps[index + 1].0
                    )));
                }
                return Ok((RunStatus::Failed, last_code));
            }
        }
        Ok((RunStatus::Success, last_code))
    }

    fn run_process(
        &self,
        run: &Run,
        shell_path: &Path,
        script: &Path,
        working_dir: &Path,
        env: &std::collections::BTreeMap<String, String>,
        sink: &mut Sink,
    ) -> Result<i32> {
        let job = Arc::new(Job::new()?);
        let mut child = Command::new(shell_path)
            .args(shell::script_args(script))
            .current_dir(working_dir)
            .envs(env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()
            .with_context(|| format!("cannot start {}", shell_path.display()))?;
        if let Err(e) = job.assign(&child) {
            let _ = child.kill();
            return Err(e);
        }
        *run.0.job.lock().unwrap() = Some(job.clone());
        if run.stopping() {
            job.terminate();
        }

        enum Msg {
            Line(OutputLine),
            Eof,
            Exited(i32),
        }
        let (tx, rx) = mpsc::channel::<Msg>();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        for (stream, is_err) in [
            (Box::new(stdout) as Box<dyn Read + Send>, false),
            (Box::new(stderr), true),
        ] {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    match reader.read_until(b'\n', &mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            while matches!(buf.last(), Some(b'\n' | b'\r')) {
                                buf.pop();
                            }
                            let text = String::from_utf8_lossy(&buf).into_owned();
                            let line = OutputLine {
                                text,
                                stderr: is_err,
                            };
                            if tx.send(Msg::Line(line)).is_err() {
                                break;
                            }
                        }
                    }
                }
                let _ = tx.send(Msg::Eof);
            });
        }
        {
            let tx = tx.clone();
            let job = job.clone();
            std::thread::spawn(move || {
                let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
                // The run lasts while any process of its tree lives (e.g. the started app).
                job.wait_empty();
                let _ = tx.send(Msg::Exited(code));
            });
        }
        drop(tx);

        let mut eofs = 0;
        let mut exit: Option<(i32, Instant)> = None;
        loop {
            let timeout = match (sink.has_pending(), exit) {
                (true, _) => BATCH_INTERVAL.saturating_sub(sink.since_flush()),
                (false, Some((_, at))) => Duration::from_secs(2).saturating_sub(at.elapsed()),
                (false, None) => Duration::from_secs(3600),
            };
            match rx.recv_timeout(timeout) {
                Ok(Msg::Line(line)) => sink.line(line),
                Ok(Msg::Eof) => eofs += 1,
                Ok(Msg::Exited(code)) => exit = Some((code, Instant::now())),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if sink.since_flush() >= BATCH_INTERVAL {
                sink.flush();
            }
            if let Some((_, at)) = exit
                && (eofs == 2 || at.elapsed() >= Duration::from_secs(2))
            {
                break;
            }
        }
        sink.flush();
        *run.0.job.lock().unwrap() = None;
        Ok(exit.map_or(-1, |(code, _)| code))
    }
}

/// Collects output of a run: memory buffer, log file, and batched events.
struct Sink<'a> {
    manager: &'a RunManager,
    run: &'a Run,
    app_id: String,
    run_id: String,
    log: Option<BufWriter<fs::File>>,
    pending: Vec<OutputLine>,
    last_flush: Instant,
}

impl<'a> Sink<'a> {
    fn new(manager: &'a RunManager, run: &'a Run, log_path: Option<&Path>) -> Self {
        let info = run.info();
        let log = log_path.and_then(|path| {
            let open = || -> Result<fs::File> {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                Ok(fs::File::create(path)?)
            };
            match open() {
                Ok(f) => Some(BufWriter::new(f)),
                Err(e) => {
                    log::warn!("cannot create the log {}: {e:#}", path.display());
                    None
                }
            }
        });
        Self {
            manager,
            run,
            app_id: info.app_id,
            run_id: info.run_id,
            log,
            pending: Vec::new(),
            last_flush: Instant::now(),
        }
    }

    fn line(&mut self, line: OutputLine) {
        if let Some(log) = self.log.as_mut() {
            let _ = writeln!(log, "{}", ansi::strip(&line.text));
        }
        self.run.0.output.lock().unwrap().push(line.clone());
        self.pending.push(line);
    }

    fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn since_flush(&self) -> Duration {
        self.last_flush.elapsed()
    }

    fn flush(&mut self) {
        self.last_flush = Instant::now();
        if self.pending.is_empty() {
            return;
        }
        let lines = std::mem::take(&mut self.pending);
        self.manager.emit(&RunEvent::Output {
            app_id: self.app_id.clone(),
            run_id: self.run_id.clone(),
            lines,
        });
    }

    fn close(&mut self) {
        if let Some(mut log) = self.log.take() {
            let _ = log.flush();
        }
    }
}
