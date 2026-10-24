# Plan: RunBay v1 — local Windows tool

- **Date:** 2026-10-24
- **Status:** not executed
- **Executor:** Claude Code
- **Specification:** [doc/specification.md](../../doc/specification.md) — the source of truth for
  behavior. This plan decides order, scope per milestone, tooling, and execution rules; it does
  not repeat the specification. When this plan and the specification disagree, the
  specification wins for behavior and this plan wins for process.
- **Related idea:** [RunBay Server](../../doc/ideas/remote-debugger.md) (not part of this plan).

## 1. Final decisions (user, 2026-10-24)

| Topic | Decision |
| --- | --- |
| Milestone order | M1 Core → M2 MCP stdio → M3 Full mode complete → M4 Compact & Icon → M5 Auto update → M6 Polish |
| Platform abstraction | **None.** Implement Windows directly; no traits prepared for Linux. Refactoring for Linux belongs to the RunBay Server work later. |
| Linux build | **Not a goal for v1.** The crate builds and is tested on Windows only; no `#[cfg(windows)]` gating to make it compile on Linux. Linux support starts with the RunBay Server work. Execute this plan on a Windows machine. |
| Commits | Commit on `work-bench` at the end of every milestone and push to `private`. No merge to `main` and no release, **except in M5** (below). |
| Tests | `cargo test` (unit + integration, including real PowerShell scripts) and a CI workflow running build, clippy, and tests on every push to `main` (and pull requests). Changed 2026-10-24: no CI on `work-bench`; checks there run locally. |
| Visual tests | **Allowed** during execution: the executor may start RunBay and use screenshots / computer use to check the UI. |
| Slint renderer | **femtovg** (OpenGL), with Slint's **software renderer** as fallback when OpenGL is unavailable. |
| RunBay logo | The executor designs a simple SVG logo in Material 3 style (replaceable later). |
| Auto update test (M5) | Make `MatrixCode-ID/RunBay` **public**, merge `work-bench` into `main`, release `1.0.0-alpha.1` and then `1.0.0-alpha.2` through the Release workflow, and test updating alpha.1 → alpha.2. |
| Version info in UI | The version is visible in Full and Compact mode outside About (specification §6.1, §6.2). Added 2026-10-24. |

## 2. Execution rules

- Follow [CLAUDE.md](../../CLAUDE.md): everything in the repository is English; commit messages in
  English; dependencies MIT-compatible (ask only if none is suitable — see §9 fallbacks first).
- Work on `work-bench`. Each milestone ends with: all tests green, `cargo clippy -- -D warnings`
  clean, `cargo fmt --check` clean, a commit, and a push to `private`.
- Do not stop to ask. When something is not covered, take the choice most consistent with the
  specification and this plan and record it in the execution report (§10).
- Version in `Cargo.toml` stays `1.0.0-alpha.1` until M5.
- Do not create `v*` tags by hand; the Release workflow does it.
- Never put private data in the repository (machine paths, names, credentials). Test fixtures use
  temp folders and placeholder paths.

## 3. Stack and dependencies

Exact versions are pinned when added (latest stable at execution time); check each license.

| Purpose | Crate | License |
| --- | --- | --- |
| UI | `slint` (+ `slint-build`), features: `backend-winit`, `renderer-femtovg`, `renderer-software` | Slint royalty-free (attribution in About) |
| Windows APIs (Job Object, pipes, registry, process, theme, monitors) | `windows` | MIT/Apache-2.0 |
| Tray | `tray-icon` | MIT/Apache-2.0 |
| JSON | `serde`, `serde_json` | MIT/Apache-2.0 |
| MCP server | `rmcp` (official Rust MCP SDK), stdio transport | MIT/Apache-2.0 |
| ANSI parsing | `vte` | MIT/Apache-2.0 |
| Versions | `semver` | MIT/Apache-2.0 |
| HTTP (update) | `ureq` with `rustls` | MIT/Apache-2.0 |
| Checksums | `sha2` | MIT/Apache-2.0 |
| Time | `time` or `chrono` | MIT/Apache-2.0 |
| Errors / logging | `anyhow`, `thiserror`, `log` + a small file logger | MIT/Apache-2.0 |
| Tests | `tempfile`, `assert_matches` (optional) | MIT/Apache-2.0 |

