//! The `--mcp` bridge: an MCP server over stdio that forwards every tool call to the GUI
//! instance through the named pipe. It holds no business logic.

pub mod catalog;

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ProgressNotificationParam, ServerCapabilities,
    ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::Value;
use windows::Win32::Foundation::{HANDLE_FLAG_INHERIT, HANDLE_FLAGS, SetHandleInformation};
use windows::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
};

use crate::ipc::protocol::{Hello, Reply};
use crate::ipc::{PROTOCOL_VERSION, pipe};

/// How long the bridge waits for a GUI it started.
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// Options of the bridge process.
#[derive(Debug, Clone)]
pub struct BridgeOptions {
    /// The pipe of the GUI instance.
    pub pipe_name: String,
    /// Arguments used to start the GUI when it is not running.
    pub gui_args: Vec<String>,
}

/// Runs the bridge until stdin closes.
pub fn run_bridge(options: BridgeOptions) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let bridge = Bridge {
            options,
            session_id: format!(
                "{}-{}",
                std::process::id(),
                chrono::Local::now().timestamp()
            ),
            session: Arc::new(Mutex::new(None)),
        };
        let service = bridge
            .serve(rmcp::transport::stdio())
            .await
            .context("cannot start the MCP server")?;
        service.waiting().await?;
        anyhow::Ok(())
    })
}

/// The open session connection (kept open so the GUI knows the agent is connected).
struct Session {
    agent: String,
    _pipe: File,
}

struct Bridge {
    options: BridgeOptions,
    session_id: String,
    session: Arc<Mutex<Option<Session>>>,
}

impl Bridge {
    /// Connects the session, starting the GUI when it is not running.
    fn ensure_session(&self, agent: &str) -> Result<()> {
        let mut session = self.session.lock().unwrap();
        if session.as_ref().is_some_and(|s| s.agent == agent) {
            return Ok(());
        }
        let mut pipe = match pipe::connect(&self.options.pipe_name, 2000)? {
            Some(pipe) => pipe,
            None => self.start_gui()?,
        };
        send(
            &mut pipe,
            &Hello::Session {
                protocol: PROTOCOL_VERSION,
                session: self.session_id.clone(),
                agent: agent.to_string(),
            },
        )?;
        let mut reader = BufReader::new(pipe.try_clone()?);
        match read_reply(&mut reader)? {
            Reply::Welcome { .. } => {}
            Reply::Refused { error } => bail!(error),
            other => bail!("unexpected reply from RunBay: {other:?}"),
        }
        // Watch for the GUI going away so the next call reconnects.
        let watched = self.session.clone();
        std::thread::spawn(move || {
            let mut line = String::new();
            while matches!(reader.read_line(&mut line), Ok(n) if n > 0) {
                line.clear();
            }
            *watched.lock().unwrap() = None;
        });
        *session = Some(Session {
            agent: agent.to_string(),
            _pipe: pipe,
        });
        Ok(())
    }

