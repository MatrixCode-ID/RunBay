# Drag to reorder apps and move them between groups

- **Date:** 2026-10-24
- **Status:** discussion

## Context

The specification (§6.1) says apps in the navigation can be dragged to reorder them and
to move them between groups. RunBay v1 ships an equivalent without drag and drop: the
app menu has *Move up* / *Move down* (within the group), and the group is chosen in the
app editor. Custom actions are reordered with up/down buttons in the editor.

Drag and drop in a Slint `ListView` with rows of different heights needs a custom
implementation (hit testing rows, an insertion indicator, auto-scroll near the edges),
which was left out of v1 to keep the first release focused.

## Decisions so far

- 2026-10-24: v1 uses menu commands for ordering; the data model (`order`, `group`) is
  already what drag and drop would change, so no data migration is needed later.

## Open questions

- Should a drop between two groups also be possible onto a collapsed group header?
- Should the order of groups themselves be draggable, or set in a small group manager?
- 2026-10-25: the user requested icon-only drag reordering. Full, Compact, and
  the Icon app list now reorder within a group, with an insertion marker and
  edge auto-scroll. Other card content does not start a drag. This was a direct
  implementation request; no execution plan was needed. Cross-group dragging
  and group reordering remain under discussion.
