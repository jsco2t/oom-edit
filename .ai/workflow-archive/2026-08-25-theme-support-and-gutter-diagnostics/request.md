# Theme Support and Gutter Diagnostics

Invocation:

`$feature-workflow /Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/prd.md`

Implement the complete work package defined by the supplied PRD, **Theme Support Solution Research**, including its appended test plan and definition of done. The PRD at the supplied absolute path is the authoritative detailed requirement source for this workflow. Its requirements include the owned theme catalog, five attributed bundled dark themes, strict user-authored themes, independent gutter theming, asynchronously projected Trouble markers, performance infrastructure and retained baseline/candidate evidence, licensing and notice compliance, documentation, and the full repository quality gate.

Planning snapshot:

- Source: `/Users/jason/Developer/sources/personal/notebook/projects/oom-edit/features/04-themes/prd.md`
- Snapshot date: 2026-08-25
- Source SHA-256: `720af6cf820c6befe8bb834e5a138cbd648b114f122c614f1a4aa80b3f7e8e12`
- The approved workflow plan and task documents preserve the executable scope and acceptance criteria derived from this PRD.

## Plan revision 2 — Rendered performance contract

The initial five-trial TUI baseline showed that its `open-first-frame` case measured cold rendered Normal mode while applying NFR-1's 150 ms source-frame ceiling. The human confirmed that both workloads are valid and must remain separate requirements.

- Preserve NFR-1 unchanged: construction plus the first highlighted 1 MiB source viewport frame remains below 150 ms.
- Extend rendered-layout performance coverage with a 1 MiB large-document contract: cold full layout below 250 ms worst, end-to-end rendered first frame below 350 ms worst, retained layout heap at or below 64 MiB, and one-shot process peak RSS at or below 192 MiB on the baseline machine.
- Enforce near-linear size scaling: each fixture doubling from 256 KiB through 512 KiB to 1 MiB may increase cold-layout time or retained heap by at most 2.25×.
- Document these targets and keep make-owned automated enforcement.
- Preserve the original failed evidence as pre-optimization diagnostic history. After the localized rendered-line-number performance repair, capture a new five-trial pre-theme/gutter baseline for final comparison.

## Acceptance follow-up — Round 2

I need to make one change in how the current Gutter works.

Right now the layout is something like this:

```text
  73 |Line of text
```

And if the Gutter has a warning - it looks like this:

```text
  73W|Line of text
```

Here is how I would like that to look:

```text
W 73 |Line of text
```

## Acceptance follow-up — Round 3

This very close - but I think we can save some gutter space by reducing the right side padding/margin around the line number.

Right now the layout is something like this:

```text
  73  |Line of text
```

I want part of that right side gap removed:

```text
  73 |Line of text
```

No change is requested to the diagnostic marker. It's fine the way it is.
