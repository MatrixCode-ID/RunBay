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
- **Service and session 0:** a Windows service runs in session 0, which has no
  desktop, so applications started there cannot be seen or captured. RunBay
  Server must start the actions in the interactive user's session (e.g. through
  the logged-on user's token) and capture there.
- **Locked screen / minimized RDP:** rendering may stop, so capture can be empty
  or frozen.
- **Capture border:** Windows 10 draws a yellow border around captured windows;
  Windows 11 can turn it off.
- **Security:** running PowerShell remotely is remote code execution by design.
  Needs device pairing with keys (no passwords), encryption, an allowlist of
  machines, an access mode like *Ask first* on the server side, and an audit log.
- **Footprint:** network and video encoding stay out of the desktop app's idle
  path, so RunBay keeps its memory targets.
- **Keeping the door open in v1 (no scope change):** keep the internal protocol
  independent of the transport, and leave room in the run/history format for
  attachments (images, video) and a host.
- **Possible order:** (1) local window capture + MCP tool, (2) RunBay Server for
  remote run and output, (3) video and live stream.

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
- How does the source reach the Windows host (Git pull, sync, shared folder)?
- Is a remote debugger attach (DAP) part of the goal, or are build/run/output and
  capture enough?
- Should agents be able to send input (clicks, keys) to the remote application?
- How are hosts paired and trusted, and who may connect?
- Where do app definitions and scripts live: pushed from the desktop app, or
  stored on the server?
- Is live viewing (stream) needed, or are screenshots and recordings enough?
- Does RunBay Server ship in the same repository and release as RunBay?
