//! The MCP tools RunBay offers: names, descriptions, input schemas, and whether a tool only
//! reads (read-only tools never need confirmation).

use serde_json::{Value, json};

/// One tool definition.
pub struct ToolDef {
    /// Tool name.
    pub name: &'static str,
    /// What the tool does, for the agent.
    pub description: &'static str,
    /// The tool never changes anything.
    pub read_only: bool,
    /// JSON schema of the arguments.
    pub schema: fn() -> Value,
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

fn retention_schema() -> Value {
    json!({
        "type": "object",
        "description": "History retention: off, last (keep the last `count` runs), or forever.",
        "properties": {
            "mode": { "type": "string", "enum": ["off", "last", "forever"] },
            "count": { "type": "integer", "minimum": 1 }
        },
        "required": ["mode"]
    })
}

fn env_schema() -> Value {
    json!({
        "type": "object",
        "description": "Environment variables (name → value). Replaces the whole set.",
        "additionalProperties": { "type": "string" }
    })
}

/// All tools, in the order they are listed to agents.
pub const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "list_apps",
        description: "List the registered apps with id, name, group, order, last run status, and their actions.",
        read_only: true,
        schema: || object(json!({}), &[]),
    },
    ToolDef {
        name: "get_app",
        description: "Get one app with all settings, actions, and the contents of their scripts.",
        read_only: true,
        schema: || object(json!({ "app_id": { "type": "string" } }), &["app_id"]),
    },
    ToolDef {
        name: "list_runs",
        description: "Run history, newest first. Filter by app, action, status (running, success, failed, stopped, skipped), and source (ui, mcp).",
        read_only: true,
        schema: || {
            object(
                json!({
                    "app_id": { "type": "string" },
                    "action_id": { "type": "string" },
                    "status": { "type": "string", "enum": ["running", "success", "failed", "stopped", "skipped"] },
                    "source": { "type": "string", "enum": ["ui", "mcp"] },
                    "limit": { "type": "integer", "minimum": 1, "default": 50 }
                }),
                &[],
            )
        },
    },
    ToolDef {
        name: "get_run",
        description: "Status, exit code, start/end, source, and log path of one run.",
        read_only: true,
        schema: || object(json!({ "run_id": { "type": "string" } }), &["run_id"]),
    },
    ToolDef {
        name: "read_output",
        description: "Output lines of a run (live while running). Give `tail` for the last lines, or `offset` + `limit` for a range. Without both, the last 200 lines are returned.",
        read_only: true,
        schema: || {
            object(
                json!({
                    "run_id": { "type": "string" },
                    "tail": { "type": "integer", "minimum": 1 },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1 }
                }),
                &["run_id"],
            )
        },
    },
    ToolDef {
        name: "get_settings",
        description: "RunBay settings.",
        read_only: true,
        schema: || object(json!({}), &[]),
    },
    ToolDef {
        name: "run_action",
        description: "Start an action of an app. With wait=false (default) the run id is returned at once. With wait=true the call waits up to timeout_seconds (default 600) and returns status, exit code, duration, and the last `tail` lines (default 200); a run that outlives the timeout keeps running. When the app is already running and the action's rule is 'ask', pass if_running ('restart' or 'parallel'); without it the call fails and lists the active runs.",
        read_only: false,
        schema: || {
            object(
                json!({
                    "app_id": { "type": "string" },
                    "action_id": { "type": "string" },
                    "wait": { "type": "boolean", "default": false },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "default": 600 },
                    "tail": { "type": "integer", "minimum": 0, "default": 200 },
                    "if_running": { "type": "string", "enum": ["restart", "parallel"] }
                }),
                &["app_id", "action_id"],
            )
        },
    },
    ToolDef {
        name: "wait_run",
        description: "Wait for a started run to finish, up to timeout_seconds (default 600). Returns the same result as run_action with wait=true.",
        // Waiting changes nothing, so it is never confirmed.
        read_only: true,
        schema: || {
            object(
                json!({
                    "run_id": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "default": 600 },
                    "tail": { "type": "integer", "minimum": 0, "default": 200 }
                }),
                &["run_id"],
            )
        },
    },
    ToolDef {
        name: "stop_run",
        description: "Stop a run and its whole process tree.",
        read_only: false,
        schema: || object(json!({ "run_id": { "type": "string" } }), &["run_id"]),
    },
    ToolDef {
        name: "create_app",
        description: "Add an app. It gets Build, Run, and Build + Run actions without scripts; set them with set_action.",
        read_only: false,
        schema: || {
            object(
                json!({
                    "name": { "type": "string" },
                    "group": { "type": "string", "description": "Group id; empty for the default group." },
                    "working_directory": { "type": "string" },
                    "shell": { "type": "string", "enum": ["auto", "pwsh", "powershell"] },
                    "env": env_schema()
                }),
                &["name"],
            )
        },
    },
    ToolDef {
        name: "update_app",
        description: "Change app fields: name, group, order, working directory, shell, or environment.",
        read_only: false,
        schema: || {
            object(
                json!({
                    "app_id": { "type": "string" },
                    "name": { "type": "string" },
                    "group": { "type": "string" },
                    "order": { "type": "integer" },
                    "working_directory": { "type": "string" },
                    "shell": { "type": "string", "enum": ["auto", "pwsh", "powershell"] },
                    "env": env_schema()
                }),
                &["app_id"],
            )
        },
    },
    ToolDef {
        name: "delete_app",
        description: "Remove an app and its whole history. Always confirmed by the user in RunBay.",
        read_only: false,
        schema: || object(json!({ "app_id": { "type": "string" } }), &["app_id"]),
    },
    ToolDef {
        name: "set_app_icon",
        description: "Copy an image file (png, ico, or svg) as the app icon.",
        read_only: false,
        schema: || {
            object(
                json!({ "app_id": { "type": "string" }, "path": { "type": "string" } }),
                &["app_id", "path"],
            )
        },
    },
    ToolDef {
        name: "set_action",
        description: "Create or update an action. Use kind to restore a missing build, run, or buildRun role; without kind, reserved ids build/run/build-run restore that role and other new ids create custom actions. Existing kinds cannot change. The script is PowerShell; an empty script removes it. working_directory/env override the app's.",
        read_only: false,
        schema: || {
            object(
                json!({
                    "app_id": { "type": "string" },
                    "action_id": { "type": "string" },
                    "kind": { "type": "string", "enum": ["build", "run", "buildRun", "custom"] },
                    "name": { "type": "string" },
                    "icon": { "type": ["string", "null"], "description": "Action Material Symbol; null restores the default.", "enum": [null, "", "build", "play_arrow", "rocket_launch", "terminal", "refresh", "folder", "folder_open", "description", "settings", "desktop_windows", "bubble_chart"] },
                    "script": { "type": "string" },
                    "working_directory": { "type": ["string", "null"] },
                    "env": env_schema(),
                    "when_running": { "type": "string", "enum": ["ask", "reject", "restart", "parallel"] },
                    "retention": retention_schema()
                }),
                &["app_id"],
            )
        },
    },
    ToolDef {
        name: "delete_action",
        description: "Remove an action, including Build, Run, or Build + Run. An active action or a dependency of an active Build + Run must be stopped first.",
        read_only: false,
        schema: || {
            object(
                json!({ "app_id": { "type": "string" }, "action_id": { "type": "string" } }),
                &["app_id", "action_id"],
            )
        },
    },
    ToolDef {
        name: "update_settings",
        description: "Change RunBay settings. MCP access can only be changed in RunBay itself.",
        read_only: false,
        schema: || {
            object(
                json!({
                    "theme": { "type": "string", "enum": ["system", "light", "dark"] },
                    "console_theme": { "type": "string", "enum": ["follow", "light", "dark"] },
                    "autostart": { "type": "boolean" },
                    "icon_size": { "type": "string", "enum": ["small", "medium", "large"] },
                    "always_on_top_compact": { "type": "boolean" },
                    "always_on_top_icon": { "type": "boolean" },
                    "taskbar_full": { "type": "boolean" },
                    "taskbar_compact": { "type": "boolean" },
                    "output_line_limit": { "type": "integer", "minimum": 100 },
                    "default_when_running": { "type": "string", "enum": ["ask", "reject", "restart", "parallel"] },
                    "auto_update": { "type": "boolean" }
                }),
                &[],
            )
        },
    },
];

/// Finds a tool by name.
pub fn find(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_is_an_object() {
        for tool in TOOLS {
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
        }
        assert!(find("list_apps").unwrap().read_only);
        assert!(!find("run_action").unwrap().read_only);
    }
}
