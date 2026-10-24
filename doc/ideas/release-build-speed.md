# Faster release workflow

- **Date:** 2026-10-24
- **Status:** considering

## Context

The 1.0.0-alpha.2 release run took about 18 minutes end to end:

| Step | Time |
| --- | --- |
| validate | 10 s |
| CI: cache restore | 45 s |
| CI: fmt | 5 s |
| CI: clippy | 1 min 35 s |
| CI: test | 1 min 51 s |
| release: `cargo build --release` | **12 min 27 s** |
| release: assets, upload, tag, publish | 15 s |

Clippy is not the bottleneck. The release build is, because:

- the `release` job has no `Swatinem/rust-cache`, so every dependency (Slint,
  femtovg, resvg, image codecs, ...) is compiled from scratch at `opt-level = 3`;
- `[profile.release]` uses `lto = true` (fat LTO) and `codegen-units = 1`, which
  makes the final optimization and link single-threaded and slow;
- the build waits for the CI job (about 4.5 minutes) although it does not need
  its result until publishing.

A push to `main` that changes code and a release note also runs `ci.yml` from its
own push trigger, so the same checks run twice (in parallel, costing runner time
but not wall-clock time).

## Possible directions

- **Build in parallel with CI**: start the release build right after `validate`
  and gate only the tag/publish step on CI. Saves about 4.5 minutes of
  wall-clock time with no change in safety.
- **Cache the release build**: add `Swatinem/rust-cache` with a release key to
  the `release` job. Dependencies are reused between releases; only RunBay and
  the LTO link are rebuilt.
- **Lighter LTO**: `lto = "thin"` and/or more `codegen-units` build much faster
  at the cost of a slightly larger and possibly slower executable; this touches
  the size and startup targets in the specification, so it needs measuring.
- **Skip the duplicate CI**: when the push already ran `ci.yml` successfully for
  the same commit, the release could reuse that result instead of calling CI
  again.
- **Faster local builds**: a separate Cargo profile for local publishing (for
  example `local-release` inheriting `release` with `lto = "thin"` and
  `codegen-units = 16`), used by `scripts/publish-local` and the RunBay app's
  Release Win64 action. The local release build took 4 min 44 s with fat LTO even
  with dependencies cached. GitHub releases would keep the current profile.
- Keep clippy: it takes about 1.5 minutes and guards the "no clippy warnings"
  rule on what is published.

## Decisions so far

- 2026-10-24 (user): build the release in parallel with CI (`build` job) and gate
  only the tag/publish step (`publish` job) on CI and the build. Done directly at
  the user's request.
- 2026-10-24 (user): cache the release build with `Swatinem/rust-cache` (`key:
  release`). Done directly at the user's request.
- 2026-10-24 (user): keep `lto = true` and `codegen-units = 1` for now; revisit after
  measuring the effect of the cache.
- 2026-10-24: keep clippy in the release CI.

## Open questions

- Should local publishing use a faster profile than GitHub releases, and should
  the local Release action keep running fmt, clippy, and tests?
- Is a larger executable acceptable for faster releases (thin LTO), or should the
  release profile stay as is?
- Should the release reuse the push's CI result, or always run its own?
