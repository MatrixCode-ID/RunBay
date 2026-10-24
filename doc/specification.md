# RunBay — Specification

Status: **draft specification**, decisions as of 2026-10-24.
No code yet; this document is the reference for the implementation plan.

## 1. Background

Building and running applications for debugging is currently done through `.cmd`
shortcuts on the desktop. The problems:

- Process status and output are hard to follow: console windows pile up, get
  covered, or disappear once they finish.
- Every application needs several shortcuts, so the desktop fills up with icons.

RunBay is a single small application that registers those applications, runs
their actions (build, run, build+run, custom) as PowerShell scripts, and shows
and stores the output of every run.

## 2. Decisions

| Topic | Decision |
| --- | --- |
| Name | **RunBay** |
| Language / UI | **Rust + Slint**; design language **Material 3 Expressive** |
| Platform | Windows 10/11 x64 |
| License | MIT |
| Source and releases | Published on GitHub (`MatrixCode-ID/RunBay`); the executable is distributed through GitHub Releases |
| Versioning | SemVer, starting at **1.0.0-alpha.1**; stages **alpha → beta → release**, with **no update channels** |
| Auto update | Checks on startup and every 6 hours; downloads automatically, installs when the user agrees; can be turned off |
| PowerShell | `pwsh.exe` (7) by default; falls back to `powershell.exe` (5.1) when not found. Can be forced per app |
| Build+Run | Run is **cancelled** when the build ends with exit code ≠ 0 |
| Action while the app is still running | Set **per action** in the GUI: Ask / Reject / Stop and restart / Parallel. Default for a new action: **Ask** |
| History retention | Set **per action**: Off / Last N runs / Forever. Defaults: Build = 50, Build+Run = 50, Run = 5, Custom = 50 |
| System tray | Yes |
| Exiting while actions are running | **Ask**: stop all and exit, or cancel |
| Theme | Follows Windows; can be forced to Light/Dark |
| Load with Windows | Optional; on startup returns to the **last mode and position** |
| App icon | Image file **png / ico / svg**, copied into the data folder |
| App/action configuration | Working directory (per app, overridable per action), environment variables (per app, overridable per action), app order and groups |
| Notifications | **Badges** only (success/failed/running), no Windows toasts |
| Message icon (compact/icon) | **Per app**: output of that app's last run, live while still running |

## 3. Terms

- **App** — an application registered in RunBay to be built/run.
- **Action** — a command belonging to an app, backed by a PowerShell script.
- **Run** — one execution of an action; has output, exit code, duration, and status.
- **Data folder** — `./data/`, relative to the folder of the RunBay executable.

## 4. Data folder layout

RunBay's own metadata is kept apart from the data of registered apps.

```
<RunBay folder>/
  RunBay.exe
  data/
    meta/                         ← owned by RunBay itself
      settings.json               ← mode, theme, always on top, icon size, autostart, …
      window-state.json           ← position/size per mode, last mode
      groups.json                 ← groups and their order
    apps/                         ← registered apps
      <app-id>/
        app.json                  ← app definition + list of actions
        icon.<png|ico|svg>        ← icon asset (optional)
        actions/
          <action-id>.ps1         ← script of each action
        history/
          index.jsonl             ← one line per run (summary)
          <yyyy-MM>/
            <run-id>.log          ← full output of a run
```

- `app-id` and `action-id`: unique, stable slugs (e.g. `my-app`, `build`).
  Renaming the display name does not change the id.
- `run-id`: `yyyyMMdd-HHmmss-<action-id>-<4 hex>`, so ids sort by time.
- All JSON is written atomically (write to `*.tmp`, then rename) so files are not
  corrupted if the process dies abruptly.
- Removing an app deletes `apps/<app-id>/` including its history, after
  confirmation.

### 4.1 `app.json`

```json
{
  "id": "my-app",
  "name": "My App",
  "group": "work",
  "order": 10,
  "icon": "icon.png",
  "workingDirectory": "C:\\src\\my-app",
  "shell": "auto",
  "env": { "DOTNET_ENVIRONMENT": "Development" },
  "actions": [
    {
      "id": "build",
      "name": "Build",
      "kind": "build",
      "script": "actions/build.ps1",
      "workingDirectory": null,
      "env": {},
      "whenRunning": "ask",
      "retention": { "mode": "last", "count": 50 }
    },
    {
      "id": "run",
      "name": "Run",
      "kind": "run",
      "script": "actions/run.ps1",
      "whenRunning": "ask",
      "retention": { "mode": "last", "count": 5 }
    },
    {
      "id": "build-run",
      "name": "Build + Run",
      "kind": "buildRun",
      "whenRunning": "restart",
      "retention": { "mode": "last", "count": 50 }
    }
  ]
}
```

