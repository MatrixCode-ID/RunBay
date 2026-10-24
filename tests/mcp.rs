//! MCP integration test: a headless RunBay instance with a temporary data folder, driven
//! through `runbay.exe --mcp` over stdio.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const EXE: &str = env!("CARGO_BIN_EXE_runbay");

struct Instance {
    child: Child,
}

impl Instance {
    /// Starts a headless instance on `data` with the given MCP access.
    fn start(data: &Path, access: &str) -> Self {
        std::fs::create_dir_all(data.join("meta")).unwrap();
        std::fs::write(
            data.join("meta").join("settings.json"),
            json!({ "mcpAccess": access }).to_string(),
        )
        .unwrap();
        let child = Command::new(EXE)
            .args(["--headless", "--data-dir"])
            .arg(data)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let name = runbay::ipc::pipe::pipe_name(Some(data)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if runbay::ipc::pipe::connect(&name, 200).unwrap().is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the headless instance did not start"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        Self { child }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    notifications: Vec<Value>,
}

impl Client {
    fn connect(data: &Path) -> Self {
        let mut child = Command::new(EXE)
            .args(["--mcp", "--data-dir"])
            .arg(data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut client = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            notifications: Vec::new(),
        };
        let init = client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "Test Agent", "version": "1.0" }
            }),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "RunBay");
        client.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        client
    }

    fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "the bridge closed stdout"
            );
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
            self.notifications.push(message);
        }
    }

    /// Calls a tool; returns (is_error, structured result or error text).
    fn call(&mut self, tool: &str, args: Value) -> (bool, Value) {
        self.call_with_meta(tool, args, None)
    }

    fn call_with_meta(&mut self, tool: &str, args: Value, meta: Option<Value>) -> (bool, Value) {
        let mut params = json!({ "name": tool, "arguments": args });
        if let Some(meta) = meta {
            params["_meta"] = meta;
        }
        let response = self.request("tools/call", params);
        let result = &response["result"];
        assert!(result.is_object(), "no result: {response}");
        let is_error = result["isError"].as_bool().unwrap_or(false);
        if is_error {
            (true, result["content"][0]["text"].clone())
        } else {
            (false, result["structuredContent"].clone())
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn agent_can_manage_and_run_apps() {
    let dir = tempfile::tempdir().unwrap();
    let data = std::path::absolute(dir.path().join("data")).unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let _instance = Instance::start(&data, "unrestricted");
    let mut client = Client::connect(&data);

    let tools = client.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"run_action") && names.contains(&"update_settings"));

    let (err, created) = client.call(
        "create_app",
        json!({ "name": "Agent App", "working_directory": work.to_string_lossy() }),
    );
    assert!(!err, "{created}");
    let app_id = created["app"]["id"].as_str().unwrap().to_string();

    let (err, updated) = client.call(
        "set_action",
        json!({ "app_id": app_id, "action_id": "build", "icon": "rocket_launch" }),
    );
    assert!(!err, "{updated}");
    assert_eq!(updated["action"]["icon"], "rocket_launch");
    let (err, loaded) = client.call("get_app", json!({ "app_id": app_id }));
    assert!(!err, "{loaded}");
    assert_eq!(loaded["actions"][0]["icon"], "rocket_launch");
    let (err, _) = client.call(
        "set_action",
        json!({ "app_id": app_id, "action_id": "build", "icon": "unknown_symbol" }),
    );
    assert!(err);
    let (err, reset) = client.call(
        "set_action",
        json!({ "app_id": app_id, "action_id": "build", "icon": null }),
    );
    assert!(!err, "{reset}");
    assert!(reset["action"]["icon"].is_null());

    let (err, _) = client.call(
        "set_action",
        json!({ "app_id": app_id, "action_id": "build",
                "script": "Write-Output 'agent build'\nWrite-Output \"by $env:RUNBAY_ACTION_ID\"" }),
    );
    assert!(!err);
    let (err, _) = client.call(
        "set_action",
        json!({ "app_id": app_id, "action_id": "run", "script": "1..3 | % { Write-Output \"line $_\"; Start-Sleep -Milliseconds 300 }" }),
    );
    assert!(!err);

    // A build with wait=true returns status, exit code, and the tail.
    let (err, result) = client.call(
        "run_action",
        json!({ "app_id": app_id, "action_id": "build", "wait": true }),
    );
    assert!(!err, "{result}");
    assert_eq!(result["status"], "success");
    assert_eq!(result["exit_code"], 0);
    let tail: Vec<&str> = result["tail"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        tail.contains(&"agent build") && tail.contains(&"by build"),
        "{tail:?}"
    );

    // An asynchronous run: start, then read its output and wait for it with progress.
    let (err, started) = client.call(
        "run_action",
        json!({ "app_id": app_id, "action_id": "run" }),
    );
    assert!(!err, "{started}");
    let run_id = started["run_id"].as_str().unwrap().to_string();

    // While it runs, the ask rule needs if_running.
    let (err, text) = client.call(
        "run_action",
        json!({ "app_id": app_id, "action_id": "build" }),
    );
    assert!(
        err && text.as_str().unwrap().contains("app is running"),
        "{text}"
    );

    let (err, waited) = client.call_with_meta(
        "wait_run",
        json!({ "run_id": run_id, "timeout_seconds": 30 }),
        Some(json!({ "progressToken": "p1" })),
    );
    assert!(!err, "{waited}");
    assert_eq!(waited["status"], "success");
    assert!(
        client
            .notifications
            .iter()
            .any(|n| n["method"] == "notifications/progress"),
        "no progress notifications"
    );

    let (err, output) = client.call("read_output", json!({ "run_id": run_id, "tail": 2 }));
    assert!(!err);
    assert_eq!(output["lines"], json!(["line 2", "line 3"]));

    let (err, run) = client.call("get_run", json!({ "run_id": run_id }));
    assert!(!err);
    assert_eq!(run["source"], "mcp");
    assert_eq!(run["agent"], "Test Agent");

    let (err, runs) = client.call("list_runs", json!({ "app_id": app_id, "source": "mcp" }));
    assert!(!err);
    assert_eq!(runs["runs"].as_array().unwrap().len(), 2);

    let (err, apps) = client.call("list_apps", json!({}));
    assert!(!err);
    assert_eq!(apps["apps"][0]["last_status"], "success");

    let (err, app) = client.call("get_app", json!({ "app_id": app_id }));
    assert!(!err);
    assert!(
        app["scripts"]["build"]
            .as_str()
            .unwrap()
            .contains("agent build")
    );

    // Settings: MCP access cannot be changed by an agent.
    let (err, text) = client.call("update_settings", json!({ "mcp_access": "off" }));
    assert!(err && text.as_str().unwrap().contains("only be changed in RunBay"));
    let (err, settings) = client.call("update_settings", json!({ "output_line_limit": 5000 }));
    assert!(!err);
    assert_eq!(settings["outputLineLimit"], 5000);

    let (err, settings) = client.call(
        "update_settings",
        json!({ "theme": "light", "console_theme": "dark" }),
    );
    assert!(!err);
    assert_eq!(settings["theme"], "light");
    assert_eq!(settings["consoleTheme"], "dark");
    let (err, settings) = client.call("get_settings", json!({}));
    assert!(!err);
    assert_eq!(settings["consoleTheme"], "dark");
    let (err, _) = client.call("update_settings", json!({ "console_theme": "system" }));
    assert!(err);
    let (err, settings) = client.call("update_settings", json!({ "console_theme": "follow" }));
    assert!(!err);
    assert_eq!(settings["consoleTheme"], "follow");
    assert_eq!(settings["theme"], "light");
    assert_eq!(settings["taskbarFull"], true);
    assert_eq!(settings["taskbarCompact"], true);
    let (err, settings) = client.call(
        "update_settings",
        json!({ "taskbar_full": false, "taskbar_compact": false }),
    );
    assert!(!err);
    assert_eq!(settings["taskbarFull"], false);
    assert_eq!(settings["taskbarCompact"], false);

    // History on disk records the agent.
    let index = std::fs::read_to_string(
        data.join("apps")
            .join(&app_id)
            .join("history")
            .join("index.jsonl"),
    )
    .unwrap();
    assert!(index.contains("\"source\":\"mcp\"") && index.contains("\"agent\":\"Test Agent\""));
}

