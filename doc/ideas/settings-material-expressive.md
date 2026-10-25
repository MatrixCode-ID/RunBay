# Settings: Material 3 Expressive composition

- Date: 2026-10-25
- Status: planned
- Resulting plan: [plan/unexecuted/settings-material-expressive.md](../../plan/unexecuted/settings-material-expressive.md)

## Context

The user supplied a Settings screenshot and an external assessment that calls it
baseline Material 3 rather than Material 3 Expressive. This note records an
evaluation and possible improvements; it is not an implementation request.

The screenshot reads as a conventional settings form: similarly weighted labels,
small selection fields, vertically stacked switches, and little visible grouping
inside a large content surface. The current implementation in `ui/pages.slint`
uses Slint standard `ComboBox` and `LineEdit` controls, a custom `ToggleSwitch`,
and a shared vertical `SettingRow`. `ui/theme.slint` supplies M3 colors, type
sizes, corner sizes, and motion durations. These establish a foundation but do
not by themselves establish expressive composition.

The application also has custom action-button shape changes elsewhere in
`ui/components.slint`. A static Settings screenshot cannot establish whether
the whole application uses expressive interaction or motion.

## Assessment

- The assessment is directionally useful about the screen's restrained visual
  hierarchy, but its categorical classification is stronger than the evidence.
- M3 Expressive expands M3; it is not a requirement to replace every baseline
  component or introduce organic asymmetry on every screen.
- Google's research emphasizes color, shape, size, motion, and containment to
  direct attention and group related content. It also explicitly prioritizes
  context, familiar interaction patterns, and functionality.
- Larger controls and emphasized typography are available tactics, not a
  requirement to enlarge every setting or animate every heading. Desktop
  density and input methods should inform their use.
- Rounded navigation selection alone cannot distinguish baseline from expressive
  design, and a screenshot cannot demonstrate motion or interaction quality.

## Follow-up: supplied audit report

Reviewed on 2026-10-25:
`../.artefacts/RunBay/Report/Audit Report Material Design By Google.md`.
The document identifies its author as an AI collaborator and supplies no source
citations or measurements. Its embedded implementation prompt is reference
material, not authorization to change the application.

Several findings do not match the supplied screenshot or current source:

- The navigation selection is described as a thin vertical rectangle. The
  screenshot already shows a rounded horizontal selection background; current
  source uses a 44px row with the 12px corner-medium token. It is not a perfect
  stadium, but the reported legacy vertical indicator is absent.
- The proposed switch track is 52 by 32px with 16/24px off/on thumbs. Existing
  `ToggleSwitch` already has those dimensions, plus a 28px pressed thumb,
  keyboard handling, and accessibility properties. The report's replacement
  omits those keyboard and accessibility features and declares a Window-based
  component rather than following the existing Rectangle-based control.
- The input is described as unbordered, but the screenshot visibly has an
  outline. Focus behavior and exact icon spacing were not demonstrated by the
  screenshot, so these require verification rather than an assumed defect.
- Most proposed palette values already exist in `Theme`, including light and
  dark variants. A fixed list of light-theme hex values is not dynamic color
  generation and should not replace the existing theme system.
- The report treats the closed selection field and the popup menu as if they
  shared one universal radius requirement. It supplies no official component
  reference to support its mandatory 8-to-12px range. Their specifications and
  supported Slint styling APIs need separate verification.
- Merely selecting an on-surface-variant token does not establish WCAG contrast
  for every context; the actual foreground/background pair, text size, and any
  opacity must be evaluated.

The new report chiefly discusses baseline M3 compliance, whereas the original
question concerns M3 Expressive composition. It does not resolve the hierarchy,
grouping, emphasis, or motion questions above, and is not a verified compliance
audit or a ready-to-execute implementation specification. Keep the supplied
report intact as reference material and retain the current discussion status.

## Follow-up: complete ten-screen audit package

Reviewed on 2026-10-25:
`../.artefacts/RunBay/Report/Google UI Audit/2026-10-25/`.

The package contains a complete Markdown report (duplicated identically in
`public/`), a React dashboard with per-screen findings in
`src/data/auditData.ts`, Slint/Rust example snippets, and a ten-page PDF of source
screenshots. The PDF covers Full, Full with History, Compact, Compact with output
open, Add app, and the five Settings categories. The extracted original images
were inspected; the dashboard was read as source and was not installed or run.

