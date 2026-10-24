# Release notes

Every RunBay version has one release note: `doc/ReleaseNote/<version>.md`, e.g.
`1.0.0-alpha.1.md`. The file name is the source of the release version.

## How a release happens

1. On `work-bench`, prepare the release note of the next version in
   `doc/ReleaseNote-Candidate/<version>.md` and keep it up to date while features
   are added. It collects every user-facing change since the previous release
   (the latest `v*` tag). When no candidate exists, it is created with the version
   of the latest `v*` tag raised by one in the same stage (e.g. `1.0.0-alpha.1` →
   `1.0.0-alpha.2`; after a final release, PATCH + 1). The candidate folder always
   exists (kept with a `.gitkeep`) and does not start the Release workflow.
2. When the version is ready to release, move the note from
   `doc/ReleaseNote-Candidate/` to `doc/ReleaseNote/`. Raise `version` in
   `Cargo.toml` to the same version in the same change.
3. Merge `work-bench` into `main` and push `main` to `origin`.
4. When the push changes `doc/ReleaseNote/**`, the
   [Release workflow](../../.github/workflows/release.yml) takes the single release
   note without a `v<version>` tag, runs CI, builds `RunBay.exe` with that version, tags
   `v<version>`, and publishes a GitHub Release with `RunBay.exe` and `SHA256SUMS`.
   The release note becomes the release body.

A push without a release note change does not start the workflow, and a push that
only edits an already tagged release note publishes nothing. The workflow fails when
more than one release note is untagged, when the version is not higher than the latest
tag, or when `## Summary` is empty.

## Versions

`MAJOR.MINOR.PATCH-alpha.N`, `MAJOR.MINOR.PATCH-beta.N`, or `MAJOR.MINOR.PATCH`.
Stages run alpha → beta → release; there are no channels. Alpha and beta releases are
marked as pre-releases on GitHub. The first version is `1.0.0-alpha.1`.

## Format

```markdown
# RunBay 1.0.0-alpha.2

Date: 2026-11-01
Previous: 1.0.0-alpha.1

## Summary
One short paragraph describing the release.

## New features
- ...

## Fixes
- ...

## Upgrade notes
- ...
```

`## Summary` is required and must not be empty; the other sections are optional and
omitted when empty. Release notes are written in English.
