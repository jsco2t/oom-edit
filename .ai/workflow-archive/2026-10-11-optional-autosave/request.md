# Autosave request

## Complete user invocation

`$feature-workflow ~Developer/sources/personal/projects/oom-edit/pipelines - I need you do add an optional new feature which will perform auto-saves using two options A) every 5min, B) when focus is lost (if its possible to support this)`

## Requested outcome

Add optional autosave with two options:

- A: save every five minutes.
- B: save when focus is lost, if focus loss can be supported.

## Workflow location

The supplied location was interpreted as `/home/duser/Developer/sources/personal/projects/oom-edit/pipelines`; it does not currently exist. A destination clarification was requested. Pending a different user preference, workflow metadata uses this repository's `.ai/workflow/`, matching the explicitly invoked skill and its approval/archive helpers. No files have been written to the supplied external path.

## Planning assumptions for approval

The two options are independently configurable, may be combined, and default to disabled. Each trigger considers all modified tabs with a named file path. Unnamed buffers require an explicit save. Focus-loss support relies on terminal focus reports in standalone use and host focus transitions in embedded use. Autosave writes the normal document path, using existing atomic saves and external-change protection; it does not create recovery files.

## Approved scope extension — 2026-10-12

User: "extension is approved."

Approval answers the request to extend this package to fix the existing core incremental-highlighting mismatch exposed by the final gate and rerun final validation. The original autosave scope remains approved.

## Acceptance follow-up — Round 2

Complete feedback:

> this is absolutely not acceptable - we spent nearly a week working on perf. $feature-workflow revise

Context: the requested local review and performance comparison identified a regression introduced by the approved highlighting repair. The user rejects that regression and requests revision of this existing package. Restore established editing performance while preserving correct incremental highlighting and the complete autosave feature. Existing performance budgets and correctness coverage remain binding.

Correction to earlier acceptance evidence: Round 1's passing make check and syntax/performance-path tests did not establish release latency equivalence. Subsequent make bench fails the edit-at-start budget; same-host TUI and public-pane comparisons show substantial regressions even with autosave disabled. Round 1 records remain historical evidence, superseded for current acceptance by the Round 2 criteria.