### Useful discussion topics in the supplied report

- Settings grouping, typography hierarchy, and the balance between the content
  width and the large available panel.
- Console readability when History occupies part of the Full window.
- Consistency of field dimensions and labels across the editor and Settings.
- The visual distinction between utilities, primary actions, and explanatory
  content, including Updates and Agents.

These are discussion topics, not verified violations or implementation approval.

### Evidence limitations and corrections

- The screenshots visibly contain rounded app cards, a tonal Add app button,
  a rounded console, and separate Windows caption and console-toolbar rows.
  They do not support the report's claims that these containers are absent or
  that the toolbar overlaps the Windows caption buttons.
- The History screenshot already contains distinct title and metadata sizes,
  selected-row treatment, status symbols, and colored source badges. The report
  overlooks these when describing uniformly styled, unstructured entries.
- The Compact output screenshot shows a floating overlay with rounded corners
  and a shadow. Current `ui/compact.slint` uses a `PopupWindow`; it is not an
  inserted layout row pushing subsequent apps downward as the report asserts.
- Small visible icons are not measurements of their hit targets. Action strips
  default to 32px bounds; SymbolButton defaults to 36px, with explicit compact
  overrides. The report's claims of targets
  below 20px or 24px are not accompanied by hit-region or DPI measurements.
- The claimed text contrast values below 2.5:1 or 2.8:1 have no sampled colors
  or calculation method. For comparison, the current light-theme hint pair
  `#49454f` on `#f7f2fa` calculates to approximately 8.47:1 at full opacity.
  This checks that token pair, not every rendered label or the entire UI.
- The 52.8/100 score is the mean of five supplied pillar scores, but the report
  gives no scoring rubric or traceable measurements for those inputs. It should
  not be represented as an official Google compliance score.
- Dashboard data declares 46 violations (9 critical, 24 major, 13 minor), while
  its actual issue lists contain 27 entries (5 critical, 16 major, 6 minor).
- The report contains no official source URLs. Statements that every small
  component must be a pill, switches must contain check/cross icons, or Save
  must be at the bottom are not substantiated by component-specific references.
- The package supplies illustrative code, not validation evidence that the
  snippets compile or integrate with RunBay. Its card snippet has no content
  insertion point, and its interactive examples omit existing keyboard and
  accessibility handling. The proposed light-only palette duplicates existing
  tokens and does not establish dark-mode support or spring physics.

### Conflicts with existing product decisions

- The proposed Primary Action + Overflow arrangement would hide actions that
  the user has explicitly required to remain directly available when they fit.
- Replacing retention controls with a 5-to-100 slider does not preserve the
  existing Off / Last N / Forever modes or unrestricted numeric selection.
- A proposed "MCP Daemon: Active on Port 3000" status does not describe RunBay's
  stdio MCP transport and named-pipe GUI bridge.
- Suggested JSON previews, syntax highlighting, detached consoles, and extra
  warning banners are product changes rather than necessary consequences of
  a visual audit. They remain proposals in the supplied report.

No report instructions were executed. Keep this topic in discussion; no UI,
backend, specification, project rules, or implementation plan is changed.

## Follow-up: revised response in the same audit package

Reviewed again on 2026-10-25 after the user supplied Google's reply. The Markdown
now begins "LAPORAN AUDIT UX/UI RUNBAY (REVISI & KLARIFIKASI TIM DESAIN)" and
`index.html` is a standalone revised dashboard. Earlier findings above describe
the previous package contents and remain as discussion history.

### Corrections accepted by the revised report

- Acknowledges all five visual corrections: app cards, tonal Add app button,
  separate console toolbar, History hierarchy/source badges, and Compact output
  overlay.
- Withdraws the categorical objection to top-right Save placement and recognizes
  that the dialog pattern permits a top action bar.
- Separates specification claims, contextual guidance, visual preferences, and
  proposed features. Marks hit regions and motion as requiring code/runtime
  verification.
- Retains directly visible actions and Off / Last N / Forever retention, and
  corrects MCP transport to stdio/named pipe.

### Remaining evidence problems

