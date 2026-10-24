# CLAUDE.md

Instructions for Claude Code (and other coding agents) working in this repository.

## Project

**RunBay** is a small Windows tool that registers applications and runs their
build/run/custom actions (PowerShell scripts), showing and storing the output of
every run. Stack: **Rust + Slint**, design language **Material 3 Expressive**.

The specification is [doc/specification.md](doc/specification.md). Read it before
implementing or changing behavior; when a decision changes, update the
specification in the same change.

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
  Ask the user before adding a dependency under any other license.
- New source files do not need per-file license headers; the root `LICENSE` file
  covers the repository.

## Git

- Commit or push only when the user asks.
- Commit messages are in English, with a short imperative title (e.g.
  `Add Job Object based process runner`).

## Code conventions

- Follow `rustfmt` defaults and keep `cargo clippy` free of warnings.
- Public items get English doc comments that explain what they do and why.
- Respect the performance targets in the specification (fast startup, low
  memory, no redraw or polling while idle).