| Field | Values |
| --- | --- |
| `shell` | `auto` (pwsh → powershell), `pwsh`, `powershell` |
| `kind` | `build`, `run`, `buildRun`, `custom` |
| `whenRunning` | `ask`, `reject`, `restart`, `parallel` |
| `retention.mode` | `off`, `last` (with `count`), `forever` |
| `workingDirectory` / `env` on an action | `null`/empty = inherit from the app; action env overrides app keys with the same name |

- Every app has exactly one `build`, `run`, and `buildRun` action, plus zero or
  more `custom` actions. The `buildRun` action has no script of its own: it runs
  the `build` script, then `run` if the build succeeded.
- `build`/`run` may be left without a script; their buttons are then disabled.

### 4.2 `history/index.jsonl`

One JSON line per run, appended to the end of the file:

```json
{"runId":"20261024-101530-build-a3f1","actionId":"build","start":"2026-10-24T10:15:30+07:00","end":"2026-10-24T10:16:02+07:00","exitCode":0,"status":"success","log":"2026-10/20261024-101530-build-a3f1.log"}
```

`status`: `running`, `success`, `failed`, `stopped`, `skipped` (the run part of a
build+run cancelled because the build failed). On startup, `running` entries
without a live process are changed to `stopped`.

### 4.3 Retention

Retention is applied after each run finishes and on startup, per action:

- `off` — no log is written to disk. Output lives in memory only for the session
  and is still available as the "last run"; it does not appear in Action History
  after RunBay is closed.
- `last` + `count` — keep the last `count` runs of that action; older logs and
  index lines are deleted (the index is rewritten atomically).
- `forever` — no automatic cleanup.

A build+run is recorded as one run of the `buildRun` action (its log holds the
build and run sections in order, with section markers).

## 5. Running actions

### 5.1 Process

- Scripts run as
  `<shell> -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File <script>`
  with the working directory and environment merged from app + action.
- The environment variables `RUNBAY_APP_ID`, `RUNBAY_ACTION_ID`,
  `RUNBAY_RUN_ID`, and `RUNBAY_APP_DIR` (the `data/apps/<app-id>` folder) are added.
- stdout and stderr are read through pipes and merged in arrival order; stderr
  lines are flagged so they can be colored differently.
- Every run is placed in a **Windows Job Object**
  (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) so **Stop** ends the whole process tree
  (e.g. `dotnet build` with its children, or the application being run).
- No console window is shown (`CREATE_NO_WINDOW`).
- Output is decoded as UTF-8; RunBay sets `[Console]::OutputEncoding` to UTF-8
  through a startup argument so pwsh and 5.1 output is consistent.

### 5.2 Action while the app is still running (`whenRunning`)

Checked when an action is triggered while the same app still has an active run:

| Value | Behavior |
| --- | --- |
| `ask` | Dialog: *Stop and run*, *Run in parallel*, *Cancel* |
| `reject` | Not started; a short message says the app is still running |
| `restart` | The app's active runs are stopped, then the new action starts |
| `parallel` | Starts immediately alongside the active runs |

Different apps may always run in parallel.

### 5.3 On-screen output

- ANSI colors (basic SGR 16/256 colors, bold) are rendered; other escapes are dropped.
- The display buffer is limited to **10,000 lines per run** (ring buffer); old
  lines leave the screen, but the log on disk stays complete. The limit can be
  changed in settings.
- Auto-scroll follows the output; it pauses when the user scrolls up and resumes
  when scrolled back to the bottom.
- Output panel actions: Stop, Copy all, Open log file, Clear view.

### 5.4 Status badges

Each app shows a badge for its last run: running (animated), success, failed,
stopped. The same badge appears on the tray icon and the floating icon
(aggregated: running > failed > success).

## 6. Display modes

Modes are switched with a button in every mode and from the tray menu. The mode,
and the position/size of each mode's window, are remembered in
`window-state.json`. A position outside the monitors available at startup is
moved to the nearest monitor.

### 6.1 Full

A regular, resizable window.

```
┌──────────────┬───────────────────────────────────┬─────────────┐
│ Navigation   │ Output                            │ Action      │
│ [group]      │ ▸ My App · Build    ● running     │ History     │
│  ▸ App A  ●  │ ───────────────────────────────── │ (flyout,    │
│    Build Run │ 10:15:30 Restoring packages...    │  can be     │
│    B+R  ⋯    │ 10:15:41 Build succeeded.         │  hidden)    │
│  ▸ App B  ✓  │                                   │             │
│ + Add app    │ [Stop] [Copy] [Open log]          │             │
└──────────────┴───────────────────────────────────┴─────────────┘
```

- **Navigation**: apps grouped by group; they can be dragged to reorder and moved
  between groups. Each app shows its icon, name, badge, Build / Run / Build+Run
  buttons, and a custom action menu. Below the app, its active/last runs are
  listed; selecting one shows its output in the Output panel.