1. **Contrast is still incorrectly classified as a verified failure.** The
   revised report attributes hint text to `#9e98a8` on `#f4eff7` and folder-path
   text to `#8c8696` on the same background. Inspection of the original embedded
   PDF images instead finds `#49454f` text and `#f7f2fa` background in both
   cited label areas, matching the source tokens. On the original 1080x837
   page images, the inspected bounds are page 6 `(213,311)-(570,324)` for the
   console-theme hint and page 7 `(213,360)-(425,374)` for the data-folder hint.
   The full-opacity token pair has an approximately 8.4701:1 contrast ratio.
   This establishes those pairs, not blanket accessibility compliance. Lighter
   antialiased edge pixels must not be substituted for the underlying text color
   when evaluating application text contrast.
2. **Even the supplied sample calculations differ from the stated results.**
   Recalculation gives approximately 2.468:1 for `#9e98a8` on `#f4eff7` and
   3.1072:1 for `#8c8696` on `#f4eff7`, rather than 2.38:1 and 3.15:1. Neither
   pair establishes the actual foreground/background of the cited RunBay labels.
3. **The package is not synchronized.** Revised Markdown and standalone HTML
   claim 22 verified issues and a 68.5/100 score, but their final table contains
   12 items, including a compliant item and items awaiting verification. No
   complete 22-item inventory is supplied there. React source still contains
   the original 52.8 score, original findings, 27 listed issues, and counters
   totaling 46. The revised score is explicitly heuristic but still has no
   reproducible scoring rubric.
4. **Retention clear-button semantics are misstated.** The revised report says
   X disables retention or resets its limit. In current source the numeric
   control is a standard LineEdit; Off is determined by the mode selector.
   `parse_retention` rejects an empty Last N count rather than mapping it to Off.
   Changing this UI must preserve those mode semantics.

The revised report is more useful as design discussion, but its remaining
"verified" labels do not establish defects in RunBay. Existing 32px action
targets and theme tokens should be accounted for before proposing changes to
them. Grouped Settings, clearer typography, and form alignment remain options
for discussion, not an approved implementation scope.

## Follow-up: third review (`ux_review_report.md`)

Reviewed on 2026-10-25: `../.artefacts/RunBay/Report/Google UI Audit/ux_review_report.md`, written
for the same ten-page screenshot PDF. The PDF pages could not be rendered in
this session; claims were checked against current source instead. The report
again cites no official component references or measurements, and it does not
acknowledge the corrections the previous revision accepted.

### Claims not supported by current source

- **Unlabelled Compact glyphs without tooltips.** Every `ActionBar` button sets
  `tooltip: action.name`, and the overflow menu lists action names
  (`ui/components.slint`). Compact header controls also have tooltips.
- **Heavy 1px strokes and full-height dividers.** Only two 1px borders exist in
  `ui/*.slint`; containment is mostly tonal surfaces and rounded containers.
- **Inconsistent radii, proposed token set.** The proposed `radius-s` to
  `radius-xl` values (8/12/16/28px) are exactly the existing `Theme.corner-*`
  tokens. The palette snippet is light-only and duplicates existing tokens.
- **Dark console in Light mode as a harsh break.** The console already has its
  own Light / Dark / Follow UI preference (specification §5.3); a dark console
  in a light UI is a user choice, not a fixed design.
- **48dp minimum targets.** That is a touch guideline; RunBay's 32px action
  targets are a deliberate desktop density decision.

### Re-raised proposals that conflict with decisions

- **Primary action + overflow for secondary actions** again conflicts with the
  rule that every configured action is a direct icon button while it fits. The
  previous revision had already withdrawn this.
- **Assist chips with text for status** would widen every card; current badges
  explain their meaning in tooltips (specification §5.4).

### Points that are accurate and worth discussing

1. **Settings title redundancy.** The page shows "Settings" and then the
   category name as a second heading (`ui/pages.slint`). Could merge into one
   heading or demote one level.
2. **History filter row.** Four `ComboBox` filters are accurate. Filter chips
   or a single filter bar are a valid option, but must keep the same four
   dimensions and work in a narrow History panel.
3. **Switch thumb icons.** `ToggleSwitch` has no thumb icon. M3 lists the icon
   as optional, not required; low priority.
4. **Retention count clear button.** The X is the standard `LineEdit` clear
   affordance; it is easy to misread as "turn retention off". A numeric field
   without a clear button, or a stepper, would avoid that.
