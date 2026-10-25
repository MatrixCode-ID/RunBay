# Plan: Material 3 Expressive pass for Settings, History, the app editor, and keyboard focus

- **Date:** 2026-10-25
- **Status:** not executed
- **Idea:** [doc/ideas/settings-material-expressive.md](../../doc/ideas/settings-material-expressive.md)
- **Specification:** §6.1 (Full, Action History), §6.2 (Compact), §8 (Settings),
  §12 (design language).

## 1. Goal

Bring Settings, the Action History filters, and the Add/Edit app editor in line
with the Material 3 Expressive component specifications, and make every
interactive control reachable and visibly focused with the keyboard. Product
behavior (what each setting does, retention semantics, filter dimensions) does
not change.

## 2. Reference values (M3, verified 2026-10-25)

Taken from the official Material token files
(`material-components/material-web`, `tokens/versions/latest/sass`) and the
Material Components Android button group documentation. RunBay uses M3 density
-2 (each step is -4px) where M3 defines density for a component.

| Component | M3 value | RunBay value in this plan |
| --- | --- | --- |
| Focus indicator | 3px, `secondary`, outer offset 2px; list items inner offset -3px | same |
| Filter chip | 32px high, `corner-small` (8px), unselected 1px `outline-variant`, label `on-surface-variant`; selected `secondary-container` / `on-secondary-container`, no outline, 18px leading check icon | same |
| Connected button group (small) | 40px high, 2px gap, outer corners full, inner corners 8px, selected button inner corners 50% (pill) | same |
| Segmented button | deprecated in M3 Expressive | not used |
| Segmented list (Expressive list) | 2px gap between items, item `corner-extra-small` (4px), group `corner-large` (16px), item `surface`, one-line 56px, two-line 72px, 16px leading/trailing space, label `body-large` | density -2: one-line 48px, two-line 64px; other values unchanged |
| Navigation item | active indicator `secondary-container`, full corner, 56px | density -2: 48px |
| Outlined text field | 56px, `corner-extra-small` (4px), 1px `outline`, focused `primary` outline | density -2: 48px (see §3 step 3) |
| Switch | 52x32 track, 16/24px handle, icons optional | unchanged (already matches); focus ring added |
| Shape scale | 4 / 8 / 12 / 16 / 20 / 28 / 32 / 48 / full | add missing tokens to `Theme` |

## 3. Decisions (final)

