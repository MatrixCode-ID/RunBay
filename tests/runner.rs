//! Runner integration tests with real PowerShell scripts.

use std::path::Path;
use std::time::{Duration, Instant};

use runbay::data::{AppDef, Retention, RunStatus, Settings, Store, WhenRunning};
use runbay::runner::{Run, RunManager, RunRequest, StartOutcome};

struct Fixture {
    _dir: tempfile::TempDir,
    work: std::path::PathBuf,
    manager: RunManager,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let store = Store::open(dir.path().join("data")).unwrap();
        Self {
            manager: RunManager::new(store, 10_000),
            work,
            _dir: dir,
        }
    }

    fn store(&self) -> &Store {
        self.manager.store()
    }

    /// Creates an app with the given scripts (`(action id, script)`).
    fn app(&self, name: &str, scripts: &[(&str, &str)]) -> AppDef {
        let mut app = self.store().create_app(name, &Settings::default()).unwrap();
        app.working_directory = self.work.to_string_lossy().into_owned();
        for (action, script) in scripts {
            self.store().write_script(&mut app, action, script).unwrap();
        }
        self.store().save_app(&app).unwrap();
        app
    }

    fn start(&self, app: &AppDef, action: &str) -> Run {
        match self.manager.start(RunRequest::ui(&app.id, action)).unwrap() {
            StartOutcome::Started(run) => run,
            _ => panic!("the run did not start"),
        }
    }

    fn finish(&self, app: &AppDef, action: &str) -> Run {
        let run = self.start(app, action);
        assert!(run.wait(Duration::from_secs(60)), "the run did not finish");
        run
    }
}

fn texts(run: &Run) -> Vec<String> {
    run.lines().1.into_iter().map(|l| l.text).collect()
}

fn process_alive(pid: u32) -> bool {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
}

fn wait_for_file(path: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(path)
            && !text.trim().is_empty()
        {
            return text;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("{} was not written", path.display());
}

#[test]
fn success_records_output_history_and_log() {
    let f = Fixture::new();
    let app = f.app(
        "Ok App",
        &[(
            "build",
            "Write-Output 'line one'\n[Console]::Error.WriteLine('warn line')\nWrite-Output 'line two'",
        )],
    );
    let run = f.finish(&app, "build");
    let info = run.info();
    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.exit_code, Some(0));
    let lines = run.lines().1;
    assert!(lines.iter().any(|l| l.text == "line one" && !l.stderr));
    assert!(lines.iter().any(|l| l.text == "warn line" && l.stderr));
    let log = std::fs::read_to_string(info.log_path.as_ref().unwrap()).unwrap();
    assert!(log.contains("line two"));
    let records = f.store().history(&app.id).read().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, RunStatus::Success);
    assert_eq!(records[0].run_id, info.run_id);
}

#[test]
fn failure_keeps_exit_code() {
    let f = Fixture::new();
    let app = f.app("Fail App", &[("build", "Write-Output 'oops'\nexit 3")]);
    let run = f.finish(&app, "build");
    assert_eq!(run.info().status, RunStatus::Failed);
    assert_eq!(run.info().exit_code, Some(3));
}

#[test]
fn script_error_fails() {
    let f = Fixture::new();
    let app = f.app("Throw App", &[("build", "throw 'broken'")]);
    let run = f.finish(&app, "build");
    assert_eq!(run.info().status, RunStatus::Failed);
    assert!(
        run.lines()
            .1
            .iter()
            .any(|l| l.stderr && l.text.contains("broken"))
    );
}