5. **Motion.** Only five `animate` blocks exist (switch, button shape, panel
   width). Short transitions for drawers, dialogs, and selection changes are a
   valid option, provided nothing animates while idle (specification §11).
6. **Editor field fill.** Filled inputs versus outlined `LineEdit` remain an
   open styling question, tied to what Slint's standard widgets allow.

### Repository hygiene

The supplied report names apps visible in the screenshots. Those names must not
be committed under the open-source rules. All supplied reports were moved out
of the repository to the local artefacts folder `../.artefacts/RunBay/Report/`
on 2026-10-25; paths in this note refer to that folder.

## Follow-up: final synchronized AI Studio report

Reviewed on 2026-10-25: the latest
`../.artefacts/RunBay/Report/Google UI Audit/2026-10-25/RUNBAY_UI_UX_REVIEW_MATERIAL3_EXPRESSIVE.md`
("Laporan Final Tersinkronisasi"). It drops the numeric score, and its 22-item
inventory now adds up (6 supported, 3 to verify, 3 preferences, 10 withdrawn).
It withdraws the contrast claim using the same `#49454f` on `#f7f2fa` pair
(about 8.47:1) recorded above, and corrects the retention X description. This
is the first version of the package whose inventory is internally consistent.

### Verification items checked against source

- **RB-VERIF-01, Compact hit targets.** Compact `ActionBar` uses `size: 30px`;
  each `SymbolButton` has a full-size `TouchArea`, not a glyph-sized one. The
  hitbox concern is resolved; 30px versus 32px in Full is a density choice.
- **RB-VERIF-02, state layers.** Present: `PillButton`, `SymbolButton`, and
  `ToggleSwitch` use on-surface 8% hover and 12% pressed layers.
- **RB-VERIF-03, keyboard focus.** A real gap. Only `ToggleSwitch`,
  `OutputView`, and `CaptionButton` contain a `FocusScope`. `PillButton`,
  `SymbolButton` (including every action button), Settings navigation rows, and
  History rows cannot receive keyboard focus and draw no focus indicator; they
  are reachable only through accessibility actions. This is the most valuable
  finding in the package and is broader than Settings.

### Supported findings, assessed

- **RB-CONF-01, Settings width.** Consistent with the existing grouping
  direction. Filling the right side with a "system status summary" panel is a
  new feature, not a consequence of the layout problem; a constrained content
  width or grouped containers address the imbalance without new content.
- **RB-CONF-02, console toolbar close to the caption.** The Full caption is a
  custom 36px `TitleBar` drawn over the native frame
  (`src/platform/chrome.rs`), so the spacing is RunBay's own choice. The claimed
  2-4px gap and misclick risk are not measured; worth checking in a screenshot,
  low priority.
- **RB-CONF-03, History filters.** Already decided above (chips for status and
  source, one dropdown row for app and action).
- **RB-CONF-04 / RB-CONF-05, row alignment.** The Updates row places the switch
  and Check now side by side inside one `SettingRow`; separating the trailing
  switch from the manual check is a reasonable part of the Settings grouping
  work.
- **RB-CONF-06, MCP permission context.** The Agents page already has the hint
  "What coding agents may do through RunBay.exe --mcp." and a Copy MCP setup
  instructions button. A short explanation of what Ask first and Unrestricted
  allow is a reasonable text improvement; a warning banner is optional.

### Preferences

- **RB-PREF-01, grouped cards.** Same as possible direction item 1.
- **RB-PREF-02, segmented retention mode.** Fits: three fixed modes, preserving
  Off / Last N / Forever semantics. The count field would then be shown only for
  Last N.
- **RB-PREF-03, JSON snippet.** Partly exists as copied setup instructions.
  Showing a client-specific config file name would tie the UI to one agent
  product; keep the instructions generic.

### Remaining caveats

- "Full-screen dialog" is the report's label for Add app; RunBay draws it as a
  `DialogFrame` page inside the window. The Save/Cancel conclusion still holds.
- The closing line claims alignment with "the RunBay development team"; no such
  joint review took place. Treat it as the reviewer's own conclusion.

## Possible direction

For discussion, prioritize composition before decorative shape changes:

1. Group related settings into titled tonal containers, for example console
   appearance, floating-window behavior, and taskbar visibility.