- Rust stable, edition 2024. One crate `runbay` (binary + `lib` for testability), executable
  `runbay.exe`, `windows_subsystem = "windows"` in release builds.
- The `--mcp` mode runs from the same GUI-subsystem executable; its stdio handles are inherited
  from the agent, so no console window appears.
- Icons: Material Symbols Rounded SVG files (Apache-2.0) under `ui/icons/`, only those used.
- `THIRD-PARTY-NOTICES.md` lists every bundled third-party license (Slint, Material Symbols,
  crates as required by their licenses).

## 4. Repository layout (target)

```
Cargo.toml
build.rs                 ← slint-build, Windows resources (icon, version info)
src/
  main.rs                ← argument parsing (--autostart, --mcp, --updated), startup
  lib.rs
  data/                  ← settings, window state, groups, apps, history index, atomic writes, retention
  runner/                ← shell resolution, process spawn, Job Object, pipes, run state, whenRunning
  ansi/                  ← SGR → styled segments
  ipc/                   ← named pipe server/client, versioned protocol
  mcp/                   ← stdio bridge, tool schemas
  platform/              ← tray, autostart, single instance, theme, monitors
  update/                ← release lookup, download, verify, swap
  ui/                    ← Rust side of the UI models and controllers
ui/
  *.slint                ← components per mode, theme tokens
  icons/                 ← Material Symbols SVG
  logo/                  ← RunBay logo (SVG + generated .ico)
tests/                   ← integration tests (runner with real pwsh scripts, data, MCP)
.github/workflows/ci.yml
THIRD-PARTY-NOTICES.md
```

## 5. Milestones

Each milestone lists its scope (specification sections), work, and done criteria.

### M1 — Core (spec §3–§5, §6.1 basic, §10, §11 partly)

Work:
1. Cargo project, `build.rs`, Slint setup with femtovg + software fallback, release profile
   (`lto`, `codegen-units = 1`, `strip`, `panic = "abort"` if compatible with error handling).
