# Plan: Custom window chrome for Full and Compact

- **Date:** 2026-10-24
- **Status:** executed 2026-10-24 — automated results and outstanding real-desktop
  checks are recorded in §6 below.
- **Idea:** [doc/ideas/custom-window-chrome.md](../../doc/ideas/custom-window-chrome.md)
- **Specification:** §6.1 (Full), §6.2 (Compact), §8 (taskbar icon settings),
  §9 (closing to the tray), §12 (design language).

## 1. Goal

Full and Compact draw their own title bar in Slint, while keeping the native window
frame behavior (shadow, rounded corners, resize, Aero Snap, Snap Layouts, system
menu). The window looks and behaves the same whether its taskbar button is shown or
hidden: the `WS_EX_TOOLWINDOW` caption no longer appears, and window managers that
skip tool windows (for example zone-based window layout tools that snap a window on
Shift+drag) keep treating Full and Compact as regular windows.

## 2. Final decisions

| Topic | Decision |
| --- | --- |
| Windows | Full and Compact. Icon and Icon popup are already `no-frame` and stay unchanged. |
| When | Always, regardless of the taskbar icon settings. |
| Technique | Subclass the window (`SetWindowSubclass`) on top of winit's window procedure. `WM_NCCALCSIZE` removes only the caption: call `DefSubclassProc`, then restore the original top edge, so the side/bottom frame, shadow, and resize stay native. When maximized, the top is inset by the frame thickness so content is not cut off. `WM_NCHITTEST` maps the Slint title bar to `HTCAPTION`, the top resize strip to `HTTOP`/`HTTOPLEFT`/`HTTOPRIGHT`, and the Full maximize button to `HTMAXBUTTON` (Snap Layouts). Everything else is `HTCLIENT`. |
| Full title bar | A thin bar (36 px) across the whole window above the existing three-panel layout: logo (18 px) and "RunBay" on the left, the rest is drag area, Minimize / Maximize (Restore when maximized) / Close on the right, 46 × 36 px each. |
| Compact title bar | The existing header row becomes the title bar. Minimize and Close are added after the Full-mode button, in the same 32 px `SymbolButton` style. No Maximize button; the Compact window drops `WS_MAXIMIZEBOX` so double-click and Win+Up do not maximize it. |
| Drag area | Every part of the title row that is not an interactive control (logo, title text, empty space). The agent indicator in Compact stays `HTCLIENT` so its tooltip keeps working. |
| Icons | Material Symbols Rounded only (repository rule): `minimize`, `crop_square` (maximize), `filter_none` (restore), and the existing `close`. Add only these SVGs, taken from the official Material Symbols Rounded source (Apache-2.0, already listed in `THIRD-PARTY-NOTICES.md`). |
| Colors | Theme tokens: buttons use `on-surface-variant`, hover uses the existing state layer of `SymbolButton`/the Full caption buttons; Close hover uses `error` / `on-error`. Title and buttons are dimmed (`on-surface-variant` at reduced opacity) while the window is inactive (`WM_ACTIVATE`). |
| Close (×) | Posts `WM_CLOSE` to the window, so the existing `on_close_requested` handlers run unchanged (Full and Compact hide to the tray, §9). |
| Minimize | Taskbar button shown → `SW_MINIMIZE`. Taskbar button hidden → hide the window to the tray, same as ×. The same rule applies to `SC_MINIMIZE` from the system menu and Win+Down (`WM_SYSCOMMAND`). |
| Maximize (Full) | Clicks arrive as non-client messages because of `HTMAXBUTTON`: handle `WM_NCLBUTTONDOWN`/`WM_NCLBUTTONUP` on it to toggle `SW_MAXIMIZE`/`SW_RESTORE`, and push hover/pressed state to Slint (`WM_NCMOUSEMOVE`, `WM_NCMOUSELEAVE`). Double-click on the drag area toggles maximize (native `HTCAPTION` behavior). |
| Rounded corners | Set `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND` explicitly, so the corners are round on every Windows 11 build regardless of the window's styles. |
| Taskbar setting (user decision 2026-10-24) | Full and Compact **no longer use `WS_EX_TOOLWINDOW`**. A hidden taskbar button is done with a **hidden owner window**: Windows creates no taskbar button for an owned window, and tools that skip tool windows or windows with a *visible* owner still accept it (FancyZones Shift+drag keeps working). `set_taskbar_button(hwnd, visible)` sets `GWLP_HWNDPARENT` to the shared hidden owner and clears `WS_EX_APPWINDOW` (which would force a button) when hiding; when showing, it sets the owner back to none and restores `WS_EX_APPWINDOW`; it always clears `WS_EX_TOOLWINDOW` (cleans up a style set by older versions). The existing hide/show-without-activation step stays, because the taskbar only reads the change when the window appears. `set_tool_window` for the Icon window and Icon popup is unchanged. |
| Hidden owner | One per process, created on the UI thread on first use and kept until exit: a top-level window of its own registered class, `WS_POPUP`, `WS_EX_TOOLWINDOW`, zero size, **never shown**. It must not be a message-only window (`HWND_MESSAGE`), because those cannot own top-level windows. It is never minimized or hidden in a way that would affect its owned windows. |
| Accessibility | Caption buttons have `accessible-role: button` and labels "Minimize", "Maximize"/"Restore", "Close". |
| Commit | Commit on `work-bench` at the end, English message; no push. The RunBay checkout was on `main` when this plan was written: `git switch work-bench` first (stop and report if it fails or `work-bench` is behind `main`). |

