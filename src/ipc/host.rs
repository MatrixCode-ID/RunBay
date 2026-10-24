//! GUI side of the pipe: sessions of connected agents, access control and confirmation,
//! tool execution, and undo of agent changes.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::PROTOCOL_VERSION;
use super::protocol::{Hello, Reply, mismatch_error};
use super::tools;
use crate::data::{McpAccess, Settings, Store};
use crate::mcp::catalog;
use crate::runner::RunManager;

/// Answer to a confirmation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Allow this call.
    Allow,
    /// Allow this call and every later call of the same agent until it disconnects.
    AllowAll,
    /// Deny (also the answer when nobody answers within 2 minutes).
    Deny,
}

/// What an agent asks to do, in plain words.
#[derive(Debug, Clone)]
pub struct ConfirmRequest {
    /// Agent name.
    pub agent: String,
    /// The operation, e.g. `Run "Build" of My App`.
    pub operation: String,
    /// `Allow all` is not offered (used for `delete_app`).
    pub single_only: bool,
}

/// The files of an app (without its history) captured before an agent changed it.
#[derive(Debug, Clone, Default)]
pub struct AppSnapshot {
    files: Vec<(PathBuf, Vec<u8>)>,
}

/// How to reverse one agent change.
#[derive(Debug, Clone)]
pub enum Undo {
    /// Restore an app's files; `None` removes an app the agent created.
    App {
        /// App id.
        app_id: String,
        /// The previous state.
        before: Option<AppSnapshot>,
    },
    /// Move a deleted app back from the trash.
    Deleted {
        /// App id.
        app_id: String,
        /// Where the app folder was moved.
        trash: PathBuf,
    },
    /// Restore the previous settings.
    Settings(Box<Settings>),
}

/// What the GUI does for agents; implemented by the window controller (and by tests).
pub trait AgentUi: Send + Sync {
    /// Shows the confirmation dialog and blocks until it is answered or 2 minutes pass.
    fn confirm(&self, request: ConfirmRequest) -> Decision;
    /// The set of connected agents changed.
    fn agents_changed(&self, agents: Vec<String>);
    /// An agent changed an app, action, script, or setting.
    fn changed(&self, agent: &str, description: String, undo: Undo);
}

struct Session {
    agent: String,
    allow_all: bool,
}

/// Executes agent requests against the data and the runner.
pub struct Host {
    /// The data folder.
    pub store: Store,
    /// The run manager.
    pub manager: RunManager,
    /// Settings shared with the window.
    pub settings: Arc<RwLock<Settings>>,
    ui: Arc<dyn AgentUi>,
    sessions: Mutex<HashMap<String, Session>>,
}