- **Output**: output of the selected run (live while running). Multiple active
  runs are shown as tabs.
- **Action History** (right flyout, opened with the *Action History* button):
  runs from every app's `history/index.jsonl`, newest first, filterable by
  app/action/status. Selecting an entry opens its log in the Output panel.
- App menu: Edit, Duplicate, Remove, Open working directory, Open app data folder.

### 6.2 Compact

A small resizable window (default about 320×420).

- List of apps (icon, name, badge) with Build / Run / Build+Run action icons
  and a custom action menu.
- **Message icon per app**: opens a popup with the output of that app's last
  run, live while running, with Stop and Open in Full buttons.
- Buttons: always on top (pin), switch mode.

### 6.3 Icon

- A frameless round floating icon that can be dragged; size **Small 40 px /
  Medium 56 px / Large 72 px** (setting).
- When released after dragging, the icon **snaps to the nearest screen edge**
  (like an Android chat bubble); its position is saved.
- Clicking opens a popup with app cards (icon, name, badge, action buttons,
  message icon) that behaves like Compact. The popup closes on an outside click
  or Esc.
- The aggregated badge is shown in a corner of the icon.
- Right-click: switch mode, always on top, icon size, exit.

### 6.4 Always on top

Available in Compact and Icon modes and stored per mode. Full mode does not use
always on top.

## 7. Managing apps and actions

App editor dialog (from Full; Compact/Icon open Full for this):

- Name, group, icon (pick a png/ico/svg file → copied to
  `data/apps/<app-id>/icon.<ext>`, replacing the previous icon), working
  directory (folder picker), shell, environment variables (key/value table).
- Action list: name, script, working directory/env override, `whenRunning`,
  retention. Custom actions can be added, reordered, and removed.
- A simple script editor (monospace, no highlighting) with an **Open in external
  editor** button (Windows `.ps1` association). Scripts are saved to
  `actions/<action-id>.ps1`.
- Validation: app names are unique; the working directory must exist when an
  action runs (otherwise the run fails with a clear message).

## 8. Settings

| Setting | Default |
| --- | --- |
| Theme | Follow Windows (Light / Dark / Follow Windows) |
| Load with Windows | Off |
| Floating icon size | Medium |
| Always on top (Compact / Icon) | Off / On |
| On-screen output line limit | 10,000 |
| Default retention for new actions | as in §2 |
| Default `whenRunning` for new actions | Ask |
| Auto update | On |

