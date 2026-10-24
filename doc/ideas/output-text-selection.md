# Select and copy text in the Output panel

- **Date:** 2026-10-24
- **Status:** planned
- **Plan:** [plan/unexecuted/output-text-selection.md](../../plan/unexecuted/output-text-selection.md)

## Context

Users expect to select part of the output with the mouse and copy it, as in a console.
Today the Output panel only offers *Copy all*.

The panel (`ui/full.slint`) is a `ListView` of `OutputLineView` rows, and each row is a
`HorizontalLayout` of Slint `Text` elements, one per colored span. Slint's `Text` is
display-only: it has no selection, caret, or copy. The `ListView` is virtualized, so
only visible rows exist as elements. The specification (§5.3) lists no selection
either, so this is a missing feature, not a regression.

## Options

1. **Own selection over the line model**: a `TouchArea` over the list turns press/drag
   into a (line, column) range. With the monospace font the column is
   `(x - padding) / char-width` (a fixed character width is already assumed for
   `content-width`). Rows draw a highlight behind the selected columns; Ctrl+C and a
   context menu *Copy* build the text in Rust from the stored lines, so selection
   works across virtualized and scrolled-away rows and keeps colors.
   - Needs auto-scroll while dragging at the edges, double-click (word) and
     triple-click (line), and a measured character width (wide characters such as
     CJK or emoji take two columns).
2. **Read-only `TextInput` per row**: native selection inside one line only, no
   multi-line selection, and one color per row (spans cannot be styled).
3. **"Select mode" switch**: replace the list with a read-only `TextEdit` holding the
   plain text. Native multi-line selection, but colors are lost while selecting, and
   10,000 lines in one `TextEdit` cost memory and layout time.

## Assessment (2026-10-24)

Option 1 is the only one that behaves like a console (multi-line, colors kept, works
with the 10,000-line ring buffer) and stays within the performance targets, because
no extra elements are created. It fits well with the planned attribute rendering in
[console-colors](console-colors.md) (inverse/background drawing per span).

## Decisions so far

- 2026-10-24: Option 1, an own selection over the line model: multi-line, colors
  kept. Reason: the only option that behaves like a console, works across the
  virtualized ring buffer, and adds no elements per line.
- 2026-10-24: All keyboard shortcuts: Ctrl+C (and Ctrl+Insert) copy, Ctrl+A selects
  all lines of the run, Shift+arrows / Shift+Home / Shift+End extend the selection,
  and Escape clears it. Mouse: drag, Shift+click to extend, double-click selects a
  word, triple-click a line.
- 2026-10-24: While a selection exists, follow mode (auto-scroll) is paused, as when
  the user scrolls up; it resumes when the selection is cleared and the view is
  back at the bottom.
- 2026-10-24: The same selection works in the Compact/Icon output popups.

## Open questions

- None. Execution decisions (release note, visual test, commit) are in the plan.
