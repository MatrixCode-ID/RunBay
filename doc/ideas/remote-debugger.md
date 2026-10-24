# RunBay Server: remote run and capture

- **Date:** 2026-10-24
- **Status:** discussion
- **Plan:** none yet

This is an idea for later. It is not part of the first version and nothing here
is to be implemented now.

## Idea

Turn RunBay into a remote debugger: run an app's actions on another machine and
capture images or video of the application running there.

A separate application, **RunBay Server**, runs as a **Windows service** on the
remote host. RunBay (the desktop app) and its MCP bridge connect to it and use it
the same way they use the local runner.

```
RunBay (local)  ──authenticated, encrypted connection──▶  RunBay Server (remote host, service)
  GUI + MCP                                                runner + capture
```

## Goal

Develop Windows applications from a **Linux** workstation. Building and
debugging a Windows application from Linux is hard today. With RunBay Server on
a Windows host:

- RunBay runs on Linux and connects to RunBay Server on the Windows host.
- Builds, runs, output, and captures happen on the Windows host.
- Coding agents on Linux talk to the local RunBay (MCP) to build, run, and debug
  the Windows application on the server.

```
Linux workstation                              Windows host
┌───────────────────────────┐                  ┌──────────────────────────┐
│ Agent ── MCP ── RunBay    │ ── network ───▶  │ RunBay Server (service)  │
│ (source code lives here)  │                  │ build · run · capture    │
└───────────────────────────┘                  └──────────────────────────┘
```

## Decisions so far

- 2026-10-24: Idea only; not executed now. (user)
- 2026-10-24: The remote side is a separate application named **RunBay Server**,
  running as a service on the remote host. (user)
- 2026-10-24: **Replaces the decision above on how it runs:** RunBay Server is
  an **application started when a user logs on**, not a Windows service. It only
  serves while that user is logged on. (user)
- 2026-10-24: RunBay Server is a **separate executable** from RunBay. (user)
- 2026-10-24: Its users are almost all Windows developers. It is a development
  tool only and must not be used to run applications for end users or in
  non-development environments; security hardening is therefore not a main
  concern. (user)
- 2026-10-24: **First step:** RunBay Server runs the app's actions (build, run,
  debug runs) on the Windows host for RunBay on Linux, and the application's UI
  is reached with an ordinary **VNC** remote desktop. The RDP server with
  per-window RAIL stays a later step. (user)
- 2026-10-24: **Agent debugging goal:** when an agent on Linux debugs an
  application running on the server and something goes wrong after an action in
  the GUI (e.g. a button pressed through VNC), the agent must be able to find out
  **where in the source to debug**. (user)
- 2026-10-24: The debug timeline targets **.NET WPF and WinForms** first, and is
  part of the **first step**, together with remote runs and VNC. (user)
- 2026-10-24: The source reaches the Windows host in one of three ways, chosen
  per app: **Git**, a **shared folder**, or a **folder already prepared on the
  server**. (user)
- 2026-10-24: Purpose: develop Windows applications from Linux. RunBay runs on
  Linux, connects to RunBay Server on a Windows host, and agents use RunBay to
  build and debug there. (user)
- 2026-10-24: This is the **main goal** of RunBay; the local Windows tool in the
  v1 specification is the first step toward it. (user)
- 2026-10-24: The v1 specification does not change for now; this stays an early
  idea. (user)

- 2026-10-24: RunBay Server must work on **every Windows edition**, including
  Home; it must not depend on the built-in Windows RDP host (Pro/Server only) or
  on RDS licensing. (user)
- 2026-10-24: Showing only the remote application's UI on Linux is wanted. The
  Linux side may use **FreeRDP** code as the client (Apache-2.0, compatible with
  MIT; keep its license and NOTICE). (user)

## Remote application UI on Linux (agent proposal, not decided)

- Windows' own RemoteApp (RAIL) needs Windows Pro/Server as RDP host and, for
  official RemoteApp, Windows Server with RDS CALs, so it cannot be the base.
- Proposal: **RunBay Server acts as its own RDP server.** It captures the
  application's windows with Windows Graphics Capture, encodes them with the GPU
  encoder, and serves them over the **standard RDP protocol**, including RAIL so
  each remote window shows as a native Linux window. Input from the client is
  injected into the user's session. This works on every edition.
- The protocol stays **standard RDP**, documented in Microsoft's Open
  Specifications (e.g. MS-RDPBCGR core, MS-RDPERP RemoteApp, MS-RDPEGFX
  graphics), so unmodified FreeRDP clients work. RunBay-specific data (run
  control, output, captures) travels in a **custom dynamic virtual channel**,
  which RDP supports for extensions; RunBay's client loads a channel plugin.