**Load with Windows** writes a `RunBay` value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` holding the executable path
with the `--autostart` argument; turning it off removes the value. With
`--autostart`, RunBay opens the last mode and position.

## 9. System tray

- Tray icon with the aggregated badge.
- Left click: show/focus the window of the active mode.
- Menu: Full / Compact / Icon, Always on top, Load with Windows,
  Check for updates (or *Restart to update* when an update is ready), Exit.
- Closing the window (× button) in Full/Compact **hides it to the tray** instead
  of exiting. Exiting is only through the Exit menu.
- Exiting with active runs → dialog: *Stop all and exit* / *Cancel*.

## 10. Single instance

Only one instance per user. Starting RunBay again focuses the window of the
running instance and exits.

## 11. Non-functional targets

| Aspect | Target |
| --- | --- |
| Startup until the window is shown | < 300 ms (cold start, SSD) |
| Idle memory (Icon/Compact mode) | < 30 MB working set |
| Memory in Full with 10,000 output lines | < 60 MB |
| Idle CPU | ~0%: no redraw or polling while there is no output |
| Distribution | A single executable, no installer or extra runtime; the `data/` folder is created automatically |
| Data writes | Atomic; a corrupted data file does not stop the application (that app is marked as errored) |

Process output is batched to the UI (e.g. every 50 ms) so heavy output does not
overload rendering.

## 12. Design language — Material 3 Expressive

- Based on Slint's built-in `material` style, extended with RunBay tokens: M3
  color roles (primary, secondary, tertiary, surface container, error) for
  Light/Dark, the M3 type scale, and **Expressive shapes** (large corners, main
  action buttons shaped as pills/rounded squares that morph when pressed).
- Expressive motion: short spring transitions for the Action History flyout, the
  icon popup, and badge changes; no animation while idle.
- UI icons: Material Symbols (Rounded) as a bundled font.
- Console output uses a monospace font (Cascadia Mono when available, falling
  back to Consolas).
- The "Made with Slint" attribution is shown in the About dialog, as required by
  Slint's royalty-free license.

## 13. Architecture (outline)

One application crate (`runbay`) with these modules:

| Module | Responsibility |
| --- | --- |
| `data` | Read/write `meta/`, `apps/`, history index, atomic writes, retention |
| `runner` | Spawn PowerShell, Job Object, pipe reading, run status, `whenRunning` rules |
| `ansi` | Parse SGR into colored segments |
| `ui` | Slint components (`.slint`) per mode, theme tokens, model bindings |
| `platform` | Tray, autostart registry, single instance, Windows theme detection, monitor positions |
| `update` | Release lookup, download, checksum verification, executable swap |

The runner works on its own thread; the UI only receives events through
`slint::invoke_from_event_loop`.

## 14. Versioning, releases, and auto update

### 14.1 Versioning

- SemVer 2.0. The first version is **`1.0.0-alpha.1`**.
- Stages run in order **alpha → beta → release**, e.g. `1.0.0-alpha.1`,
  `1.0.0-alpha.2`, `1.0.0-beta.1`, `1.0.0`. A later version starts again at alpha
  (`1.1.0-alpha.1`).
- The version lives in `Cargo.toml`, kept equal to the newest release note, and is
  embedded in the executable (shown in About and in the Windows file properties).
  The release build takes the version from the release note.
- **No channels**: there is a single update line. The updater offers the
  highest version published, whatever its stage. Users on a stable release will
  therefore also receive the next alpha/beta once it is published.

### 14.2 GitHub releases

- Each version is a Git tag `v<version>` (e.g. `v1.0.0-alpha.1`) with a GitHub
  Release of the same name. Alpha and beta releases are marked as
  *pre-release* on GitHub; this marking is informational only and does not
  affect the updater.
- Release assets:
  - `RunBay.exe` — the portable executable (x64).
  - `SHA256SUMS` — SHA-256 checksums of the assets, in `sha256sum` format.
- Every version needs a release note `doc/ReleaseNote/<version>.md`; its file
  name is the source of the version and its content is the body of the GitHub
  Release. Format and rules: [doc/ReleaseNote/README.md](ReleaseNote/README.md).
- Publishing is automatic: the [Release workflow](../.github/workflows/release.yml)
  runs on every push to `main` that changes `src/**`, `Cargo.toml`, or
  `Cargo.lock`. It releases the single release note that has no `v<version>` tag
  yet: builds `RunBay.exe` with that version, tags `v<version>`, and creates the
  release with its assets. Without an untagged release note nothing is published.
- Auto update needs the repository and its releases to be **public**; the
  updater does not use GitHub credentials.

### 14.3 Update check

- With auto update on, RunBay checks **on startup** and **every 6 hours** while
  running. *Check for updates* (tray menu and About/settings) checks immediately,
  even when auto update is off.
- The check calls the GitHub REST API
  `GET /repos/MatrixCode-ID/RunBay/releases` (the `latest` endpoint skips
  pre-releases, so it is not used), ignores drafts, parses the tag versions, and
  picks the highest one by SemVer precedence. It sends `If-None-Match` with the
  last `ETag` (stored in `data/meta/update.json`) to stay well inside the
  unauthenticated rate limit.
- A failed check (offline, API error, rate limit) is silent apart from a log
  entry; a manual check shows the error.

### 14.4 Download and install

1. When a newer version is found, `RunBay.exe` and `SHA256SUMS` are downloaded
   in the background to `data/meta/update/`.
2. The checksum of the downloaded executable must match `SHA256SUMS`; on a
   mismatch the download is discarded and reported.
3. RunBay shows an **Update ready** badge (Full header, tray icon, About) with the
   new version and a link to its release notes.
4. The update is installed only when the user chooses **Restart to update**.
   If actions are running, the user is asked first: *Stop all and update* /
   *Cancel*.
5. Install: the running `RunBay.exe` is renamed to `RunBay.exe.old`, the new
   executable is moved into place, and RunBay restarts in the same mode and
   position with `--updated`. On the next startup `RunBay.exe.old` is deleted.
6. If the new executable cannot be moved into place (e.g. no write access to the
   folder), the old executable is restored and the error is shown with the
   download location for a manual update.

The `data/` folder is never touched by an update. Changes to the data format
must stay readable by the new version (migrations run on startup) and are
listed in the release notes.

### 14.5 Settings

- **Auto update** (default on): turning it off stops the periodic and startup
  checks; manual checks remain available.
- A version the user dismisses with **Skip this version** is not offered again
  until a newer one is published.

## 15. Out of scope for the first version

- Windows toast notifications.
- Extracting icons from `.exe` files and built-in Material Symbols icons for apps.
- Syntax highlighting in the script editor.
- Interactive input to the stdin of a running process.
- Platforms other than Windows.
- Syncing or importing/exporting configuration between machines.

## 16. Open questions

None blocking. Defaults not specified by the user (icon sizes in px, the
10,000-line limit, the Compact window size, memory targets) are proposals and can
be changed while reviewing this specification.
