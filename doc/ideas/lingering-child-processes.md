# Runs kept active by lingering background processes

- **Date:** 2026-10-24
- **Status:** discussion

## Context

A run ends when every process in its job object has exited (specification §5,
"run lasts while started process lives"). This is what makes Run actions that start
a GUI app or a server stay active.

During a Rust release build, the MSVC linker started `VCTIP.EXE` (Visual C++
telemetry). The build script printed its result and exited with code 0, but
`VCTIP.EXE` stayed alive inside the job for more than ten minutes, so RunBay kept
showing the build as running. Stopping `VCTIP.EXE` let the run finish as success.

The RunBay app's own actions now stop `VCTIP.EXE` instances they started, but any
user's build action that links with MSVC can hit the same problem.

## Possible directions

- For Build and Custom actions, end the run when the script process exits and
  leave (or terminate) remaining job processes; keep the current rule for Run
  actions, whose purpose is to keep processes alive.
- A per-action option: "Finish when the script exits" versus "Finish when all
  started processes exit".
- A built-in ignore list of known background helpers (`VCTIP.EXE`,
  `mspdbsrv.exe`, `vctip`, compiler servers) that do not keep a run active.
- Show which processes are still alive after the script exited (tooltip or output
  line), so users understand why a run has not finished.

## Decisions so far

- None yet.

## Open questions

- Should leftover processes be terminated or detached when a build run ends?
- Is a per-action option needed, or is a kind-based default enough?
