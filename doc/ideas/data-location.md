# Data folder migration, restart, and Git ignore rules

- **Date:** 2026-10-24
- **Status:** discussion

## Context

Settings can select a custom data folder, including one inside a Git repository.
The choice takes effect after restarting RunBay. Existing data stays where it is;
the current feature does not migrate data or restart automatically. The location
pointer stays in the default data folder next to the executable. See the
[specification](../specification.md#4-data-folder-layout) for the current behavior.

## Decisions so far

- 2026-10-24: Keep the location pointer in the default data folder so startup and
  the MCP bridge can find the selected folder before opening its settings.
- 2026-10-24: Apply a location change only after restarting, without moving or
  copying existing data. This keeps the current session on one data folder and
  leaves the existing files intact.
- 2026-10-24: Lock the location controls when `--data-dir` is supplied, so a test
  instance cannot change the saved location of the normal instance.
- 2026-10-24: Keep migration, a restart button, and Git ignore assistance under
  discussion, separate from the implemented folder selection.

## Ideas under discussion

### Move or copy existing data

Offer to move or copy the current data when selecting a different folder, with
an explicit choice to use the destination's existing contents instead. A design
needs to cover active runs, destination conflicts, permissions, partial failures,
and recovery before a migration can be implemented.

### Restart button

Offer a restart button when the saved folder differs from the active one. Decide
how to handle active runs, preserve launch options and window state, and report
startup failures before adding this action.

### Git ignore guidance

When the data folder is versioned, app definitions, scripts, icons, groups, and
selected settings may be useful to track. Generated or machine-specific files
should usually be ignored. Candidate rules below assume a `.gitignore` placed
at the root of the data folder; a repository-root file would need the data
folder's relative path as a prefix.

```gitignore
/meta/window-state.json
/meta/update*
/apps/*/history/
/meta/*.log
```

| Pattern | Reason |
| --- | --- |
| `/meta/window-state.json` | Window positions, sizes, and last display mode are local to a machine. |
| `/meta/update*` | Update metadata and downloads are generated locally. |
| `/apps/*/history/` | Run summaries and output logs change on every run and may contain local paths or sensitive output. |
| `/meta/*.log` | Diagnostic logs, including `runbay.log` and `runbay.old.log`, are local runtime output. |

These are proposed rules, not files that RunBay creates or edits automatically.
The location pointer itself contains a local path and stays outside the custom
folder; if the default folder is also versioned, consider ignoring
`/meta/data-location.json` there.

## Open questions

- Should migration offer both move and copy, and when should the operation run?
- How should an existing destination be handled: reject, merge, or explicitly
  replace selected files? What backup and rollback guarantees are needed?
- Should restart wait for active runs or offer to stop them with confirmation?
- Should Git ignore guidance remain documentation, or should RunBay offer a
  preview before creating or updating a `.gitignore`?
- Which settings should be shared across machines, given that app definitions
  and settings may contain machine-specific paths and preferences?

## Resulting plan

None yet; this topic remains under discussion.
