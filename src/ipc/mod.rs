//! Inter-process communication between RunBay processes over the per-user named pipe.
//!
//! The pipe serves single instance activation and (from the MCP bridge) the versioned
//! request/response protocol. Messages are JSON lines.

pub mod host;
pub mod pipe;
pub mod protocol;
pub(crate) mod tools;

use std::io::{BufRead, BufReader, Write};

use anyhow::Result;

pub use protocol::{Hello, Reply};

/// Version of the internal pipe protocol; a bridge and GUI with different versions refuse
/// to talk to each other.
pub const PROTOCOL_VERSION: u32 = 1;

/// Asks a running instance to show its window. Returns false when none is running.
pub fn activate_running_instance(name: &str) -> Result<bool> {
    let Some(mut pipe) = pipe::connect(name, 2000)? else {
        return Ok(false);
    };
    // Let the running instance bring its window to the foreground.
    crate::platform::allow_foreground();
    let mut line = serde_json::to_string(&Hello::Activate)?;
    line.push('\n');
    pipe.write_all(line.as_bytes())?;
    let mut reply = String::new();
    let _ = BufReader::new(pipe).read_line(&mut reply);
    Ok(true)
}

/// Reads the first line of a connection and parses it as [`Hello`].
pub fn read_hello(reader: &mut impl BufRead) -> Result<Option<Hello>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(line.trim())?))
}