2. CI workflow `.github/workflows/ci.yml`: on push to `work-bench` and `main` and on pull requests;
   `windows-latest`; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test`. Must not conflict with `release.yml`.
3. `data`: folder layout (`data/meta`, `data/apps/<app-id>`), `app.json` model with defaults,
   atomic writes, slug ids, `run-id` format, `index.jsonl` append, startup fix of stale `running`
   entries, retention (`off` / `last` / `forever`). Corrupted files mark the app as errored.
4. `runner`: shell resolution (`auto` → pwsh → powershell), argument line from spec §5.1, merged
   working directory and env, `RUNBAY_*` variables, UTF-8 output, stdout/stderr pipes with
   ordering and stderr flag, Job Object kill on Stop, exit code, status, log file writing,
   `buildRun` (run skipped when build fails), `whenRunning` (`ask` surfaced to the caller as a
   decision request, `reject`, `restart`, `parallel`). Output batched to the UI every 50 ms.
5. `ansi`: SGR 16/256 colors and bold; other escapes dropped.
6. Single instance through the named pipe `\\.\pipe\RunBay-<user SID>` (ACL: current user only),
   activating the existing window.
7. Minimal Full mode: Navigation with apps and their Build / Run / Build+Run / custom buttons,
   Output panel with live colored output (ring buffer, auto-scroll, Stop, Copy, Open log), badges,
   **version label** at the bottom of Navigation. Apps can be added/edited through a minimal
   dialog (name, working directory, scripts) so the core is usable; the full editor comes in M3.
8. Theme tokens for Light/Dark (follow Windows), RunBay logo SVG and `.ico` embedded in the exe
   with Windows version info.

Done when: an app with real build/run scripts can be created, built, run, stopped (whole process
tree), and its history and retention behave as specified; tests cover `data`, retention,
`ansi`, shell resolution, runner (success, failure, stop kills child processes, buildRun skip,
whenRunning rules, env/working directory, UTF-8); CI green.

### M2 — MCP stdio (spec §15)

Work:
1. `ipc`: versioned request/response + event protocol over the pipe (JSON lines), version
   handshake with a clear error on mismatch.
2. `--mcp` bridge with `rmcp` over stdio: starts the GUI when absent (waits up to 10 s), forwards
   tools, passes `clientInfo.name` as the agent name.
3. Tools from spec §15.3 with their parameters: read tools, `run_action` (`wait`,
   `timeout_seconds`, `tail`, `if_running`), `wait_run`, `stop_run`, app/action edit tools,
   `set_app_icon`, `update_settings` (excluding MCP access). Progress notifications with output
   when a progress token is given.
4. MCP access setting (Off / Ask first / Unrestricted, default Ask first) and the confirmation
   dialog (Allow, Allow all from this agent until it disconnects, Deny; 2-minute timeout).
   `delete_app` is always confirmed.
5. Agent markers (spec §15.4): tertiary running state with `smart_toy` corner icon and press
   animation, "by <agent>" chip, `source`/`agent` in history, connection dot on tray/floating icon
   (tray arrives in M3; wire the state now), snackbar with Undo for agent changes.

Done when: an MCP client (integration test driving `runbay.exe --mcp` over stdio, plus a manual
check with Claude Code) can list apps, run a build with `wait: true` and get exit code and tail,
start a run asynchronously and read its output, edit an action script, and is refused when access
is Off; Ask first shows the dialog; GUI shows the agent markers.

### M3 — Full mode complete (spec §6.1, §7, §8, §9)

Work:
1. Full app/action editor (spec §7): groups, order (drag), icon file copy, shell, env table,
   per-action working directory/env, `whenRunning`, retention, custom actions add/reorder/remove,
   script editor with "Open in external editor".
2. Action History flyout with filters (app, action, status, source), opening logs in Output;
   multiple active runs as tabs.
3. Settings page (spec §8) including theme override, output line limit, defaults for new actions,
   MCP access.
4. Tray (spec §9): menu, left click, hide-to-tray on close, exit confirmation with active runs.
5. Load with Windows (`HKCU\...\Run`, `--autostart`), window state per mode, off-screen
   correction.
6. About dialog: version, Slint attribution, third-party notices.

Done when: every Full-mode behavior in the specification is reachable from the UI; tests cover
editor validation, retention changes, autostart registry write/remove (against a test key name),
window-state persistence.

### M4 — Compact & Icon modes (spec §6.2–§6.4)

Work:
1. Compact window: app list with action icons, per-app message icon popup with live output,
   Stop and Open in Full, pin (always on top), mode switch, **version footer** with update badge.
2. Floating icon: frameless round window, sizes 40/56/72 px, drag with edge snap, aggregated
   badge, click popup with app cards (closes on outside click/Esc), right-click menu.
3. Always on top per mode; remembered position/size per mode; mode switching from every mode and
   the tray.

Done when: all three modes work and restore their last position; visual check of each mode in
Light and Dark.

### M5 — Auto update and first public releases (spec §14)

Work:
1. `update` module: release list from the GitHub API with `ETag`, SemVer selection (no channels),
   background download to `data/meta/update/`, SHA-256 verification, *Update ready* badge (next
   to the version in Full/Compact, tray, About), *Restart to update* with the running-actions
   question, executable swap with `.old` and rollback, `--updated` restart, *Skip this version*,
   Auto update setting, periodic check every 6 hours, manual *Check for updates*.
2. Unit tests against a local HTTP server fixture (release list, assets, checksum mismatch,
   ETag not-modified, pre-release ordering).
3. Public release and real update test:
   1. Review the whole repository for private data (none expected).
   2. Make `MatrixCode-ID/RunBay` public (`gh repo edit --visibility public
      --accept-visibility-change-consequences`).
   3. Write `doc/ReleaseNote/1.0.0-alpha.1.md`, merge `work-bench` into `main`, push `main` to
      `origin`; confirm the Release workflow publishes `v1.0.0-alpha.1` with `RunBay.exe` and
      `SHA256SUMS`. Fix the workflow if it fails (on `work-bench`, merge again).
   4. Bump to `1.0.0-alpha.2` with its release note (a small real change is fine), merge and push
      again; confirm the release.
   5. Install alpha.1 in a temp folder, let it detect alpha.2, download, verify, and update;
      confirm it restarts as alpha.2 with its data intact.

Done when: both releases exist, the real update alpha.1 → alpha.2 succeeds, and the fixture tests
pass.

### M6 — Polish (spec §11, §12)

Work:
1. Measure against the non-functional targets: cold start to window < 300 ms, idle working set
   < 30 MB in Icon/Compact, < 60 MB in Full with 10,000 lines, ~0% idle CPU. Record the numbers
   and the method in the report; fix regressions found.
2. Material 3 Expressive pass: color roles, type scale, Expressive shapes and spring motion
   (flyout, popup, badges), filled/outlined icon states; check Light/Dark visually.
3. Error handling review: corrupted data, missing working directory, pwsh missing, pipe failures,
   update failures — clear messages, no crashes.
4. README: usage, MCP setup for agents (`"command": "...RunBay.exe", "args": ["--mcp"]`),
   screenshots.

Done when: targets met or deviations documented with reasons; all modes reviewed visually.
No release in M6 (later releases happen on the user's request).

## 6. CI and release interaction

- `ci.yml` runs on `main` (public repository) and pull requests only; `work-bench` is checked
  locally (user decision 2026-10-24).
- `release.yml` stays as is: it only publishes when a push to `main` changes `src/**`,
  `Cargo.toml`, or `Cargo.lock` and exactly one release note is untagged.

## 7. Testing approach

- Unit tests next to the modules; integration tests in `tests/` that write real `.ps1` scripts to
  a temp folder and run them through the runner (needs `pwsh` or Windows PowerShell on the CI
  runner — both exist on `windows-latest`).
- Stop/kill test: a script that starts a child process; after Stop no process of the tree remains.
- MCP integration test: spawn `runbay.exe --mcp` against a GUI instance started with a temporary
  data folder (a hidden test switch such as `--data-dir <path>` may be added; keep it documented
  as a development option).
- Visual checks (allowed): start RunBay, check each mode in Light and Dark, the agent markers, the
  confirmation dialog, and the version label/footer; save screenshots outside the repository.

## 8. Specification upkeep

Any behavior decided during execution that the specification does not cover is added to the
specification in the same commit, and listed in the report.

## 9. Fallbacks decided in advance

| Problem | Fallback |
| --- | --- |
| Slint transparent/frameless round window misbehaves on Windows | Keep the window frameless and shape it with a Win32 window region (`SetWindowRgn`), or draw a round icon on a tiny square window |
| femtovg/OpenGL unavailable | Software renderer (already decided) |
| `tray-icon` conflicts with the Slint event loop | Implement the tray with `Shell_NotifyIcon` through `windows` on a hidden message window |
| `rmcp` does not fit (API or license) | Implement the needed MCP subset (initialize, tools/list, tools/call, progress) over stdio JSON-RPC by hand |
| Slint lacks a needed widget (e.g. tabs styling, drag reorder) | Build it from Slint primitives in `ui/` |
| A target in M6 cannot be met | Document the measured value, the cause, and options in the report; do not block |
| Release workflow fails in M5 | Fix on `work-bench`, merge to `main` again; never tag by hand |
| No suitable MIT-compatible crate for a need | Write the code in the project |

## 10. Execution report

After execution, append a section **Execution report** to this file (then move it to
`plan/executed/`): what was done per milestone with commit hashes, test and CI results, measured
performance numbers, visual checks done, choices made where the plan was silent, deviations, and
remaining work.

## Execution report

- **Date:** 2026-10-24
- **Executor:** Claude Code
- **State:** M1–M4 done; M5 code done and tested locally, the public releases are **not**
  done yet; M6 partly done. The plan stays in `plan/unexecuted/` until M5 and M6 finish.

### Per milestone

| Milestone | Commit(s) on `work-bench` | Result |
| --- | --- | --- |
| M1 Core | `a01da61` | Data folder, runner (Job Object, UTF-8, stdout/stderr, buildRun, whenRunning), ANSI, single instance pipe, minimal Full mode, logo, icons, CI, notices |
| M2 MCP stdio | `0e53abf` | Versioned pipe protocol, `--mcp` bridge on `rmcp`, all tools of §15.3, access setting and confirmation dialog, agent markers, Undo, `--headless` |
| M3 Full mode | `598baec` | Full editor, Action History flyout, settings page, tray, hide to tray, exit confirmation, load with Windows, window state, About |
| M4 Compact & Icon | `b732559` | Compact window, floating icon with native drag and edge snap, icon popup, per-mode always on top and positions, Light/Dark |
| M5 Auto update | `c85f62d`, `c21885c` | Update module, badges, Restart to update, Skip, fixture tests, release note `1.0.0-alpha.1`; releases pending (see below) |
| M6 Polish | — (README uncommitted at report time, committed with this report) | README; performance measured once; memory work moved to its own plan |

### Tests and CI

- Local: 48 unit tests, 14 runner integration tests (real PowerShell), 3 MCP
  integration tests (headless instance + bridge over stdio), 2 updater fixture tests;
  all green. `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean.
- CI (`ci.yml`, private repository): the M1 run failed on one test (8.3 short temp
  paths on the runner); fixed in M2. Later runs were cancelled by newer pushes
  (`cancel-in-progress`); the run for `c85f62d` was still in progress when this report
  was written. A green CI run is still to be confirmed before merging to `main`.

### Visual checks (screenshots outside the repository)

Full mode (dark, light), live colored output with tabs and auto-scroll, editor,
settings, Action History, agent confirmation dialog and agent markers, Compact mode
with the output popup, floating icon with badge, popup, right-click menu, drag and
edge snap, hide to tray and re-activation by a second start. Local update test:
alpha.1 (debug build) detected alpha.2 from a local server (`RUNBAY_UPDATE_API`),
downloaded and verified it, showed *Update ready*, and *Restart to update* restarted
as alpha.2 in the same position with `RunBay.exe.old` removed.

### Performance (release build, 2026-10-24)

| Target | Measured | Status |
| --- | --- | --- |
| Startup < 300 ms | 383 ms first start, ~30 ms afterwards (process start until the window exists) | met when warm; first start slightly above |
| Idle CPU ~0% | 0 ms CPU over 10 s idle | met |
| Idle memory (Full) < 60 MB | 171.6 MB working set (femtovg) | **not met** — moved to [memory-footprint plan](memory-footprint.md) on the user's instruction (no memory optimization in this execution) |
| Compact/Icon < 30 MB | not measured | see the memory plan |

Release executable size: 27 MB.

### Choices where the plan was silent

- Scripts run through a `-Command` bootstrap instead of `-File`, so UTF-8 output can
  be set before the script starts (spec §5.1 updated).
- A run lasts until its whole process tree has exited (spec §5.1 updated).
- Empty working directory falls back to the app data folder; logs are stored without
  ANSI escapes; scripts are saved as UTF-8 with BOM (spec updated).
- A failed build in Build + Run records status `failed`; `skipped` stays reserved.
- `wait_run` is never confirmed (it changes nothing).
- Agent removals go to `data/meta/trash/` so Undo works; emptied on the next start.
- `--data-dir` gets its own pipe name; `--headless` for tests; `RUNBAY_UPDATE_API`
  for update tests (spec updated).
- Dialogs (ask, agent confirmations) bring up the Full window while Compact/Icon mode
  is active.
- The Icon popup, right-click menu, and Icon snackbar are a separate frameless window,
  because Slint popups are clipped to the 56 px icon window (fallback of §9).
- `panic = "abort"` was not used in the release profile (a panic in a reader thread
  must not end the app).
- Local builds need the VS 2022 MSVC environment on this machine (the VS 18 install
  lacks the x64 CRT libraries); CI is unaffected.

### Deviations

- Drag and drop reordering of apps is replaced by *Move up / Move down* and the group
  field in the editor; recorded in [doc/ideas/drag-reorder.md](../../doc/ideas/drag-reorder.md).
- App menu has no *Duplicate*.
- The repository `MatrixCode-ID/RunBay` was already made public by the user before
  M5, so step M5 §3.2 (`gh repo edit --visibility public`) was skipped. Because it is
  public, the private-data review (§3.1) is repeated before every push of `main` to
  `origin`; `work-bench` is pushed only to `private`. The review before this report
  found no private data.
- No manual check with Claude Code as an MCP client was done (the protocol is covered
  by the integration test).

### Remaining work

1. M5 §3.3–3.5 (postponed by the user, 2026-10-24: no merge to `main` yet; CI runs only
   on `main`, so run fmt, clippy, and tests locally first): merge `work-bench` into `main`, push `main` to
   `origin`, confirm the `v1.0.0-alpha.1` release with `RunBay.exe` and `SHA256SUMS`;
   bump to `1.0.0-alpha.2` with its release note, release again, and test the real
   update alpha.1 → alpha.2 from GitHub.
2. M6: Material 3 Expressive pass (e.g. monospace font in the script editor, the
   pulsing agent button, native title bar color in light mode), error handling review,
   README screenshots.
3. Memory footprint: [plan/unexecuted/memory-footprint.md](memory-footprint.md)
   (proposed decisions need the user's confirmation).