#[test]
fn environment_working_directory_and_utf8() {
    let f = Fixture::new();
    let mut app = f.app(
        "Env App",
        &[(
            "build",
            "Write-Output \"FOO=$env:FOO\"\nWrite-Output \"BAR=$env:BAR\"\n\
             Write-Output \"APP=$env:RUNBAY_APP_ID ACTION=$env:RUNBAY_ACTION_ID\"\n\
             Write-Output \"RUN=$env:RUNBAY_RUN_ID\"\nWrite-Output \"DIR=$((Get-Location).Path)\"\n\
             Write-Output 'héllo ✓ 日本'",
        )],
    );
    app.env.insert("FOO".into(), "app".into());
    app.env.insert("BAR".into(), "app".into());
    let sub = f.work.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let action = app.action_mut("build").unwrap();
    action.env.insert("BAR".into(), "action".into());
    action.working_directory = Some(sub.to_string_lossy().into_owned());
    f.store().save_app(&app).unwrap();

    let run = f.finish(&app, "build");
    let lines = texts(&run);
    assert!(lines.contains(&"FOO=app".to_string()), "{lines:?}");
    assert!(lines.contains(&"BAR=action".to_string()), "{lines:?}");
    assert!(
        lines.contains(&format!("APP={} ACTION=build", app.id)),
        "{lines:?}"
    );
    assert!(
        lines.contains(&format!("RUN={}", run.info().run_id)),
        "{lines:?}"
    );
    assert!(
        lines.contains(&format!("DIR={}", sub.display())),
        "{lines:?}"
    );
    assert!(lines.contains(&"héllo ✓ 日本".to_string()), "{lines:?}");
}

#[test]
fn missing_working_directory_fails_clearly() {
    let f = Fixture::new();
    let mut app = f.app("Nowhere", &[("build", "Write-Output 'x'")]);
    app.working_directory = f.work.join("missing").to_string_lossy().into_owned();
    f.store().save_app(&app).unwrap();
    let run = f.finish(&app, "build");
    assert_eq!(run.info().status, RunStatus::Failed);
    assert!(
        texts(&run)
            .iter()
            .any(|l| l.contains("working directory") && l.contains("does not exist"))
    );
}

#[test]
fn action_without_script_does_not_start() {
    let f = Fixture::new();
    let app = f.app("Empty", &[]);
    assert!(f.manager.start(RunRequest::ui(&app.id, "build")).is_err());
    assert!(
        f.manager
            .start(RunRequest::ui(&app.id, "build-run"))
            .is_err()
    );
}

#[test]
fn stop_kills_the_whole_process_tree() {
    let f = Fixture::new();
    let pid_file = f.work.join("child.pid");
    let script = format!(
        "$p = Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList '-NoProfile','-Command','Start-Sleep 120' -PassThru -WindowStyle Hidden\n\
         Set-Content -Path '{}' -Value $p.Id\nStart-Sleep 120",
        pid_file.display()
    );
    let app = f.app("Tree", &[("run", &script)]);
    let run = f.start(&app, "run");
    let pid: u32 = wait_for_file(&pid_file).trim().parse().unwrap();
    assert!(process_alive(pid));
    assert!(f.manager.stop(&run.info().run_id));
    assert!(run.wait(Duration::from_secs(20)));
    assert_eq!(run.info().status, RunStatus::Stopped);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!process_alive(pid), "the child process survived Stop");
}

#[test]
fn run_lasts_while_started_process_lives() {
    let f = Fixture::new();
    let script = "Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList '-NoProfile','-Command','Start-Sleep 3' -WindowStyle Hidden\nWrite-Output 'launched'";
    let app = f.app("Launcher", &[("run", script)]);
    let started = Instant::now();
    let run = f.finish(&app, "run");
    assert_eq!(run.info().status, RunStatus::Success);
    assert!(started.elapsed() >= Duration::from_secs(3));
}

#[test]
fn build_run_skips_run_when_build_fails() {
    let f = Fixture::new();
    let marker = f.work.join("ran.txt");
    let run_script = format!("Set-Content -Path '{}' -Value ran", marker.display());
    let app = f.app(
        "Skip",
        &[
            ("build", "Write-Output 'building'\nexit 2"),
            ("run", &run_script),
        ],
    );
    let run = f.finish(&app, "build-run");
    let info = run.info();
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.exit_code, Some(2));
    assert!(!marker.exists(), "the run script ran after a failed build");
    let lines = texts(&run);
    assert!(lines.iter().any(|l| l.contains("── Build ──")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("skipped")), "{lines:?}");
    let records = f.store().history(&app.id).read().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action_id, "build-run");
}