impl Host {
    /// Creates the host.
    pub fn new(
        store: Store,
        manager: RunManager,
        settings: Arc<RwLock<Settings>>,
        ui: Arc<dyn AgentUi>,
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            manager,
            settings,
            ui,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    /// Names of the connected agents.
    pub fn agents(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .sessions
            .lock()
            .unwrap()
            .values()
            .map(|s| s.agent.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Handles a session or call connection whose first message was `hello`.
    pub fn serve(self: &Arc<Self>, hello: Hello, reader: BufReader<File>) {
        if let Some(protocol) = hello.protocol()
            && protocol != PROTOCOL_VERSION
        {
            let mut file = reader.into_inner();
            let _ = write_reply(
                &mut file,
                &Reply::Refused {
                    error: mismatch_error(protocol),
                },
            );
            return;
        }
        match hello {
            Hello::Activate => {}
            Hello::Session { session, agent, .. } => self.session(session, agent, reader),
            Hello::Call {
                session,
                tool,
                args,
                progress,
                ..
            } => {
                let mut file = reader.into_inner();
                let reply = self.call(&session, &tool, args, progress, &mut file);
                let _ = write_reply(&mut file, &reply);
            }
        }
    }

    fn session(&self, id: String, agent: String, mut reader: BufReader<File>) {
        self.sessions.lock().unwrap().insert(
            id.clone(),
            Session {
                agent,
                allow_all: false,
            },
        );
        self.ui.agents_changed(self.agents());
        if write_reply(
            reader.get_mut(),
            &Reply::Welcome {
                protocol: PROTOCOL_VERSION,
            },
        )
        .is_ok()
        {
            // The bridge never writes again; EOF means it exited.
            let mut sink = String::new();
            while matches!(reader.read_line(&mut sink), Ok(n) if n > 0) {
                sink.clear();
            }
        }
        self.sessions.lock().unwrap().remove(&id);
        self.ui.agents_changed(self.agents());
    }

    fn call(
        &self,
        session: &str,
        tool: &str,
        args: Value,
        progress: bool,
        out: &mut File,
    ) -> Reply {
        match self.call_inner(session, tool, args, progress, out) {
            Ok(value) => Reply::Result { value },
            Err(e) => Reply::Error {
                message: format!("{e:#}"),
            },
        }
    }

    fn call_inner(
        &self,
        session: &str,
        tool: &str,
        args: Value,
        progress: bool,
        out: &mut File,
    ) -> Result<Value> {
        let access = self.settings.read().unwrap().mcp_access;
        if access == McpAccess::Off {
            bail!("MCP access is turned off in RunBay");
        }
        let def = catalog::find(tool).with_context(|| format!("unknown tool \"{tool}\""))?;
        let (agent, allow_all) = {
            let sessions = self.sessions.lock().unwrap();
            let s = sessions
                .get(session)
                .context("unknown session; reconnect the MCP server")?;
            (s.agent.clone(), s.allow_all)
        };
        let always = tool == "delete_app";
        let needs_confirmation =
            always || (!def.read_only && access == McpAccess::AskFirst && !allow_all);
        if needs_confirmation {
            let operation = tools::describe(self, tool, &args);
            let decision = self.ui.confirm(ConfirmRequest {
                agent: agent.clone(),
                operation: operation.clone(),
                single_only: always,
            });
            match decision {
                Decision::Allow => {}
                Decision::AllowAll => {
                    if let Some(s) = self.sessions.lock().unwrap().get_mut(session) {
                        s.allow_all = true;
                    }
                }
                Decision::Deny => {
                    bail!("The user denied the request in RunBay: {operation}")
                }
            }
        }
        let mut emit = |count: u64, lines: Vec<String>| {
            if progress {
                let _ = write_reply(
                    out,
                    &Reply::Progress {
                        progress: count,
                        lines,
                    },
                );
            }
        };
        tools::execute(self, &agent, tool, &args, &mut emit)
    }

    /// Reports an agent change to the GUI.
    pub(super) fn changed(&self, agent: &str, description: String, undo: Undo) {
        self.ui.changed(agent, description, undo);
    }

    /// Captures the files of an app (without history).
    pub fn snapshot(&self, app_id: &str) -> Result<AppSnapshot> {
        let dir = self.store.app_dir(app_id);
        let mut files = Vec::new();
        for entry in fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_file() {
                files.push((PathBuf::from(&name), fs::read(&path)?));
            } else if name == "actions" {
                for script in fs::read_dir(&path)?.flatten() {
                    if script.path().is_file() {
                        files.push((
                            PathBuf::from("actions").join(script.file_name()),
                            fs::read(script.path())?,
                        ));
                    }
                }
            }
        }
        Ok(AppSnapshot { files })
    }

    /// Reverses an agent change.
    pub fn undo(&self, undo: Undo) -> Result<()> {
        match undo {
            Undo::App {
                app_id,
                before: None,
            } => self.store.delete_app(&app_id),
            Undo::App {
                app_id,
                before: Some(snapshot),
            } => {
                let dir = self.store.app_dir(&app_id);
                let _ = fs::remove_dir_all(dir.join("actions"));
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        if entry.path().is_file() {
                            let _ = fs::remove_file(entry.path());
                        }
                    }
                }
                for (relative, bytes) in snapshot.files {
                    crate::data::atomic::write_atomic(&dir.join(relative), &bytes)?;
                }
                Ok(())
            }
            Undo::Deleted { app_id, trash } => {
                let dir = self.store.app_dir(&app_id);
                if dir.exists() {
                    bail!("an app with id \"{app_id}\" exists again");
                }
                fs::rename(&trash, &dir).context("cannot restore the app from the trash")
            }
            Undo::Settings(settings) => {
                self.store.save_settings(&settings)?;
                *self.settings.write().unwrap() = *settings;
                Ok(())
            }
        }
    }
}

fn write_reply(out: &mut File, reply: &Reply) -> Result<()> {
    let mut line = serde_json::to_string(reply)?;
    line.push('\n');
    out.write_all(line.as_bytes())?;
    out.flush()?;
    Ok(())
}