    fn start_gui(&self) -> Result<File> {
        let exe = std::env::current_exe()?;
        // The GUI must not inherit the agent's stdio pipes.
        for std in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            set_not_inheritable(std);
        }
        let spawn = |flags| {
            Command::new(&exe)
                .args(&self.options.gui_args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(flags)
                .spawn()
        };
        let base = DETACHED_PROCESS.0 | CREATE_NEW_PROCESS_GROUP.0;
        // Leave the agent's job when allowed, so closing the agent does not close RunBay.
        spawn(base | CREATE_BREAKAWAY_FROM_JOB.0)
            .or_else(|_| spawn(base))
            .context("cannot start RunBay")?;
        let deadline = Instant::now() + START_TIMEOUT;
        while Instant::now() < deadline {
            if let Some(pipe) = pipe::connect(&self.options.pipe_name, 500)? {
                return Ok(pipe);
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        bail!("RunBay did not start within 10 seconds")
    }

    /// Performs one tool call over its own connection; progress lines go to `on_progress`.
    fn call(
        &self,
        agent: &str,
        tool: &str,
        args: Value,
        progress: bool,
        mut on_progress: impl FnMut(u64, Vec<String>),
    ) -> Result<Result<Value, String>> {
        self.ensure_session(agent)?;
        let mut pipe =
            pipe::connect(&self.options.pipe_name, 5000)?.context("RunBay is not running")?;
        send(
            &mut pipe,
            &Hello::Call {
                protocol: PROTOCOL_VERSION,
                session: self.session_id.clone(),
                tool: tool.to_string(),
                args,
                progress,
            },
        )?;
        let mut reader = BufReader::new(pipe);
        loop {
            match read_reply(&mut reader)? {
                Reply::Progress { progress, lines } => on_progress(progress, lines),
                Reply::Result { value } => return Ok(Ok(value)),
                Reply::Error { message } => return Ok(Err(message)),
                Reply::Refused { error } => return Ok(Err(error)),
                Reply::Welcome { .. } => {}
            }
        }
    }
}

fn set_not_inheritable(which: STD_HANDLE) {
    // SAFETY: plain handle queries on this process's standard handles.
    unsafe {
        if let Ok(handle) = GetStdHandle(which)
            && !handle.is_invalid()
        {
            let _ = SetHandleInformation(handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
        }
    }
}

fn send(pipe: &mut File, hello: &Hello) -> Result<()> {
    let mut line = serde_json::to_string(hello)?;
    line.push('\n');
    pipe.write_all(line.as_bytes())?;
    pipe.flush()?;
    Ok(())
}

fn read_reply(reader: &mut BufReader<File>) -> Result<Reply> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        bail!("RunBay closed the connection");
    }
    serde_json::from_str(line.trim()).context("invalid reply from RunBay")
}

fn tool_list() -> Vec<Tool> {
    catalog::TOOLS
        .iter()
        .map(|t| {
            let schema = match (t.schema)() {
                Value::Object(map) => map,
                _ => serde_json::Map::new(),
            };
            Tool::new(t.name, t.description, Arc::new(schema))
        })
        .collect()
}

impl ServerHandler for Bridge {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("RunBay", crate::VERSION);
        info.instructions = Some(
            "RunBay builds and runs registered apps through PowerShell actions. Use list_apps \
             to see apps and actions, run_action to start one (wait=true to get the result), \
             and read_output to follow a run. Every change is shown in the RunBay window."
                .into(),
        );
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tool_list()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let agent = context
            .peer
            .peer_info()
            .map(|info| info.client_info.name.clone())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Agent".to_string());
        let token = context.meta.get_progress_token();
        let args = Value::Object(request.arguments.clone().unwrap_or_default());
        let tool = request.name.to_string();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(u64, Vec<String>)>();
        let peer = context.peer.clone();
        let forward = token.clone().map(|token| {
            tokio::spawn(async move {
                while let Some((progress, lines)) = rx.recv().await {
                    let mut param = ProgressNotificationParam::new(token.clone(), progress as f64);
                    param.message = Some(lines.join("\n"));
                    let _ = peer.notify_progress(param).await;
                }
            })
        });

        let bridge = Bridge {
            options: self.options.clone(),
            session_id: self.session_id.clone(),
            session: self.session.clone(),
        };
        let wants_progress = token.is_some();
        let result = tokio::task::spawn_blocking(move || {
            bridge.call(&agent, &tool, args, wants_progress, |p, lines| {
                let _ = tx.send((p, lines));
            })
        })
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        if let Some(forward) = forward {
            let _ = forward.await;
        }

        let result = match result {
            Ok(Ok(value)) => {
                let text = serde_json::to_string_pretty(&value).unwrap_or_default();
                let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
                result.structured_content = Some(value);
                result
            }
            Ok(Err(message)) => CallToolResult::error(vec![ContentBlock::text(message)]),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("{e:#}"))]),
        };
        Ok(result.into())
    }
}

/// Arguments for starting the GUI from the bridge: `--autostart` (last mode and position)
/// plus the development data folder when one is used.
pub fn gui_args(custom_data_dir: Option<&PathBuf>) -> Vec<String> {
    let mut args = vec!["--autostart".to_string()];
    if let Some(dir) = custom_data_dir {
        args.push("--data-dir".into());
        args.push(dir.to_string_lossy().into_owned());
    }
    args
}