| Topic | Decision |
| --- | --- |
| Implementation | RunBay's own components in `ui/components.slint`, built to the values in §2. The Slint Material Components library is a reference only; no new dependency. |
| Theme tokens | Add `secondary` (light `#625b71`, dark `#ccc2dc`) and `on-secondary` (light `#ffffff`, dark `#332d41`), shape tokens `corner-extra-small` (4px), `corner-large-increased` (20px), `corner-extra-large-increased` (32px), `corner-extra-extra-large` (48px), `corner-full` (9999px), and focus tokens `focus-thickness` (3px), `focus-offset` (2px). |
| Density | M3 density -2 for lists, navigation items, and text fields. Buttons keep 40px (`PillButton`) and icon buttons keep their sizes. |
| Settings heading levels | "Settings" stays title-large in the page header; the category heading in the content panel becomes title-medium; the category description stays. |
| Settings layout | Content column max 720px, left-aligned in the content panel; the remaining space stays empty (no new content). |
| Settings groups | Each group is a titled **segmented list** (§2): group title in `title-small` / `primary` above the list (existing `SectionTitle`), each setting a list item with label (`body-large`) and optional hint (`body-small`, `on-surface-variant`) on the left and the control trailing on the right. When the content column is narrower than 480px, the control moves below the label inside the same item. |
| Group contents | **Appearance**: Theme (Theme, Console theme) · Floating window (Floating icon size, Always on top in Compact mode, Always on top in Icon mode) · Taskbar (Taskbar icon in Full mode, Taskbar icon in Compact mode) · Output (On-screen output line limit). **Startup & data**: Startup (Load with Windows) · Data (Use another data folder + Choose folder). **Action defaults**: Running (When the app is still running) · History retention (Build, Run, Build + Run, Custom). **Updates**: Automatic updates (Update automatically switch, status as hint) · Manual check (separate item with a tonal Check now button). **Agents**: MCP access (selector plus a hint that changes with the selection, texts below) · Setup (Copy MCP setup instructions). |
| MCP access hints | Matching specification §15.2. Off: "Every agent request is refused." Ask first: "Agents can read apps and runs. RunBay asks before an agent runs, stops, or changes anything." Unrestricted: "Agents can run, stop, and change apps and actions without asking." |
| Settings navigation | Items 48px, active indicator `secondary-container` with full corners, keyboard focusable. |
| Retention control | A new `ConnectedButtonGroup` component (§2, single selection) with segments **Off / Last N / Forever**. The run count field is shown only for Last N, followed by the text "runs", and has no clear (X) button. Used in Settings → Action defaults and in the editor's action details. Saved values and semantics (`parse_retention`, Off / Last N / Forever) are unchanged. |
| History filters | App and action stay `ComboBox`es on one row. Status and source become single-select filter chips (§2) on their own rows: Running, Success, Failed, Stopped / Clicks, Agents. No "All" chip: no selected chip means all; clicking the selected chip clears it. The selected chip shows the 18px check icon. Internal indices stay as they are (0 = all), so `src/ui/history.rs` filtering does not change. |
| History panel width | 372px → 400px, so four status chips with one check icon fit. Splitter minimums in §6.1 still apply. |
| Input controls | Keep std-widgets `LineEdit` and `ComboBox` (material style). Step 3 measures their rendered height; if they can be set to 48px cleanly, do that everywhere in Settings and the editor. If not, add a `TextField` wrapper (TextInput-based, 48px, 4px corners, 1px `outline`, `primary` focus outline) for `LineEdit` uses only and leave `ComboBox` at its native height; record the outcome in the execution report. |
| Editor | Keep three columns. Section titles per column; the action detail column is divided into **Script**, **Environment**, and **Behavior** (working directory, when running, history retention). Uniform field heights and label style (`FieldLabel` above each field). Button hierarchy: Save filled, Cancel text; Choose… / Image… / Choose folder tonal; Remove and + Add variable text buttons. |
| Keyboard focus | `PillButton`, `SymbolButton` (including `ActionBar` buttons and More), filter chips, connected button segments, Settings navigation items, and History rows get a `FocusScope`. Tab / Shift+Tab move between focusable controls in declaration order; Enter and Space activate. Up/Down move between Settings navigation items and between History rows (selecting the row, as a click does). Focus is not taken on mouse click, so the ring appears only for keyboard focus. Ring: §2 focus indicator, following the control's own shape. |
| Motion | Only on interaction, never while idle: navigation indicator position, selected chip / segment shape and color, and focus ring appearance, using `Theme.motion-fast` (`ease-out`, `ease-out-back` for shape changes). No new idle or repeating animation. |
| Compact action targets | Compact (and Icon popup, if it uses the same value) `ActionBar` and its Stop button go from 30px to 32px, matching Full. |
| Console toolbar spacing | Measure the distance between the bottom of the Full caption (`TitleBar`, 36px) and the console toolbar buttons. If it is under 8px, add top padding so it is 8px. Report the measured value. |
| Specification | Update §6.1 (History filters as chips, 400px flyout), §6.2/§6.3 (32px action targets), §8 (groups, retention control, navigation, MCP hints), §12 (focus indicator, density -2, connected button group, filter chip, segmented list, motion scope). |
| Release note | Add to `doc/ReleaseNote-Candidate/1.0.0-alpha.7.md`: `## New features` — keyboard navigation and focus indicators; redesigned Settings groups and retention control; chip filters in Action History. Fill `## Summary`. |
| Verification | Executor runs fmt, clippy, and tests via RunBay actions and builds the app; visual checks are done by the user with the checklist in §5. |
| Commit | One commit on `work-bench` after all checks pass, e.g. `Apply Material 3 Expressive pass to Settings, History, and editor`. No push. |