## 3. Steps

1. **Read first**: root `CLAUDE.md`, specification §§6, 8, 9, 12, and the current
   `src/platform/mod.rs`, `src/ui/modes.rs`, `src/ui/lifecycle.rs`, `ui/full.slint`,
   `ui/compact.slint`, `ui/components.slint`, `ui/theme.slint`.
2. **Icons**: add `minimize.svg`, `crop_square.svg`, `filter_none.svg` to `ui/icons/`
   and their entries in `Icons` (`ui/theme.slint`).
3. **Slint title bars**:
   - `components.slint`: a `CaptionButton` (46 × 36 px, icon 16 px, `danger` flag for
     Close, `hover`/`pressed` input properties for the maximize button whose mouse
     input comes from Win32) and a `TitleBar` for Full.
   - Each window exposes to Rust what hit testing needs, in logical pixels:
     `out property <length> caption-height`, `out property <length> drag-end`
     (x where the drag area ends; everything right of it is controls), and for Full
     `out property <length> max-button-x` / `max-button-width`.
   - Input properties `window-active`, `maximized`, `max-hover`, `max-pressed`;
     callbacks `caption-minimize()`, `caption-close()` (Full also gets
     `caption-maximize()` for keyboard/accessibility activation).
   - `full.slint`: put `TitleBar` above the existing content; the minimum height
     accounts for the bar.
   - `compact.slint`: add Minimize/Close to the header; derive `drag-end` from the
     left edge of the first control after the title text.
4. **Win32 chrome module** `src/platform/chrome.rs`:
   - `install(hwnd, kind)` with `kind` = Full | Compact; stores per-window state
     (the latest caption metrics in physical pixels, maximize hover/pressed) in a
     thread-local map keyed by HWND. All calls happen on the UI thread.
   - The subclass procedure handles `WM_NCCALCSIZE`, `WM_NCHITTEST`,
     `WM_NCLBUTTONDOWN`/`UP`, `WM_NCMOUSEMOVE`/`LEAVE`, `WM_ACTIVATE`, `WM_SIZE`
     (maximized state), `WM_SYSCOMMAND` (`SC_MINIMIZE` → policy callback), and
     `WM_NCDESTROY` (remove subclass and state). Everything else goes to
     `DefSubclassProc`.
   - After installing, force a frame recalculation with
     `SetWindowPos(..., SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER)`
     and set the DWM corner preference.
   - Keep the hit-test decision a **pure function** (`hit_test(point, client size,
     metrics, maximized, border) -> code`) so it is unit-tested without a window.
   - Add `Win32_UI_Controls` (or the feature that exposes `SetWindowSubclass` in
     `windows` 0.62) to `Cargo.toml` only if `Win32_UI_Shell` does not cover it.
5. **Wiring** in `src/ui/`:
   - Install the chrome once per window after it is first shown (where
     `apply_taskbar_buttons` already gets the HWND), for Full (`lifecycle.rs`) and
     Compact (`modes.rs`).
   - Push metrics into the chrome state whenever the Slint `caption-height`,
     `drag-end`, or max-button properties change, and on scale-factor changes.
   - Push `window-active`, `maximized`, `max-hover`, `max-pressed` from the chrome
     module back into the Slint window through a small callback interface.
   - `caption-close` → post `WM_CLOSE`. `caption-minimize` and `SC_MINIMIZE` → if
     the window's taskbar setting (`taskbar_full` / `taskbar_compact`) is off, hide
     the window and save window state as the close handler does; otherwise
     `SW_MINIMIZE`.
   - Compact: remove `WS_MAXIMIZEBOX` at install.
   - Taskbar button: rewrite `platform::set_taskbar_button` to the hidden owner
     approach (§2), add the hidden owner window to `src/platform/`, and keep
     `apply_taskbar_buttons` calling it after every show. Update its doc comment and
     the `taskbar_full` / `taskbar_compact` doc comments in `src/data/model.rs` if
     they mention the tool window style.