2. Establish clearer page, section, and setting typography roles.
3. Put switches beside their labels where width permits, keeping explanations
   with the associated setting and preserving a narrow-width layout.
4. Give selection controls comfortable bounds and consistent styling; consider
   a visible choice group for short option lists when space supports it.
5. Use shape variation and short interaction feedback where they communicate
   selection or priority, while preserving readability, keyboard access,
   contrast, and idle performance.

## Decisions made so far

- 2026-10-25: No redesign has been approved. Keep this evaluation in discussion;
  no UI behavior or specification changes are made by this note. The user's
  message presents a critique for evaluation rather than requesting code edits.
- 2026-10-25: **Settings heading levels.** Keep "Settings" as the page title
  (title-large) and demote the category heading in the content panel to
  title-medium, keeping the category description. Reason: both headings
  currently use title-large 600 and compete; the words differ, so the problem is
  equal weight rather than duplication. Smallest change that keeps context.
- 2026-10-25: **History filters.** Status and source become single-select
  filter chips (short, fixed lists); app and action stay as dropdowns on one row
  (dynamic, potentially long lists). Reason: chips show the active state at a
  glance for short lists without hiding long app/action lists in chip rows.
- 2026-10-25: **Keyboard focus stays in this topic.** Keyboard focus and a
  visible focus indicator for `PillButton`, `SymbolButton` (including action
  buttons), Settings navigation rows, and History rows are handled here rather
  than in a separate accessibility note, even though they affect every screen.
  Reason: user decision; the work touches the same shared controls in
  `ui/components.slint` as the Settings changes.
- 2026-10-25: **History status chips without "All".** The History panel is a
  fixed 372px (about 348px of content); five standard chips need about 400px.
  Drop the "All" chip: no selected chip means every status (or source).
  Clicking the selected chip again clears it. Four status chips fit one row;
  source chips (Clicks, Agents) take a second row.
- 2026-10-25: **Selected chip style.** Filled `secondary-container` background,
  no check icon, so chip width does not change on selection and the row does
  not shift.
- 2026-10-25: **Retention mode as a segmented button.** Off / Last N / Forever
  becomes a three-segment control; the run count field appears only for Last N
  and has no clear (X) button. Existing retention semantics are unchanged.
- 2026-10-25: **Scope includes the Add/Edit app editor.** Besides Settings,
  History filters, and keyboard focus, the topic covers consistent field
  heights, labels, and grouping in the app editor. Reason: user decision, so
  shared form controls are designed once for both pages.
- 2026-10-25: **Follow the M3 Expressive component specifications.** The user
  required the revision to match the M3 Expressive specification. Checked
  against the official material-web tokens and the Material Components Android
  button group documentation; this changed two earlier decisions:
  - Segmented buttons are deprecated in M3 Expressive. The retention control
    becomes a **connected button group** (40px, 2px gap, outer full corners,
    inner 8px, selected segment turns into a pill), replacing the
    segmented-button decision above.
  - M3 filter chips show a check icon when selected. The selected chip now
    shows it, replacing the "no check icon" decision above, and the History
    panel widens from 372px to 400px so four status chips still fit one row.
- 2026-10-25: **Plan decisions.** Own components rather than the Slint Material
  Components library; M3 density -2 for lists, navigation items, and fields;
  Settings groups as Expressive segmented lists with trailing controls and a
  720px left-aligned content column; 48px pill navigation items; keep three
  editor columns with sections and M3 button hierarchy; std-widgets inputs
  measured first; Up/Down keyboard navigation in Settings navigation and
  History; motion only on interaction; Compact action targets 32px; console
  toolbar at least 8px below the caption; user does the visual check; one
  commit on `work-bench`. Details are final in the plan.

## Open questions

- None blocking. Density was settled as M3 density -2 in the plan.

## References

- [Google Design: Better, Easier, Emotional UX](https://design.google/library/expressive-material-design-google-research)
- [Material: Get started, including M3 Expressive](https://m3.material.io/get-started)
- [Slint: Window](https://docs.slint.dev/latest/docs/slint/reference/window/window/)
- [W3C: Understanding Contrast (Minimum), including antialiasing guidance](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html)
- Existing implementation: `ui/pages.slint`, `ui/theme.slint`,
  `ui/components.slint`.
- Existing product design intent: `doc/specification.md`, section 12.
