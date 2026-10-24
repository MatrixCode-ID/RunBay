# Plan: Select and copy text in the output

- **Date:** 2026-10-24
- **Status:** executed on 2026-10-24. Automated checks passed; visual validation
  remains with the user as agreed below.
- **Idea:** [doc/ideas/output-text-selection.md](../../doc/ideas/output-text-selection.md)
- **Specification:** §5.3 (on-screen output), §6.2 (Compact), §6.3 (Icon).

## 1. Goal

Let the user select any part of a run's output with the mouse or keyboard, across
several lines and with colors kept, and copy it, in the Full Output panel and in the
Compact/Icon output popups, the way a console does.

## 2. Decisions (final)

| Topic | Decision |
| --- | --- |
| Approach | Own selection over the line model (idea option 1). No `TextInput`/`TextEdit`; the rows stay `Text` spans. |
| Selection unit | A range from an anchor `(line, column)` to a focus `(line, column)`; columns count characters of the plain text (ANSI removed). Lines are addressed by their index in the run's stored lines, not by the on-screen model index, so the ring buffer trimming does not shift a selection. |
| Mouse | Press and drag selects; Shift+click extends from the anchor; double-click selects a word (letters, digits, `_`, `-`, `.`, `/`, `\`, `:`); triple-click selects a whole line; a plain click without drag clears the selection. Dragging past the top/bottom edge auto-scrolls. |
| Keyboard (output focused) | Ctrl+C and Ctrl+Insert copy the selection; Ctrl+A selects all lines of the run; Shift+Left/Right/Up/Down, Shift+Home/End extend the focus; Escape clears the selection. Ctrl+C with no selection does nothing (it never stops a run). |
| Copied text | Plain text, lines joined with `\r\n`, as *Copy all* does today. A selection that ends at column 0 of a line does not add that empty line. |
| Follow mode | Paused while a selection exists, as when the user scrolls up. It resumes when the selection is cleared and the view is at the bottom. |
| Highlight | A rectangle behind the selected columns of each visible row, in the theme's selection color (a new `Theme.console-selection` token for light and dark, with readable contrast for the span colors). Span colors are not changed. |
| Column from x | `column = floor((x - padding-left) / char-width)`, clamped to the line length. `char-width` is measured once from the monospace font (a hidden `Text` with one character, `preferred-width`) instead of the fixed 8 px now used for `content-width`; the fixed value is replaced by the measured one. Wide characters (CJK, emoji) are treated as one column in this plan; noted as a known limitation. |
| Scope of windows | Full Output panel, Compact output popup, and Icon output popup share one selection component and one Rust selection model. |
| Context menu | Right-click on the output opens *Copy* (enabled with a selection) and *Select all*. |
| Release note | Add the feature under `## New features` of a new candidate `doc/ReleaseNote-Candidate/1.0.0-alpha.2.md` (`Previous: 1.0.0-alpha.1`), created in the format of `doc/ReleaseNote/README.md`; the `.gitkeep` stays. |
| Visual test | Not done by the executor. The executor runs fmt, clippy, and tests (including unit tests of the selection logic); the user tests the UI with the checklist in §5. |
| Commit | One commit on `work-bench` at the end, after the checks pass. No push. |

## 3. Steps

1. **Selection model (Rust, `src/ui/selection.rs`)**: a pure type holding anchor and
   focus, with operations: start, extend, select word, select line, select all,
   move focus by key, clear, normalized range, and `text(lines)` building the copied
   text from plain lines. Unit tests cover multi-line ranges, reversed ranges, word
   and line selection, clamping, the column-0 end rule, and select all.
2. **Plain text per line**: keep the plain text (ANSI stripped) of each displayed
   line next to the spans, so hit-testing and copying need no re-parsing. Add the
   line's run index to `OutputRow` (and `selected-from` / `selected-to` columns, or
   `-1` when the row has no selection) in `ui/types.slint` and `src/ui/convert.rs`.
3. **Shared output view (`ui/components.slint`)**: one `OutputView` component used by
   `ui/full.slint` and `ui/compact.slint`, containing the `ListView`, the rows with
   their selection rectangle, the hidden measuring `Text`, a `TouchArea` for mouse
   handling (press, move, release, double/triple click detection by time and
   position, Shift modifier), a `FocusScope` for the keys, and the context menu.
   Callbacks to Rust: `selection-start`, `selection-extend`, `selection-word`,
   `selection-line`, `selection-key`, `selection-clear`, `copy-selection`,
   `select-all`. Replace `OutputLineView` and `MiniOutputLine` with it, keeping their
   font sizes and paddings (13 px / 12 px, 12 px / 8 px).