6. **Tests**: unit tests for `hit_test` (drag area, controls, max button, top strip,
   corners, maximized without top strip, scale 1.0/1.5/2.0) and for the minimize
   policy (taskbar on/off). Existing tests stay green.
7. **Specification and docs**:
   - §6.1 and §6.2: describe the custom title bars and their buttons.
   - §8: hiding the taskbar icon no longer changes the window frame or makes the
     window a tool window (it uses a hidden owner window); Minimize then hides to the
     tray.
   - §9: × in the custom title bar keeps hiding to the tray.
   - §12: caption button sizes and colors.
   - `doc/ReleaseNote-Candidate/<current candidate>.md`: entry under
     `## New features` (custom title bars) and `## Fixes` (window frame no longer
     changes to a tool window when the taskbar icon is hidden, and zone-based window
     layout tools keep snapping the window); update `## Summary`.
   - Idea note: status line stays `planned` until execution; on completion link the
     executed plan.
8. **Build and checks through RunBay** (MCP `runbay`): find the RunBay app with
   `list_apps`/`get_app`, use its existing fmt/clippy/test/build actions (create a
   missing one with `set_action`, no duplicates), run with `wait=true`, and report
   run ids. `cargo fmt`, clippy without warnings, and all tests must pass.
9. **GUI check** in the background with offscreen rendering where possible (repository
   rule: no desktop focus or mouse/keyboard driving). The interactive checks in §4
   that need the real desktop are listed for the user in the execution report
   instead of being driven by the agent.
10. Move this plan to `plan/executed/custom-window-chrome.md` with an execution
    report section (results, run ids, anything not verified), update the idea link,
    and commit on `work-bench` (no push).

## 4. Done when

- Full and Compact show the custom title bar in light and dark themes, active and
  inactive.
- Drag, double-click maximize (Full), resize from all edges and corners including
  the top, Aero Snap, the Snap Layouts flyout on the Full maximize button, and the
  system menu (Alt+Space, right-click on the title bar) work.
- Hiding the taskbar icon changes nothing in the frame (no tool window caption,
  corners still round) and removes only the taskbar button; neither window has
  `WS_EX_TOOLWINDOW` in either setting. Minimize then hides to the tray and the tray
  restores it. Turning the setting on and off repeatedly, and restarting Explorer,
  shows/hides the button correctly.
- PowerToys FancyZones: Shift+drag on the title bar of Full and of Compact snaps into
  a zone, with the taskbar icon shown and with it hidden.
- Alt+Tab behavior with the taskbar icon hidden is recorded in the execution report
  (an owned window may be left out of Alt+Tab, as the tool window was before).
- Maximized Full is not cut off at the top and does not cover the taskbar; DPI
  changes between monitors keep hit areas aligned.
- × still hides to the tray in both windows; window position/size saving is
  unchanged.
- fmt, clippy, and tests are clean through RunBay; specification and release note
  candidate are updated; the change is committed on `work-bench`.

Items in this list that need the real desktop and were not checked by the agent are
reported as **not verified**, with steps for the user to check them.

## 5. Risks

- winit may reapply window styles (e.g. on `set_resizable`, maximize, or theme
  changes) and undo `WS_MAXIMIZEBOX` removal or force a frame change; re-run the
  style/`SWP_FRAMECHANGED` step after such calls, as `apply_taskbar_buttons` already
  does after every show.
- An owned window always stays above its owner and is hidden/minimized together
  with it; the hidden owner is never shown, minimized, or destroyed before exit, so
  this has no visible effect. Changing `GWLP_HWNDPARENT` after creation is
  documented as allowed for the owner of a top-level window; verify the taskbar
  button really disappears and returns (step in §4).
- The subclass must not swallow messages winit relies on (DPI, size, activation);
  always call `DefSubclassProc` except where a message is fully handled above.

## 6. Execution report

- **Executed:** 2026-10-24 on `work-bench`; no push.
- **Result:** Full and Compact use their Slint captions with native frame
  subclassing, DPI-aware hit targets, activation colors, and close-to-tray
  callbacks. Full supports native maximize hit testing; Compact suppresses
  maximize, including backend style refreshes. Icon and Icon popup behavior is
  unchanged.
- Taskbar hiding uses one never-shown top-level owner and always clears the
  regular windows' tool-window style. Minimize uses the same save-and-hide
  close path when the taskbar button is disabled.
- The specification (§§6, 8, 9, 12), release note candidate
  `1.0.0-alpha.4`, and idea link were updated. No project instruction files
  were changed.

### Verification

- `Format Rust` (`fmt`): `20261024-225302-fmt-da96` — passed.
- `Offscreen GUI checks` (`gui-check`):
  `20261024-225303-gui-check-1a9f` — passed.
- `Release Win64` (`build`):
  `20261024-225403-build-05b5` — passed: fmt check, warning-free clippy,
  all tests, Windows x64 release build, and local packaging.
