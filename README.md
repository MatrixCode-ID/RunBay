# RunBay

A small, fast Windows tool to build and run your applications from one place.

Register your apps, give each one Build / Run / Build+Run and custom actions
backed by PowerShell scripts, and watch their output live — no more piles of
console windows and desktop shortcuts. RunBay works as a full window, a compact
list, or a floating icon that stays on top.

> **Status:** alpha. See [doc/specification.md](doc/specification.md) for the full
> behavior and [Releases](https://github.com/MatrixCode-ID/RunBay/releases) for
> downloads.

## Features

- Apps with icons, groups, working directory, and environment variables.
- Build, Run, Build+Run, and custom actions as PowerShell scripts
  (`pwsh` 7, falling back to Windows PowerShell 5.1).
- Every run lives in a Windows Job Object: **Stop** ends the whole process tree.
- Live, colored output with tabs and a per-action run history with configurable
  retention (off / last N runs / forever).
- Three display modes: Full, Compact, and a draggable floating icon that snaps to
  the screen edge.
- Always on top, system tray, load with Windows, and remembered window positions.
- An MCP server for coding agents, with confirmation dialogs and Undo.
- Automatic updates from GitHub Releases, verified with SHA-256.
- Material 3 Expressive design, built with Rust and [Slint](https://slint.dev).

## Install

Paste this into PowerShell (no admin rights needed):

```powershell
irm https://raw.githubusercontent.com/MatrixCode-ID/RunBay/main/scripts/install/install.ps1 | iex
```

The script downloads `RunBay.exe` of the newest release (pre-releases included),
verifies it against `SHA256SUMS`, installs it in
`%LOCALAPPDATA%\Programs\RunBay\`, adds a Start menu shortcut, and starts RunBay.
Run it again to upgrade (exit RunBay first). Options: `-InstallDir <folder>`,
`-Version <version>`, `-NoShortcut`, and `-NoStart`, passed like this:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/MatrixCode-ID/RunBay/main/scripts/install/install.ps1))) -InstallDir 'D:\Tools\RunBay'
```

To install by hand instead:

1. Download `RunBay.exe` from the latest
   [release](https://github.com/MatrixCode-ID/RunBay/releases).
2. Put it in a folder you can write to, e.g. `C:\Tools\RunBay\`. RunBay creates
   its `data\` folder next to the executable; there is no installer.
3. Start it. Use **Settings → Load with Windows** to start it at sign-in.

RunBay then updates itself: it checks GitHub Releases on startup and every 6
hours, downloads and verifies a new version in the background, and installs it
when you choose **Restart to update**. Your `data\` folder is never touched.

## Usage

1. **Add app**: give it a name and a working directory.
2. In the editor, write the **Build** and **Run** scripts, for example:

   ```powershell
   # Build
   dotnet build -c Debug
   ```

   ```powershell
   # Run
   dotnet run --no-build
   ```

3. Use **Build**, **Run**, or **Build + Run** (the run is skipped when the build
   fails). Custom actions sit in the terminal menu of each app.
4. Choose per action what happens when the app is still running: ask, reject,
   stop and restart, or run in parallel.

The environment of every script includes `RUNBAY_APP_ID`, `RUNBAY_ACTION_ID`,
`RUNBAY_RUN_ID`, and `RUNBAY_APP_DIR` (the app's data folder).

## Use from coding agents (MCP)

RunBay is an MCP server over stdio. Point your agent at the executable with the
`--mcp` argument, for example in an MCP configuration:

```json
{
  "mcpServers": {
    "runbay": {
      "command": "C:\\Tools\\RunBay\\RunBay.exe",
      "args": ["--mcp"]
    }
  }
}
```

The bridge starts RunBay when it is not running. Agents can list apps, run
actions (optionally waiting for the result), read output, edit apps and actions,
and change settings. **Settings → MCP access** decides what they may do: *Off*,
*Ask first* (the default: every change is confirmed in RunBay), or
*Unrestricted*. Runs started by agents are marked in the window, and agent
changes can be undone from the snackbar.

## Development

- Windows 10/11, Rust stable (edition 2024), and the MSVC build tools with the
  Windows SDK.
- `cargo test` runs unit tests and integration tests with real PowerShell
  scripts, the MCP bridge against a headless instance, and the updater against a
  local HTTP server.
- Development options: `--data-dir <path>` uses another data folder (with its own
  pipe, so it never talks to your real instance), `--headless` serves the pipe
  without a window (tests), and `RUNBAY_UPDATE_API` replaces the GitHub API URL.
- Releases are published by the workflow in `.github/workflows/release.yml`; see
  [doc/ReleaseNote/README.md](doc/ReleaseNote/README.md).

## License

[MIT](LICENSE). Third-party licenses: [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
