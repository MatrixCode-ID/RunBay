# Plan: Memory footprint

- **Date:** 2026-10-24
- **Status:** not executed — decisions below are the executor's proposals and must be
  confirmed by the user before execution.
- **Idea:** [doc/ideas/memory-footprint.md](../../doc/ideas/memory-footprint.md)
- **Specification:** §11 (non-functional targets), §12 (renderer).

## 1. Goal

Bring the idle working set to < 30 MB in Icon/Compact mode and < 60 MB in Full mode
with 10,000 output lines (specification §11), or document the measured lower bound
and change the targets with the user.

## 2. Proposed decisions (to confirm)

| Topic | Proposal |
| --- | --- |
| Metric | Working set after 10 s idle, release build, measured by a script in `scripts/measure.ps1` (start, wait for the window, idle, read `WorkingSet64` and CPU time) |
| Renderer | Measure femtovg and the software renderer; keep femtovg as default only if it can meet the targets, otherwise make software the default and femtovg an option |
| Lazy windows | Create the Compact, Icon, and Icon popup windows on first use instead of at startup |
| Output model | Keep the 10,000-line ring buffer; store lines as parsed spans only for the visible run |
| Release workflow | No change |

## 3. Steps

1. Add `scripts/measure.ps1` and a development option `RUNBAY_RENDERER=software|femtovg`.
2. Measure the baseline per mode (Full empty, Full with 10,000 lines, Compact, Icon)
   for both renderers; record the numbers in this plan.
3. Create the mode windows lazily; measure again.
4. Apply the renderer decision; measure again.
5. Reduce output model memory if Full with 10,000 lines is still above target.
6. Update specification §11/§12 with the results and any changed target.

## 4. Done when

The targets are met, or the measured values, causes, and options are documented and
the user decided on new targets. `cargo test`, clippy, and fmt stay clean.
