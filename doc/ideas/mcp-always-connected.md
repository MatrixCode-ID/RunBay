# Keep agents connected to RunBay

- **Date:** 2026-10-24
- **Status:** discussion

## Context

The `--mcp` bridge is already lazy (specification §15.1): `tools/list` is answered
by the bridge itself, every tool call opens its own pipe connection, the GUI is
started when it is not running, and the session reconnects after the GUI goes
away. So once an agent has started the bridge, RunBay is reachable whenever a
tool is called, even if the GUI was closed in between.

The remaining failures happen outside that loop:

1. **Wrong executable path in the client configuration.** The MCP client starts
   the command it was given; if the file does not exist (moved install, old path,
   typo) the server fails for the whole agent session. This happened on a
   developer machine where the configuration pointed to a folder RunBay was never
   installed in.
2. **Bridge and GUI from different versions.** After an auto update the agent's
   bridge process still runs the old executable for the rest of its session; if
   the internal protocol version changed, every call is refused until the agent
   restarts its MCP servers.
3. **Access set to Off**, or *Ask first* while nobody is at the desk to confirm.

## Possible directions

- **Register with agents** in Settings > Agents: write or update the user-scope
  MCP entry for detected clients (Claude Code, Codex, Claude Desktop) with this
  executable's path, instead of only copying instructions. On start, RunBay could
  check those entries and offer to fix a path that no longer exists.
- A stable launcher path that survives reinstalls and updates (for example the
  installer's fixed folder), so client configurations never need to change.
- On a protocol mismatch the bridge could start the current `RunBay.exe --mcp`
  from the install folder and proxy to it, instead of failing until restart.
- A clearer error text in the bridge for each failure (missing GUI after the
  timeout, version mismatch, access Off), so agents can tell the user what to do.

## Decisions so far

- None yet.

## Open questions

- Should RunBay edit other applications' configuration files at all, or only
  offer to run their own CLI (`claude mcp add`, `codex mcp add`) with consent?
- Which clients are worth detecting automatically?
- Is a bridge-side proxy worth the complexity, given protocol changes are rare?