#[test]
fn access_off_refuses_every_tool() {
    let dir = tempfile::tempdir().unwrap();
    let data = std::path::absolute(dir.path().join("data")).unwrap();
    let _instance = Instance::start(&data, "off");
    let mut client = Client::connect(&data);
    let (err, text) = client.call("list_apps", json!({}));
    assert!(err);
    assert_eq!(text, "MCP access is turned off in RunBay");
}

#[test]
fn agents_can_remove_and_restore_standard_actions() {
    let dir = tempfile::tempdir().unwrap();
    let data = std::path::absolute(dir.path().join("data")).unwrap();
    let _instance = Instance::start(&data, "unrestricted");
    let mut client = Client::connect(&data);
    let (err, created) = client.call("create_app", json!({"name": "Flexible App"}));
    assert!(!err, "{created}");
    let id = created["app"]["id"].as_str().unwrap();
    for action in ["build", "run", "build-run"] {
        let (err, value) = client.call("delete_action", json!({"app_id": id, "action_id": action}));
        assert!(!err, "{value}");
    }
    let (err, empty) = client.call("get_app", json!({"app_id": id}));
    assert!(!err, "{empty}");
    assert_eq!(empty["actions"], json!([]));
    let (err, build) = client.call(
        "set_action",
        json!({"app_id": id, "kind": "build", "script": "Write-Output 'restored'"}),
    );
    assert!(!err, "{build}");
    assert_eq!(build["action"]["kind"], "build");
    let (err, _) = client.call(
        "set_action",
        json!({"app_id": id, "action_id": "another-build", "kind": "build"}),
    );
    assert!(err);
    let (err, _) = client.call(
        "set_action",
        json!({"app_id": id, "action_id": "build", "kind": "run"}),
    );
    assert!(err);
    let (err, run) = client.call(
        "set_action",
        json!({"app_id": id, "action_id": "run", "script": "Start-Sleep -Seconds 30"}),
    );
    assert!(!err, "{run}");
    assert_eq!(run["action"]["kind"], "run");
    let (err, started) = client.call("run_action", json!({"app_id": id, "action_id": "run"}));
    assert!(!err, "{started}");
    let run_id = started["run_id"].as_str().unwrap();
    let (err, blocked) = client.call("delete_action", json!({"app_id": id, "action_id": "run"}));
    assert!(
        err && blocked.as_str().unwrap().contains("stop the action"),
        "{blocked}"
    );
    let (err, _) = client.call("stop_run", json!({"run_id": run_id}));
    assert!(!err);
    let (err, _) = client.call("wait_run", json!({"run_id": run_id, "timeout_seconds": 10}));
    assert!(!err);
    let (err, _) = client.call("delete_action", json!({"app_id": id, "action_id": "run"}));
    assert!(!err);
    let (err, composite) = client.call("set_action", json!({"app_id": id, "kind": "buildRun"}));
    assert!(!err, "{composite}");
    let (err, missing) = client.call(
        "run_action",
        json!({"app_id": id, "action_id": "build-run"}),
    );
    assert!(
        err && missing.as_str().unwrap().contains("no run action"),
        "{missing}"
    );
    let (err, folder) = client.call("set_action", json!({"app_id": id, "action_id": "open-dist", "name": "Open dist folder", "icon": "folder_open", "script": "Write-Output 'folder'"}));
    assert!(!err, "{folder}");
    assert_eq!(folder["action"]["icon"], "folder_open");
    let (err, completed) = client.call(
        "run_action",
        json!({"app_id": id, "action_id": "build", "wait": true}),
    );
    assert!(!err, "{completed}");
    assert_eq!(completed["exit_code"], 0);
    let (err, app) = client.call("get_app", json!({"app_id": id}));
    assert!(!err, "{app}");
    assert!(app["scripts"].get("run").is_none());
}

#[test]
fn ask_first_runs_read_tools_and_asks_for_others() {
    let dir = tempfile::tempdir().unwrap();
    let data = std::path::absolute(dir.path().join("data")).unwrap();
    let _instance = Instance::start(&data, "askFirst");
    let mut client = Client::connect(&data);
    let (err, _) = client.call("list_apps", json!({}));
    assert!(!err);
    // The headless instance cannot show the dialog, so the request is denied.
    let (err, text) = client.call("create_app", json!({ "name": "Nope" }));
    assert!(err);
    assert!(text.as_str().unwrap().contains("denied"), "{text}");
    assert!(!data.join("apps").join("nope").exists());
}
