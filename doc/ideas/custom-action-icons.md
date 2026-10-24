# Custom image icons for actions

- **Date:** 2026-10-24
- **Status:** planned (implemented directly at the user's request, 2026-10-24; no separate plan file)

## Context

Action icons are chosen from a fixed list of Material Symbols (specification §4.1).
Users who add actions such as "Open in IDE" may want the real logo of their editor
(for example an IDE or code editor installed on their machine). Brand logos cannot be
bundled: the repository only ships Material Symbols Rounded (Apache-2.0), stays
product-neutral, and editor logos are trademarks with their own brand guidelines.

A user-selected image avoids that: the image lives only in the user's data folder,
the same way app icons do (`data/apps/<app-id>/icon.<ext>`).

## Decisions so far

- 2026-10-24: brand logos are not bundled. Generic Material Symbols (`code`,
  `integration_instructions`, `code_blocks`, `data_object`) were added for IDE-style
  actions instead.
- 2026-10-24: the user asked to implement custom images directly. Decisions taken:
  - A separate `iconFile` field (not `icon: "file:<ext>"`), so older versions simply
    ignore it and keep showing the symbol.
  - One image per action, stored as `actions/<action-id>.icon.<ext>` next to the
    scripts, so agent undo snapshots of the app folder include it. No shared icon
    library.
  - The image is drawn in full color with no extra background; the symbol picker is
    disabled while an image is set and stays as the fallback after removing it.
  - The editor offers **Image…** / **Remove image**; removing an action deletes its
    image. MCP gets `set_action_icon` (empty path removes), and `delete_action`
    deletes the image too.
  - No shortcut to reuse the app's own icon.

## Open questions

- None blocking. Revisit a shared icon library if users repeat the same image
  across many actions.
