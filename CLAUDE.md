# CLAUDE.md

Instructions for Claude Code (and other coding agents) working in this repository.

## Project

**RunBay** is a small Windows tool that registers applications and runs their
build/run/custom actions (PowerShell scripts), showing and storing the output of
every run. Stack: **Rust + Slint**, design language **Material 3 Expressive**.

The specification is [doc/specification.md](doc/specification.md). Read it before
implementing or changing behavior; when a decision changes, update the
specification in the same change.

## Ideas → plan → execution

- Ideas that are **still under discussion and not decided** are written in
  `doc/ideas/<topic>.md`, not in `plan/` or the specification. An idea note is
  not a work order: agents never implement it.
- **Every suggestion or idea from an agent** (improvements, refactors, features,
  findings nobody asked for, follow-up proposals at the end of a report) is also
  written to `doc/ideas/` instead of being implemented directly or written as a
  plan. Exception: when the user **directly asks for a code change**, do it
  without going through idea/plan; extra suggestions that come up during that
  work are still recorded in `doc/ideas/`.
- Each note contains: **date**, **status** (`discussion`, `considering`,
  `planned`, or `rejected`), the **decisions made so far** with their reasons and
  dates, the **open questions**, and a **link to the resulting plan** once
  promoted. When the discussion continues, update the same note (new decisions
  with their date); do not create a second note for the same topic. Old notes are
  never deleted, including `rejected` ones, so the reasons stay on record.
- Lifecycle:
  1. An **idea** in `doc/ideas/` is discussed until it is mature: decisions are
     clear and no open question blocks it.
  2. Only then is a **plan** written in `plan/unexecuted/<topic>.md`; the idea's
     status becomes `planned`, with a link to the plan. Before writing the plan,
     ask the user every decision it needs and record the answers in the plan as
     final, so execution needs no further questions.
  3. A plan is executed only when the user asks for it, then moved to
     `plan/executed/`. Decisions that change product behavior are carried into
     [doc/specification.md](doc/specification.md) in the same change.
- One topic uses one idea note and one plan file; do not split a topic into
  several files per phase or part.
- Idea notes and plans are written in **English** and follow the open-source rules
  below (no private data, product-neutral), because they are published with the
  repository.

## Open source — MIT license

RunBay will be published as open source under the **MIT license**.

- Everything that lands in the repository is written in **English**:
  - code identifiers, string literals, log and error messages, UI text;
  - code comments and doc comments (`///`, `//!`, and any XML/help text);
  - commit messages (title and body);
  - README, documentation, and other files meant for contributors.
- Communication with the user in chat stays in Bahasa Indonesia, as set by the
  user's global instructions; only the repository content is English.
- Keep the repository product-neutral and free of private data: no internal
  company or product names, server addresses, database names, credentials,
  tokens, or paths tied to a specific machine. Examples and sample `data/`
  content use placeholders (e.g. `C:\src\my-app`).
- New dependencies must have an MIT-compatible license (MIT, Apache-2.0, BSD,
  ISC, Zlib, and similar). Slint is used under its royalty-free license, which
  requires the "Made with Slint" attribution in the About dialog; do not remove it.
  UI icons use **Material Symbols Rounded** (Apache-2.0) as SVG files of only the
  icons in use; keep its license text in `THIRD-PARTY-NOTICES.md`. Do not add
  other icon sets.
  Ask the user before adding a dependency under any other license.
- New source files do not need per-file license headers; the root `LICENSE` file
  covers the repository.

## Git, branches, and remotes

- Daily work happens on branch `work-bench`, tracking `private/work-bench` on the
  private repository `MatrixCode-ID/RunBay-Private` (remote `private`).
- The public repository `MatrixCode-ID/RunBay` (remote `origin`) holds only `main`;
  `main` tracks `origin/main`. Never push `work-bench` to `origin`.
- Publishing: merge `work-bench` into `main`, then push `main` to `origin`.
- Commit or push only when the user asks.
- Commit messages are in English, with a short imperative title (e.g.
  `Add Job Object based process runner`).

## Releases

- A push to `main` that changes `src/**`, `Cargo.toml`, or `Cargo.lock` runs
  `.github/workflows/release.yml`, which publishes the version of the single
  untagged release note in `doc/ReleaseNote/<version>.md`.
- Every release needs its release note; follow
  [doc/ReleaseNote/README.md](doc/ReleaseNote/README.md). Add the note and raise
  `version` in `Cargo.toml` in the same change, and only when the user asks for a
  release. One untagged release note per push to `main`.
- Never create or push `v*` tags by hand; the workflow does it.

## Code conventions

- Follow `rustfmt` defaults and keep `cargo clippy` free of warnings.
- Public items get English doc comments that explain what they do and why.
- Respect the performance targets in the specification (fast startup, low
  memory, no redraw or polling while idle).
