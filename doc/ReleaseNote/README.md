# Release notes

Every RunBay version has one release note: `doc/ReleaseNote/<version>.md`, e.g.
`1.0.0-alpha.1.md`. The file name is the source of the release version.

## How a release happens

1. On `work-bench`, change the application and add the release note of the next
   version. Raise `version` in `Cargo.toml` to the same version in the same change.
2. Merge `work-bench` into `main` and push `main` to `origin`.
3. When the push changes `src/**`, `Cargo.toml`, or `Cargo.lock`, the
   [Release workflow](../../.github/workflows/release.yml) takes the single release
   note without a `v<version>` tag, builds `RunBay.exe` with that version, tags
   `v<version>`, and publishes a GitHub Release with `RunBay.exe` and `SHA256SUMS`.
   The release note becomes the release body.

Without an untagged release note the push publishes nothing. The workflow fails when
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