- Building blocks: **IronRDP** (Rust, MIT/Apache-2.0, client and server crates;
  RAIL support to be verified) or FreeRDP's server libraries (C, Apache-2.0).
- Built-in RemoteApp on a Pro/Server host remains usable as an alternative,
  since the client side speaks the same protocol.
- Hardest parts: RAIL on the server side (mapping windows, popups, menus,
  z-order), input injection, and the user session staying logged in and
  unlocked (auto-logon, virtual display driver for headless hosts).

## First step with VNC (agent notes, not decided)

- RunBay Server starts actions in the logged-on user's session, which is the
  console session; a VNC server on the host shares exactly that session, so the
  application started by RunBay Server is visible and usable through VNC.
- VNC works on every Windows edition, so the "every edition" decision holds.
- The VNC server and viewer are **separate, existing tools** run next to RunBay,
  not part of its code: e.g. TightVNC/UltraVNC/TigerVNC on Windows and TigerVNC
  viewer or Remmina on Linux. Most of them are GPL, which is fine as long as
  RunBay only launches them as separate programs and never links or copies
  their code.
- Optional convenience later: RunBay on Linux opens the VNC viewer for the
  server's host with one click.
- VNC shows the whole desktop, not single windows, and is not encrypted by
  default; on a development LAN that is acceptable, otherwise tunnel it (e.g.
  SSH).
- What the first step needs from RunBay Server: connection from RunBay (Linux),
  app/action definitions, starting/stopping runs, streaming output and status,
  history; and getting the source onto the host (still open).

## From a GUI problem to a source location (agent proposal, not decided)

A **debug run** on RunBay Server collects, on one timeline, what the user did,
what the application did, and where in the code it went wrong. Agents read it
through MCP tools.

1. **UI action trail** — RunBay Server records UI Automation events of the
   application's windows: which element was invoked (control type, `Name`,
   `AutomationId`, window), focus changes, and new windows/dialogs, with a
   timestamp. An agent searches the source for the `AutomationId`/`Name` (e.g.
   in XAML) to find the handler or command behind the button. Works for any
   UI technology that exposes UI Automation (WPF, WinForms, WinUI, most others).
2. **Screenshots at events** — a capture of the window right after each invoked
   action and on every error, so the agent sees what the user saw.
3. **Exceptions with source locations** — the run is started under a debugger
   that reports first-chance and unhandled exceptions with stack traces. For
   .NET, `netcoredbg` (MIT) through the Debug Adapter Protocol; other runtimes
   can use their own DAP adapters (e.g. `lldb-dap` for native code). Output
   lines and logs stay on the same timeline.
4. **Paths that exist on Linux** — the build maps source paths to the Linux
   checkout (for .NET, `PathMap` in the build), so stack traces and PDBs point
   to files the agent can open directly. Without it, RunBay maps the paths when
   showing them.
5. **Crash dumps** — on a crash, a minidump is written (Windows Error Reporting
   local dumps) and attached to the run.
6. **Interactive debugging (later)** — MCP tools that pass DAP requests through:
   set breakpoints, continue, step, read the stack and variables, so the agent
   can reproduce the action and stop at the suspected code.

Possible MCP tools: `get_timeline(run_id)`, `get_exceptions(run_id)` (stack
with mapped paths), `get_ui_events(run_id)`, `capture_window(run_id)`, and later
`debug_*` tools for breakpoints and stepping.

Example: the user presses *Save* through VNC → the timeline shows
`Button "Save" (AutomationId=SaveButton)` at 10:15:41, a screenshot, and a
`NullReferenceException` at `src/Orders/OrderEditorViewModel.cs:212` 80 ms later
→ the agent opens that file and the handler bound to `SaveButton`.

## Source modes (agent notes, not decided)

| Mode | How | Notes |
| --- | --- | --- |
| **Git** | Before a build, RunBay Server fetches and checks out the branch/commit the Linux side is on, into its own working folder | Needs the commit to be reachable by the server (pushed to a shared remote). Uncommitted work could travel as a patch of the working tree applied after checkout. |
| **Shared folder** | The server builds directly from a folder shared between Linux and Windows (e.g. a Samba share of the Linux checkout, or a Windows share mounted on Linux) | No copy step; edits are visible at once. Building over the network is slower and file watching/locking can misbehave, so build output (`bin/`, `obj/`) should go to a local folder on the server. |
| **Prepared folder** | The app points to a folder that already exists on the server; the user keeps it up to date | RunBay does nothing to the source. |

- For every mode the app definition holds the **path mapping** between the
  server folder and the Linux checkout, used for `PathMap` in .NET builds and
  for mapping stack traces, so the agent always gets Linux paths.
- Git and shared folder can show, per run, which commit (and whether there were
  local changes) was built; the run history records it.

## Running at logon instead of as a service (agent notes, not decided)