## 4. Steps

1. **Tokens** (`ui/theme.slint`): add the color, shape, and focus tokens from §3.
2. **Shared components** (`ui/components.slint`):
   - `FocusRing`: a non-interactive rectangle drawn around its parent with
     `focus-thickness`, `focus-offset`, `Theme.secondary`, radius = parent radius
     + offset; visible only while the owning `FocusScope` has focus.
   - Add `FocusScope` + `FocusRing` + Enter/Space handling to `PillButton` and
     `SymbolButton` (keep existing accessibility properties).
   - `FilterChip` (§2), with `selected`, `text`, `clicked`, check icon, state
     layers (8% hover / 12% pressed / focus), focus ring.
   - `ConnectedButtonGroup` with a model of labels and `selected-index`,
     single selection, selected segment morphs inner corners from 8px to pill.
   - `SegmentedList` + `ListItem` (label, hint, trailing `@children`, narrow
     fallback below 480px), using density -2 heights.
   - Add the `check` icon (Material Symbols Rounded, `check.svg`) to `ui/icons`
     and `Icons`.
3. **Input height check**: render Settings, measure std `LineEdit`/`ComboBox`
   heights, then apply the Input controls decision.
4. **Settings** (`ui/pages.slint`): heading levels, 720px column, navigation
   items, groups and items as in §3, Updates split, MCP hints (bridge a
   selection-dependent hint from the existing `mcp-index`), retention rows with
   `ConnectedButtonGroup` + count field. Replace the old vertical `SettingRow`.
5. **History** (`ui/pages.slint`, `ui/full.slint`): one row of app/action
   dropdowns, status and source chip rows mapped to the existing filter indices,
   row focus with Up/Down, flyout width 400px.
6. **Editor** (`ui/pages.slint`): sections, field heights, button styles,
   retention with `ConnectedButtonGroup`.
7. **Compact and console**: 32px Compact action targets; measure and adjust the
   console toolbar spacing.
8. **Rust wiring** (`src/ui/settings.rs`, `src/ui/editor.rs`, `src/ui/history.rs`,
   as needed): only adapt to renamed or new properties; no behavior change.
   Existing tests stay green; add tests if a mapping (e.g. MCP hint selection,
   chip-to-index) is implemented in Rust.
9. **Docs**: specification sections in §3, release note candidate, idea note
   status stays `planned` until executed.
10. **Checks** via RunBay: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
    `cargo test`, release build. Create missing RunBay actions with `set_action`
    if they do not exist.
11. **Commit** on `work-bench` (check the branch first) and move this plan to
    `plan/executed/` with the execution report.

## 5. User checklist (visual)

Check each in Light and Dark:

1. Settings: one title-large "Settings", title-medium category heading, groups
   as segmented lists with 2px gaps, controls on the right, content not wider
   than 720px; narrowing the window moves controls below labels.
2. Settings navigation: 48px pill indicator; Up/Down and Tab work; focus ring is
   visible only after keyboard use.
3. Retention: Off / Last N / Forever buttons, count field only for Last N, saved
   values survive a restart; same control in the editor.
4. Updates: switch and Check now in separate items. Agents: hint changes with
   the MCP access selection.
5. History: 400px flyout, app/action dropdowns on one row, chips with check icon
   when selected, clicking a selected chip clears it, filters return the same
   runs as before; Up/Down moves through rows.
6. Editor: three columns with sections, uniform field heights, button weights as
   in §3.
7. Tab through Full, Compact, and the editor: every button shows a 3px ring with
   2px offset; Enter/Space activate; mouse clicks do not show the ring.
8. Compact action buttons are 32px; the console toolbar is at least 8px below
   the caption.
9. Idle: with nothing running, no animation or redraw (Task Manager GPU/CPU
   stays idle).

## 6. Out of scope

Thumb icons in switches, the Slint Material Components library, new Settings
content (status panels, JSON previews), console theme changes, and any change to
what settings or filters do.

## 7. Execution report

_To be filled in by the executor._
