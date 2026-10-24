//! Messages of the internal pipe protocol (JSON lines).
//!
//! A bridge keeps one **session** connection open while it runs (it tells the GUI which
//! agent is connected and ends with the bridge), and opens a new **call** connection for
//! every tool call, so no connection is ever read and written at the same time.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::PROTOCOL_VERSION;

/// The first message of every connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Hello {
    /// Another RunBay process asks the running instance to show its window.
    Activate,
    /// A bridge announces an agent; the connection stays open until the bridge exits.
    Session {
        /// Protocol version of the bridge.
        protocol: u32,
        /// Session id chosen by the bridge (unique per bridge process).
        session: String,
        /// Agent name from MCP `clientInfo.name`.
        agent: String,
    },
    /// One tool call.
    Call {
        /// Protocol version of the bridge.
        protocol: u32,
        /// Session id of the bridge making the call.
        session: String,
        /// Tool name.
        tool: String,
        /// Tool arguments.
        #[serde(default)]
        args: Value,
        /// The agent wants progress messages.
        #[serde(default)]
        progress: bool,
    },
}

impl Hello {
    /// The protocol version carried by the message (activation has none).
    pub fn protocol(&self) -> Option<u32> {
        match self {
            Hello::Activate => None,
            Hello::Session { protocol, .. } | Hello::Call { protocol, .. } => Some(*protocol),
        }
    }
}

/// A message from the GUI on a session or call connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Reply {
    /// The session was accepted.
    Welcome {
        /// Protocol version of the GUI.
        protocol: u32,
    },
    /// The connection was refused (e.g. protocol mismatch).
    Refused {
        /// Why.
        error: String,
    },
    /// New output of a run being waited on.
    Progress {
        /// Number of lines delivered so far.
        progress: u64,
        /// The new lines.
        lines: Vec<String>,
    },
    /// The tool succeeded.
    Result {
        /// The tool result (JSON).
        value: Value,
    },
    /// The tool failed; the message is meant for the agent.
    Error {
        /// What went wrong.
        message: String,
    },
}

/// The error text for a bridge and GUI with different protocol versions.
pub fn mismatch_error(bridge: u32) -> String {
    format!(
        "RunBay protocol mismatch: the MCP bridge speaks version {bridge}, the running RunBay \
         speaks version {PROTOCOL_VERSION}. Restart RunBay (it was probably just updated)."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_round_trip() {
        let call = Hello::Call {
            protocol: 1,
            session: "s".into(),
            tool: "list_apps".into(),
            args: serde_json::json!({}),
            progress: false,
        };
        let text = serde_json::to_string(&call).unwrap();
        assert!(text.contains("\"type\":\"call\""));
        let back: Hello = serde_json::from_str(&text).unwrap();
        assert_eq!(back.protocol(), Some(1));
        let activate: Hello = serde_json::from_str(r#"{"type":"activate"}"#).unwrap();
        assert!(matches!(activate, Hello::Activate));
    }

    #[test]
    fn reply_round_trip() {
        let r = Reply::Progress {
            progress: 3,
            lines: vec!["a".into()],
        };
        let text = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), r);
    }
}