- Gains: the server and every app it starts live in the interactive user's
  session, so the session 0 problem disappears (no launching into another
  session with the user's token); capture and input work directly; no admin
  rights are needed to install (start at logon through the user's `Run` key,
  like RunBay's own autostart); simpler to build and debug.
- Costs: nothing is served before logon or after logoff, so a dedicated host
  needs auto-logon; the screen must not lock (input is blocked on the lock
  screen); a crash is not restarted by the service manager (a small watchdog or
  a logon scheduled task with restart can cover it); builds that need admin
  rights need the server started elevated (e.g. a logon scheduled task with
  highest privileges).
- A headless host still needs a display to render on (virtual display driver).

## Notes from the discussion (agent, not decided)

- **Reuse:** the specification already separates who issues commands (GUI, MCP
  bridge) from who executes them (the runner) through a versioned internal
  protocol. Remote use sends the same protocol over the network.
- **Remote run:** scripts, live output, history, `whenRunning`, and Stop through
  a Job Object work the same; apps need to know their host, and the working
  directory becomes per host.
- **Capture images:** Windows Graphics Capture can capture the window of a given
  process, including GPU content. A Rust wrapper under MIT exists
  (`windows-capture`).
- **Capture video:** encode frames from the same API to H.264 with the hardware
  encoder through Media Foundation, stored as MP4 attachments of the run.
- **Agents:** MCP tools can return images, so a tool such as
  `capture_window(run_id)` lets an agent check the UI it built. This is useful
  locally too, before any remote support.
- **Service and session 0** (superseded by running at logon): a Windows service
  runs in session 0, which has no desktop, so applications started there cannot
  be seen or captured; a service would have to start actions in the interactive
  user's session through the user's token.
- **Locked screen / minimized RDP:** rendering may stop, so capture can be empty
  or frozen.
- **Capture border:** Windows 10 draws a yellow border around captured windows;
  Windows 11 can turn it off.
- **Security** (scaled down by the development-only decision): running
  PowerShell remotely is remote code execution by design. Because the project is
  open source and others may expose a server by mistake, a light baseline is
  still suggested: a pairing key instead of an open port, encryption (RDP's
  TLS), and a clear "development use only" notice in the README and the server
  UI.
- **Footprint:** network and video encoding stay out of the desktop app's idle
  path, so RunBay keeps its memory targets.
- **Keeping the door open in v1 (no scope change):** keep the internal protocol
  independent of the transport, and leave room in the run/history format for
  attachments (images, video) and a host.
- **Possible order** (updated by the first-step decisions): (1) RunBay Server
  for remote run and output, UI through VNC, and the debug timeline for .NET
  WPF/WinForms (UI action trail, screenshots at events, exceptions through
  `netcoredbg` with mapped paths, crash dumps) with its MCP tools; (2)
  interactive debugging through MCP (breakpoints, stepping); (3) RDP server with
  per-window RAIL.

- **RunBay on Linux:** the goal needs the desktop app (and its MCP bridge) on
  Linux. Slint and Rust are cross-platform, but the v1 specification is
  Windows-only in several places: Job Object, PowerShell as the action shell,
  tray, autostart through the registry, named pipes, and theme detection. On
  Linux the desktop app mainly acts as a client of the server; local actions
  there would need `bash`/`pwsh` and process groups instead of Job Objects.
- **Getting the source to the Windows host:** the code lives on Linux, the build
  runs on Windows. Options: the server pulls a Git branch/commit before the
  build; RunBay syncs the working tree (incremental, ignoring `bin/obj/target`);
  or a shared folder. Sync speed decides how pleasant the loop is.
- **Debugging beyond logs and screenshots:** attaching a real debugger remotely,
  e.g. through the Debug Adapter Protocol (`netcoredbg`, MIT, for .NET) tunneled
  through the server connection, so an editor or agent on Linux can set
  breakpoints in the Windows process.
- **Interacting with the remote UI:** for agents to test a UI, capture alone may
  not be enough; sending clicks and keys to the captured window (UI Automation
  or input injection in the user's session) is a possible later step.

## Open questions


- Should v1 already keep the platform-specific parts behind an abstraction so
  RunBay can later run on Linux, or is Linux support added together with RunBay
  Server?
- In Git mode, should uncommitted changes on Linux be carried to the server (as a
  patch), or only committed and pushed work?
- Is a remote debugger attach (DAP) part of the goal, or are build/run/output and
  capture enough?
- Should agents be able to send input (clicks, keys) to the remote application?
- How are hosts paired and trusted, and who may connect?
- Where do app definitions and scripts live: pushed from the desktop app, or
  stored on the server?
- Is live viewing (stream) needed, or are screenshots and recordings enough?
- Does RunBay Server ship in the same repository and release as RunBay?
