# Custom window chrome for Full and Compact

- **Date:** 2026-10-24
- **Status:** planned — implemented 2026-10-24; [executed plan and verification report](../../plan/executed/custom-window-chrome.md)

## Context

The Full and Compact windows use the native Windows frame drawn for the Slint/winit
window. The *Taskbar icon* settings (specification §8) hide a window's taskbar button
by switching its extended style to `WS_EX_TOOLWINDOW` (`platform::set_taskbar_button`).
Windows then draws the tool window caption: thinner, no icon, only a close button. The
window therefore looks different depending on a setting that is only meant to affect
the taskbar.

The Compact window also already has its own header row (logo, name, pin, mode
buttons), so the native caption above it is a second title bar.

## Options considered

- **Hidden owner window**: give the window an invisible owner instead of
  `WS_EX_TOOLWINDOW`; owned windows have no taskbar button and keep the normal
  caption. Smaller change, but keeps the duplicated header in Compact and does not
  address the look of the frame.
- **Slint `no-frame` + winit drag/resize**: simplest custom chrome, but loses the
  Windows 11 Snap Layouts flyout and needs its own shadow and resize edges.
- **Win32 subclass that removes only the caption** (chosen): keep the native frame
  (shadow, rounded corners, side/bottom resize, Aero Snap, system menu) and remove the
  caption area through `WM_NCCALCSIZE`; `WM_NCHITTEST` maps the Slint title bar to
  caption, top resize edge, and maximize button (for Snap Layouts).

## Decisions so far

- 2026-10-24: custom chrome for **both Full and Compact**, **always** (not only when
  the taskbar button is hidden), so there is one look and one code path.
- 2026-10-24: technique is the **Win32 subclass** that keeps the native frame and
  removes only the caption.
- 2026-10-24: Full gets a **thin title bar** across the top (logo, "RunBay",
  Minimize / Maximize / Close); the three-panel layout below it does not change.
- 2026-10-24: Compact keeps its header and adds **Minimize and Close** at its right
  end; no Maximize.
- 2026-10-24: while a window's taskbar button is hidden, **Minimize hides the window
  to the tray** (a minimized window without a taskbar button would otherwise become a
  small stub above the taskbar). With the taskbar button shown, Minimize minimizes.
- 2026-10-24: the plan is executed with a commit on `work-bench` at the end (no push).
- 2026-10-24: Full and Compact hide their taskbar button through a **hidden owner
  window** instead of `WS_EX_TOOLWINDOW`. Reason: PowerToys FancyZones (Shift+drag into
  a zone) rejects windows with `WS_EX_TOOLWINDOW` or with a *visible* owner
  (`FancyZonesWindowProcessing::DefineWindowType`), so a tool window cannot be
  snapped while its taskbar button is hidden. An owner that is never shown keeps the
  window a regular window for such tools and still removes the taskbar button. The
  native move loop started by the custom title bar (`HTCAPTION`) is what lets those
  tools detect the drag, and the frame styles (`WS_CAPTION`, `WS_THICKFRAME`,
  minimize/maximize boxes) stay on the window. The Icon window and Icon popup keep
  `WS_EX_TOOLWINDOW`.

## Open questions

None blocking.
