# Memory footprint

- **Date:** 2026-10-24
- **Status:** planned — [plan/unexecuted/memory-footprint.md](../../plan/unexecuted/memory-footprint.md)

## Context

The specification (§11) targets < 30 MB working set idle in Icon/Compact mode and
< 60 MB in Full mode with 10,000 output lines. A first measurement of the v1 release
build (2026-10-24, femtovg renderer, Full mode, one app, idle) gave a working set of
**171.6 MB**. Startup (process start until the window exists) was 383 ms cold and
about 30 ms warm; idle CPU was 0 ms over 10 s, so the CPU target is met.

## Decisions so far

- 2026-10-24 (user): no memory optimization during the v1 plan execution; it is
  handled by its own plan.

## Suspected causes (not verified)

- The OpenGL driver loaded for femtovg (GPU drivers map large DLLs and buffers).
- Four Slint windows created at startup (Full, Compact, Icon, Icon popup).
- The tokio runtime and rustls are linked in but only used by `--mcp` and update
  checks; they should not cost memory in the GUI unless initialized.

## Open questions

- Is the software renderer acceptable as the default when it meets the targets,
  with femtovg only as an option?
- Should the 30/60 MB targets count private bytes instead of the working set?