- The existing Build action includes fmt check, warning-free clippy, the full
  Windows x64 test suite, and release build/local packaging. Its temporary
  candidate-version substitution is restored after building; the repository
  package version was not advanced for a release.
- Added pure hit-test coverage for drag, controls, maximize, top resize/corners,
  maximized behavior, and 1.0/1.5/2.0 scales; both minimize policies are tested.
  Hidden native test windows verify owner reuse, repeated taskbar-style
  changes, Compact maximize suppression after style refreshes, system-menu
  minimize dispatch, and subclass cleanup. They are never shown or activated.
- Software-rendered GUI checks exercise Full and Compact light/dark and
  active/inactive captions, resized/DPI-scaled caption bounds, caption
  callbacks, the Full restore state, and the Compact agent exclusion.
  Existing console-theme tests still pass with Full's shifted content origin.
  PNG artifacts are outside the repository in
  `../.artefacts/RunBay/gui/custom-window-chrome/`; the light/dark and
  inactive renders were visually inspected.
- No desktop focus was taken and no operating-system mouse/keyboard input was
  sent. Offscreen input was dispatched directly to software windows.

### RunBay actions and retries

Used the existing `build` action rather than duplicating its combined checks;
the existing `check` action was inspected. Added the missing `fmt` action
(`Format Rust`) and `gui-check` action (`Offscreen GUI checks`) to the
existing RunBay app. The latter runs the two GUI regression test targets.

Earlier verification runs, retained in RunBay:
- `20261024-224653-fmt-bb24`, `20261024-224754-fmt-9745`,
  `20261024-224842-fmt-2fa3`, `20261024-225025-fmt-48bd` — formatting passed.
- `20261024-224701-build-dcf1` — corrected the hidden-owner callback ABI
  and removed Slint property-shadowing warnings.
- `20261024-224755-build-2392` — corrected the offscreen renderer callback's
  return type.
- `20261024-224842-build-0897` — the new render test found Full's title bar
  needed an explicit top-left origin.
- `20261024-225026-build-ce19` — the new click test found caption focus/input
  layering needed correction; GUI checks passed after the fix.

### Implementation choices not explicit in the plan

- Slint already has a built-in Window property named `maximized`. The Rust
  appearance input is named `window-maximized` to avoid shadowing it;
  TitleBar retains its internal `maximized` input.
- `SetWindowSubclass` is available through the existing `Win32_UI_Shell`
  feature in windows 0.62. Added only `Win32_UI_HiDpi` and
  `Win32_System_LibraryLoader` features for DPI metrics and owner registration;
  no new dependency was added.
- Compact also filters `WM_STYLECHANGING` and `SC_MAXIMIZE` so backend
  refreshes cannot restore its maximize behavior.
- Full's caption sits above page/dialog overlays; editor/settings content starts
  below it. Caption input is event-driven, and native-to-Slint updates are
  deferred to avoid reentering Slint while winit processes a message.
- Icons were downloaded from the official
  `google/material-design-icons` repository,
  `symbols/web/<name>/materialsymbolsrounded/<name>_24px.svg` (Apache-2.0).

### Real-desktop checks — not verified

All items below remain **not verified** and are intended for the user:

1. In both modes, try light/dark themes and activate another application:
   confirm caption dimming and Close hover colors. Check agent tooltip, pin,
   mode controls, keyboard/accessibility caption labels, and caption controls
   while Settings, the editor, or a dialog is open.
2. Drag from the logo, title, and empty title-row space. Resize all edges and
   corners, including the top. Double-click Full's caption to maximize/restore;
   confirm Compact stays unmaximized after double-click and Win+Up.
3. In Full, hover the maximize button for Windows 11 Snap Layouts, choose a
   layout, then restore. Drag either mode to screen edges for Aero Snap. Verify
   Alt+Space and caption right-click show the system menu.
4. For each mode with its taskbar icon on and off, use PowerToys FancyZones
   Shift+drag to snap into a zone.
5. Toggle each taskbar switch repeatedly, then restart Explorer and repeat:
   only the taskbar button should change; frame, shadow, and round corners
   should stay consistent. The hidden owner must never appear.
6. With each taskbar icon shown, try Minimize, system-menu Minimize, and
   Win+Down. With it hidden, repeat and confirm the window hides to the tray,
   leaves no minimized stub, and restores through the tray. Test Close and
   confirm position/size saving is retained in both modes.
7. Move between monitors at 100%, 150%, and 200% DPI. Check drag/control hit
   alignment and top resize. Maximize Full: its top must not be clipped and
   the taskbar must remain uncovered.
8. With the taskbar icon hidden, inspect Alt+Tab in both modes and record the
   observed inclusion/exclusion. This run did not observe Alt+Tab; owned
   windows may be excluded, as the old tool windows were.