#[test]
fn build_run_runs_both_steps() {
    let f = Fixture::new();
    let app = f.app(
        "Both",
        &[("build", "Write-Output 'b'"), ("run", "Write-Output 'r'")],
    );
    let run = f.finish(&app, "build-run");
    assert_eq!(run.info().status, RunStatus::Success);
    let lines = texts(&run);
    let b = lines.iter().position(|l| l == "b").unwrap();
    let r = lines.iter().position(|l| l == "r").unwrap();
    assert!(b < r);
}

#[test]
fn when_running_rules() {
    let f = Fixture::new();
    let mut app = f.app(
        "Busy",
        &[("run", "Start-Sleep 30"), ("build", "Write-Output 'built'")],
    );
    let first = f.start(&app, "run");

    // ask (default): needs a decision.
    assert!(matches!(
        f.manager.start(RunRequest::ui(&app.id, "build")).unwrap(),
        StartOutcome::NeedsDecision { active } if active.len() == 1
    ));

    // reject.
    app.action_mut("build").unwrap().when_running = WhenRunning::Reject;
    f.store().save_app(&app).unwrap();
    let mut req = RunRequest::ui(&app.id, "build");
    req.decision = Some(WhenRunning::Parallel);
    assert!(matches!(
        f.manager.start(req.clone()).unwrap(),
        StartOutcome::Rejected { .. }
    ));

    // parallel.
    app.action_mut("build").unwrap().when_running = WhenRunning::Parallel;
    f.store().save_app(&app).unwrap();
    let StartOutcome::Started(parallel) =
        f.manager.start(RunRequest::ui(&app.id, "build")).unwrap()
    else {
        panic!("parallel run did not start")
    };
    assert!(parallel.wait(Duration::from_secs(30)));
    assert!(!first.is_done());

    // ask answered with restart.
    app.action_mut("build").unwrap().when_running = WhenRunning::Ask;
    f.store().save_app(&app).unwrap();
    req.decision = Some(WhenRunning::Restart);
    let StartOutcome::Started(restarted) = f.manager.start(req).unwrap() else {
        panic!("restart did not start")
    };
    assert!(first.is_done());
    assert_eq!(first.info().status, RunStatus::Stopped);
    assert!(restarted.wait(Duration::from_secs(30)));
    assert_eq!(restarted.info().status, RunStatus::Success);
}

#[test]
fn retention_off_writes_nothing_to_disk() {
    let f = Fixture::new();
    let mut app = f.app("Quiet", &[("build", "Write-Output 'mem only'")]);
    app.action_mut("build").unwrap().retention = Retention::Off;
    f.store().save_app(&app).unwrap();
    let run = f.finish(&app, "build");
    assert!(run.info().log_path.is_none());
    assert!(texts(&run).contains(&"mem only".to_string()));
    assert!(f.store().history(&app.id).read().unwrap().is_empty());
    assert!(f.manager.last_for_app(&app.id).is_some());
}

#[test]
fn retention_last_is_applied_after_runs() {
    let f = Fixture::new();
    let mut app = f.app("Keep", &[("build", "Write-Output 'x'")]);
    app.action_mut("build").unwrap().retention = Retention::Last { count: 2 };
    f.store().save_app(&app).unwrap();
    let mut logs = Vec::new();
    for _ in 0..3 {
        logs.push(f.finish(&app, "build").info().log_path.unwrap());
    }
    let records = f.store().history(&app.id).read().unwrap();
    assert_eq!(records.len(), 2);
    assert!(!logs[0].exists());
    assert!(logs[2].exists());
}

#[test]
fn events_are_delivered() {
    use std::sync::{Arc, Mutex};
    let f = Fixture::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    f.manager.subscribe(move |e| {
        let name = match e {
            runbay::runner::RunEvent::Started(_) => "started",
            runbay::runner::RunEvent::Output { .. } => "output",
            runbay::runner::RunEvent::Finished(_) => "finished",
        };
        sink.lock().unwrap().push(name);
    });
    let app = f.app(
        "Events",
        &[("build", "1..5 | ForEach-Object { Write-Output $_ }")],
    );
    f.finish(&app, "build");
    // Finished is emitted right after `done` is set; give it a moment.
    std::thread::sleep(Duration::from_millis(200));
    let events = events.lock().unwrap();
    assert_eq!(events.first(), Some(&"started"));
    assert!(events.contains(&"output"));
    assert_eq!(events.last(), Some(&"finished"));
}
