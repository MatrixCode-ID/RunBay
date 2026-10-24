//! Clipboard instructions for connecting this RunBay installation to MCP clients.

use std::path::Path;

use serde_json::json;

use super::Controller;
use crate::platform;

impl Controller {
    pub(super) fn copy_mcp_setup(&self) {
        let result = std::env::current_exe()
            .map_err(anyhow::Error::from)
            .and_then(|executable| {
                platform::copy_text(&instructions(&executable, &self.restart_args))
            });
        match result {
            Ok(()) => {
                self.snackbar("MCP setup instructions copied.".into());
            }
            Err(error) => {
                self.report(Err(error));
            }
        }
    }
}

fn instructions(executable: &Path, extra_args: &[String]) -> String {
    let executable = executable.to_string_lossy();
    let args: Vec<&str> = std::iter::once("--mcp")
        .chain(extra_args.iter().map(String::as_str))
        .collect();
    let claude = serde_json::to_string_pretty(&json!({
        "mcpServers": { "runbay": { "command": executable, "args": args } }
    }))
    .expect("MCP configuration contains only strings");
    // JSON string/array escaping is also valid for these TOML basic strings.
    let command_toml = serde_json::to_string(executable.as_ref()).unwrap();
    let args_toml = serde_json::to_string(&args).unwrap();
    let powershell = |value: &str| format!("'{}'", value.replace('\'', "''"));
    let cli = std::iter::once(executable.as_ref())
        .chain(args.iter().copied())
        .map(powershell)
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"Help me connect RunBay to the MCP client I am using, Claude Desktop or Codex,
on Windows. Follow the relevant section below and preserve all other settings
and MCP servers. If you cannot edit my client configuration, guide me through it.

This configuration uses the RunBay executable and launch arguments from the
installation where these instructions were copied. Verify that the executable
exists. On another computer, replace the executable and any --data-dir path with
the paths for that computer's RunBay installation.

RunBay is a local stdio MCP server. It starts its GUI when needed. No Node.js,
Python, API key, or separate server package is needed for RunBay.

Claude Desktop
1. Open Settings > Developer > Edit Config to locate the active configuration
   (normally %APPDATA%\Claude\claude_desktop_config.json on Windows).
2. Merge the runbay entry below into mcpServers, keeping other entries and keys:
```json
{claude}
```
3. Save the file, fully quit Claude Desktop, and reopen it to load the server.

Codex
Run this in PowerShell if the Codex CLI is installed:
```powershell
codex mcp add runbay -- {cli}
```
Alternatively, merge this table into the active Codex config.toml (normally
%USERPROFILE%\.codex\config.toml), preserving other configuration. Update an
existing runbay table instead of creating a duplicate:
```toml
[mcp_servers.runbay]
command = {command_toml}
args = {args_toml}
```
Reload or restart the Codex client to load the configuration. If the CLI is
available, use codex mcp list to check the registration.

Verify the connection
In RunBay, Settings > Agents > MCP access controls access. Ask first is the
default; Off refuses tool calls. Keep the access level chosen by the user.
After the client reloads, ask it to call RunBay's list_apps tool to verify the
connection. An empty app list is a successful connection to a new installation.

References:
https://modelcontextprotocol.io/docs/develop/connect-local-servers
https://developers.openai.com/codex/mcp
"#
    )
    .replace('\n', "\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_configurations_escape_paths_and_preserve_the_target_instance() {
        let executable = Path::new(r"C:\Tools\RunBay's $build\RunBay.exe");
        let extra = vec!["--data-dir".to_owned(), r"C:\Work\My App\data".to_owned()];
        let text = instructions(executable, &extra);
        let json = text
            .split("```json\r\n")
            .nth(1)
            .unwrap()
            .split("\r\n```")
            .next()
            .unwrap();
        let config: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(
            config["mcpServers"]["runbay"]["command"],
            executable.to_str().unwrap()
        );
        assert_eq!(
            config["mcpServers"]["runbay"]["args"],
            json!(["--mcp", "--data-dir", extra[1]])
        );
        assert!(text.contains("'C:\\Tools\\RunBay''s $build\\RunBay.exe' '--mcp' '--data-dir' 'C:\\Work\\My App\\data'"));
        let toml = text.split("```toml\r\n").nth(1).unwrap();
        let command = toml
            .lines()
            .find_map(|line| line.strip_prefix("command = "))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<String>(command).unwrap(),
            executable.to_str().unwrap()
        );
        assert!(!text.replace("\r\n", "").contains('\n'));
        let default_text = instructions(executable, &[]);
        assert!(!default_text.contains("'--data-dir'"));
        assert!(default_text.contains("args = [\"--mcp\"]"));
    }
}