4. **Wiring (`src/ui/mod.rs`, `src/ui/modes.rs`)**: own one selection per output view
   (Full, Compact/Icon popup); update the `selected-from/to` of the affected rows on
   every change; clear the selection when another run is displayed or the view is
   cleared; when the ring buffer drops lines inside the selection, clamp the range to
   the remaining lines. Copy through `platform::copy_text`.
5. **Follow mode**: pause auto-scroll while a selection exists in the Full panel and
   the popups; resume on clear when the view is at the bottom. Drag auto-scroll near
   the edges uses a timer that runs only while dragging (no idle polling, §11).
6. **Theme**: add `console-selection` for light and dark in `ui/theme.slint`.
7. **Specification**: describe selection, shortcuts, the context menu, and follow-mode
   pause in §5.3, and mention it for the popups in §6.2/§6.3.
8. **Release note candidate**: create `doc/ReleaseNote-Candidate/1.0.0-alpha.2.md`
   with a `## Summary` and the feature under `## New features`.
9. **Checks**: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test`. Fix anything they report.
10. **Report and commit**: write the execution report into this plan (what was done,
    known limitations, the checklist below for the user), move the plan to
    `plan/executed/`, update the plan link in the idea note to the new path, then commit on
    `work-bench` with an English imperative title (e.g.
    `Add text selection and copy to the output`).

## 4. Out of scope

- Wide-character (double-width) column handling.
- Selection in the app editor or Action History (they use their own widgets).
- ConPTY and the extra SGR attributes ([console-colors](../../doc/ideas/console-colors.md)).

## 5. User checklist (visual test)

1. Drag across several colored lines in Full: highlight follows, colors stay.
2. Ctrl+C / Ctrl+Insert, then paste: text matches, no ANSI codes, `\r\n` line ends.
3. Double-click a word, triple-click a line, Shift+click to extend.
4. Ctrl+A, Shift+arrows, Shift+Home/End, Escape.
5. Right-click: *Copy* and *Select all*.
6. During a live run: selecting pauses auto-scroll; Escape at the bottom resumes it.
7. Drag beyond the panel edge: the list scrolls.
8. The same in the Compact and Icon output popups.
9. Light and dark theme: the highlight is readable.

## 6. Done when

All steps are done, the checks pass, the specification and release note candidate are
updated, the plan is in `plan/executed/` with its report, and the change is committed
on `work-bench`.

## 7. Execution report (2026-10-24)

- Added a shared, virtualized `OutputView` for Full and Compact/Icon popups,
  retaining colored/bold spans and drawing selection behind them. Hit testing
  and horizontal content width use a hidden monospace glyph measurement.
- Added a pure Rust selection model and cached ANSI-free lines with stable
  run indices. Mouse selection, word/line selection, Shift+click, keyboard
  extension, copying, Select all, and clearing are wired through `OutputBridge`.
  Only rows whose highlighted columns change are notified.
- Selection pauses follow mode. Clearing resumes at the bottom; dragging
  beyond the visible edges scrolls with a timer active only during the drag.
  Run changes and Clear view reset selection; buffer trimming clamps endpoints.
- Output snapshots include their first run index and omit already displayed
  lines, so opening a run during batched event delivery cannot duplicate rows
  or corrupt the selected range. Popups now use the configured output line
  limit, matching Full and Select all over retained output.
- Updated specification §§5.3, 6.2, and 6.3 and the alpha.2 candidate. Before
  implementation, compared the stale alpha.1 candidate with the released note:
  preserved only unreleased entries in alpha.2 and removed the stale candidate.
  Updated release rules in `CLAUDE.md`, the release guide, and §14.2 following
  the user's instructions to maintain RNC continuously and recreate it as soon
  as the previous note moves to RN.
- Validation: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  and `cargo test` passed (76 tests, including five selection unit tests).
  Used the existing MSVC environment setup and external Cargo target directory.
- Visual testing was not performed, as agreed. The user checklist in §5 remains
  available for Full, Compact, Icon, light/dark, and live output.
- Known limitation: wide CJK/emoji characters count as one column, as planned.
  Select all covers retained output; logs on disk remain complete.
- The implementation commit excludes changes already present before this task.
  No push is performed.
