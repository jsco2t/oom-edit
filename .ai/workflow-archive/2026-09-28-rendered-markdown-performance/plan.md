# Rendered Markdown performance

## Objective

Remove the confirmed query-compilation and cursor-mapping pathologies, address the related large-fence and unnecessary save costs, and enforce realistic first-frame performance through the public editor pane while retaining standalone and embedded fidelity. The approved delivery must make the specified representative notes pass their performance gates on this machine and preserve exact rendering/source provenance.

## Current behavior

The detailed findings and reproduction commands are in [evidence/investigation.md](evidence/investigation.md). The report is accurate for this revision and machine: the current 350 ms first-render gate uses a 1 MiB prose fixture, whereas a 16 KiB many-fence fixture takes about 393 ms and a 1 MiB mixed fixture takes about 2.31 s through the public pane. The cursor penalty depends on whether its source offset has a rendered atom; it is not paid by every cursor position. Successful saves invalidate the layout even when the text is unchanged. The reported two-fix scratch build improves common notes but still takes 459 ms on 1 MiB mixed Markdown, so the two fixes alone are insufficient for that representative case.

`EditorSession::render_layout` owns the core layout cache and remaps the source cursor. `Highlighter` already owns an incremental source tree, physical line-start index and compiled injection-query cache. `EditorPane` and the standalone host both use the same private `App`, which calls the session synchronously. The public pane exposes no mutable session or bounded layout API. The existing release performance suite covers 5,000 rendered lines and a repeated-prose 1 MiB fixture; it does not cover fence-heavy, mixed, list or table shapes.

## Proposed implementation

1. Add deterministic, versioned, realistic fixtures and a same-machine public-pane release benchmark. Record cold one-process-per-sample first frames at 100×41 and 70×28; include construct, open, render and host cell copy. Add trigger/rebuild measurements, layout growth and exact output characterization. The benchmark is a new `make bench-realistic` target. Its performance assertions become a final package gate; its functional and shape tests enter `make test` immediately.
2. Route rendered fenced-code highlighting through the existing core `Highlighter` query cache, keyed by the canonical `LangDef`, so source and rendered views share compiled queries. Keep the cache document/session owned and bounded by the static language registry. Preserve unknown-language fallback, aliases and per-fence parser independence.
3. Use the existing maintained physical line index for rendered cursor fallback. Eliminate prefix newline recounting per rendered row. Preserve all atom-first and nearest-content tie-breaking behavior, including synthetic rows and UTF-8/CRLF offsets. Use property and regression tests to compare the new mapping with the old reference behavior.
4. Replace the all-spans-for-every-line scan in large snippet highlighting with an ordered span-to-line sweep that preserves capture priority. Retain parsed injection trees or equivalent versioned parse results inside `Highlighter` for repeated source frames, with mutation invalidation through `LiveDocument`. Measure retained heap and avoid a second mutable text owner.
5. Remove layout invalidation on successful saves that do not change text. Keep presentation-frame invalidation for dirty/status changes where needed, and verify all save variants, path retargeting and external-version protections.
6. Profile the remaining full-layout cost on the fixed mixed/table/list fixtures and optimize only measured pure core layout work (parser-leaf materialization, wrapping, tables or block assembly as indicated). Keep `BlockModel` and mapped-atom provenance as the sole render path. The final target is a 1 MiB mixed public-pane first frame below 350 ms, not merely the scratch two-fix result. If meeting that target requires viewport-lazy rendering, background layout, a new public API or a changed mode contract, stop under the workflow's plan-change rule for human review rather than making that architectural expansion implicitly.
7. Verify exact standalone/public-pane parity, core-only use, byte-exact provenance, interaction behavior and whole-package performance. Update `docs/performance.md` to distinguish the published fixed fixture from the new realistic-content gates and the measured limits of extreme content.

## Architectural decisions

- All parsing, query reuse, line indexing and rendered layout remain in `oom-edit-core`; `oom-edit` only measures/presents the same core output through the owned pane. No terminal dependency enters core, and no host-specific shortcut changes one usage model without the other.
- Extend `Highlighter`'s existing cache rather than creating a second global/thread-local query registry. `LiveDocument` remains the sole owner of mutable text and derived caches; all edit invalidation passes through its mutation gateway.
- `LangDef` stays the only grammar/query/alias registry. No dependency is planned. Any change to the public crate facades requires the exact compile-time API/privacy guards in the same task.
- Preserve eager, fully faithful rendered layout and the current four modes during this package. No loading placeholder may be counted as a first rendered frame. Core output must remain semantic and byte mapped; synthetic glyphs remain source-less.
- A successful save changes document metadata and dirty state, not Markdown layout. Failed/uncertain saves retain the existing no-data-loss behavior.
- Use the repository's existing golden, property, conformance and host-parity infrastructure. Tests must compare semantics and provenance, not only rendered line counts or retained heap.

## Work included

- R1: repeated code-fence highlight-query compilation.
- R3: cursor remap's repeated source-prefix scan and its cursor-position-sensitive stalls.
- R2: superlinear capture-to-line assignment for one large highlighted fence.
- R4: repeated full-fence source-view parsing during viewport redraw.
- Unnecessary rendered-layout rebuild after successful clean or dirty saves.
- Realistic cold open, edit-to-rendered, width-change, save and reload benchmarks through `EditorPane`, with standalone/core parity tests.
- Measured remaining mixed/table/list layout work needed for the objective's representative performance envelope.

## Task sequence

1. [001 — Reproducible performance fixtures and public-pane harness](tasks/001-performance-harness.md)
2. [002 — Share compiled highlight queries](tasks/002-query-cache.md)
3. [003 — Indexed rendered cursor mapping](tasks/003-cursor-index.md)
4. [004 — Linear large-fence highlighting and source parse reuse](tasks/004-large-fence-highlighting.md)
5. [005 — Preserve layout across metadata-only saves](tasks/005-save-layout-cache.md)
6. [006 — Remaining full-layout efficiency](tasks/006-layout-efficiency.md)
7. [007 — Standalone and embedded acceptance](tasks/007-integration-acceptance.md)

Tasks are ordered so each measured optimization has a preceding regression harness and can pass the repository gate on its own. Task 001's performance recorder may report the known baseline failures; it does not claim a passing performance gate before fixes. Task 007 enables the final asserting threshold run.

## Quality gate

`make check` is the standard gate for every task. It runs the repository's format, lint, build, full tests, license/advisory and data-license checks. The final gate adds `make bench` for the existing release performance contracts, `make bench-realistic` for the new public-pane content classes, and `make test-public-api`, `make test-embedding-example`, and `make test-standalone-host` to make both usage models explicit. These commands are recorded in `gate.json`. No separate type checker exists for this Rust workspace; `make lint` and `make build` cover compilation/type checking. The new benchmark workflow gets a top-level Make target in the same change.

## Risks

- Absolute wall-time gates can vary with host contention. Use cold subprocesses, five trials, fixed fixtures, recorded host metadata, and the same machine for baseline/candidate comparisons. Structural regression tests carry the algorithmic guarantees independent of timings.
- The scratch two-fix patch checked line counts and heap only; its cursor and style semantics were not proven. Differential and property tests must close that gap before relying on its speed.
- Retaining injection parse trees can increase memory; measure RSS and retained layout heap, especially tables and large fences, and invalidate by exact text generation.
- A fully rendered 1 MiB mixed note may still exceed 350 ms after local optimizations. This objective remains required; an architectural expansion must trigger explicit plan review if needed.
- The neighboring project's temporary evidence may disappear. Versioned in-repo fixtures and runner make the gate reproducible without its scratch directory or private vault.

## Out of scope

- Modifying the integrating `oom` project, its dependency pin, or its private vault files.
- A universal 350 ms guarantee for every possible 1 MiB Markdown document. Pathological single-fence and thousands-of-fences cases receive explicit scaling and documented measurements; the required 350 ms class/size gates below define this package's envelope.
- Changing Markdown syntax, modal semantics, public host ownership, dependencies, themes or the four public modes.

## Final acceptance criteria

1. On this machine, the final `make bench-realistic` runner passes worst-of-five cold public-pane first frames below 350 ms at both 100×41 and 70×28 for: 1 MiB existing prose, 1 MiB mixed, 144 KiB mixed, 128 KiB single Rust fence, 128 KiB many small fences, 512 KiB lists, and 448 KiB tables. The runner includes host cell copy and records open/render components, process memory and fixture identity.
2. The same runner records full-layout time and retained heap at 256/512/1024 KiB for every class. No class has a 256→512 or 512→1024 time-growth ratio above 2.25 after removal of its known quadratic path; classes whose intrinsic parser complexity prevents this must cause a plan-change review, not a silently weakened gate.
3. Query compilation occurs at most once per canonical language per session/document generation, including repeated fences, width rebuilds and edits; source and rendered styles, unknown-language fallback and aliases match the baseline.
4. Cursor remapping on source-backed and source-less positions is byte/row exact against the baseline behavior for headings, lists, fences, tables, blank lines, wrapping, CRLF, Unicode and Select endpoints. Its fallback performs no repeated source-prefix newline scan; large-fixture mapping scales within the new structural/performance gate.
5. A successful save of unchanged text, including a clean save, does not rebuild `RenderedLayout`; the frame still reflects saved/dirty state. Edit, width change and reload still rebuild when required. Save failure and external-version protections remain intact.
6. Large-fence snippet span assignment and repeated source frames avoid whole-fence quadratic/reparse work. Existing highlight/style/provenance goldens and property tests pass; new tests cover long fences, edits inside/outside the fence and UTF-8 boundaries.
7. Both `EditorSession` direct hosts and `EditorPane` hosts retain identical editing/rendering semantics. The standalone binary and split-pane example pass their existing parity/public API tests, and the full `make check` plus existing `make bench` gates pass with no warnings.

## Revision 2 — authoritative interaction-first plan

This section supersedes the original task-006/007 strategy and original final
acceptance criteria where they conflict. Tasks 001–005 are complete and their
documents/evidence are unchanged. The previous plan was approved at
`2026-09-28T15:11:58Z` with SHA-256
`77a9375883b242d5a3cd6f1a17dfcd6a7fe53066a019e3ea0619d47a6aadb159`.
The five-trial cold gate then found 560–563 ms for 1 MiB mixed Markdown, and
the interaction profile found a 644 ms median rendered line delete. The
approved pure-layout scope could not plausibly meet its 350 ms mixed-note
target without changing architecture; see `evidence/006-profile.md` and
`evidence/interaction-profile.md`. The user explicitly authorized this
plan-change revision, **not** implementation before renewed approval.

### Revised objective and priorities

Make ordinary edits and scrolling of realistic 1 MiB notes promptly usable in
both standalone and embedded `oom-edit`, with complete current-text cells,
styles, selections, source-byte provenance and navigation after each operation.
The initial target is **under 50 ms p99 from input to completed public-pane
frame for ordinary local edits** on this machine. Preserve rapid scrolling and
measure mode transitions, resize, reload and cold open separately. Cold first
render should aim for approximately 500 ms, but never by worsening editing or
scrolling; the former uniform 350 ms realistic-content gate is superseded only
as specified below. There is no loading screen, pending mode, stale editable
layout, background-preparation lifecycle, or second mutable text owner.

### Revised architecture

`LiveDocument` remains the sole owner of mutable Vim text. Its mutation
gateway supplies exact sequential edits and completes the current-text source
analysis transaction. `EditorSession` uses that outcome to update a single
derived document-to-view projection before returning control: a retained
Markdown block model, explicit document-wide semantic dependencies, and
width-aware rendered block/row caches. The projection tracks affected source
spans, reused/rebuilt blocks, row counts and source ranges; an index supports
cursor mapping, viewport access and scroll geometry without a whole-document
walk on ordinary edits. A new frame is built from current-generation data
only. Current `EditorSession::render_layout` and `EditorPane` ownership remain
the shared core/pane paths; public API changes are allowed only if essential
and accompanied by compile-time API/privacy guards.

The source tree-sitter parser already receives an edited old tree, yet a 1 MiB
structural edit spent 176.5 ms parsing in the one-shot phase probe. The
rendered path independently rebuilds the entire pulldown block model and
mapped layout (69.4 + 153.3 ms in the cold profile). Therefore both source
analysis and rendered projection need bounded reprocessing. The feasibility
work must establish safe Markdown parse/invalidation boundaries rather than
assuming Tree-sitter `changed_ranges`, pulldown slice parsing or a cache alone
is sufficient. Document-wide definitions, links, footnotes, list/container
continuations, fence delimiters and front matter may expand dependencies; a
correct wider rebuild is preferable to stale output, with its latency measured
and disclosed. Do not add a parser/dependency or a second rendering path on
speculation.

For width changes, retain syntax/model work and rebuild width-dependent row
geometry only as needed while preserving exact row counts, cursor/selection
mapping and navigation. The current fully faithful builder remains an
independent test oracle, not a production fallback for ordinary local edits.
Any fallback for truly document-wide semantic changes must be explicit,
instrumented and covered by tests; it must not silently absorb common edits.

### Feasibility gate before the full refactor

First establish a versioned differential oracle and opt-in/bench-only work
counters. Measure input, source parse, injection/definition work, block-model
parse, layout, cursor mapping, frame copy, changed byte/line ranges, blocks and
rows rebuilt/reused, retained heap and peak RSS. Add an asserting public-pane
interaction target to `Makefile` alongside the existing recorder. Five fresh
processes × 20 cycles give at least 100 samples per step; report median,
p95, p99 and worst, with top and midpoint variants and fixture identity.

Then build a bounded feasibility prototype against the full-build oracle. It
must demonstrate exact source/rendered output for representative local line
delete/undo, source structural edit and fence-language edits, and show that
ordinary local changes reprocess a bounded region rather than the whole
1 MiB document. The prototype must provide measured evidence that its
remaining parser/model/layout work can plausibly reach the 50 ms end-to-end
target. If it cannot, stop at `PLAN_CHANGE_REQUIRED` with the failed
measurements and fidelity counterexamples; do not proceed to a larger refactor
or substitute loading/async styling without another human decision.

### Test-forward implementation sequence

6. [006 — Instrument interaction work and exact reference](tasks/006-layout-efficiency.md): extend existing fixture/public-pane probes; add internal changed-range, reuse and phase counters, deterministic differential reference vectors and runner tests. Capture a repeatable baseline before product algorithm changes.
7. [007 — Bounded incremental feasibility](tasks/007-integration-acceptance.md): prove or reject safe local parser/model/layout invalidation with a test-only prototype, exact comparison and same-machine release measurements. This is the explicit go/no-go gate.
8. [008 — Incremental source analysis transaction](tasks/008-incremental-source-analysis.md): integrate the proven bounded source update through `LiveDocument`, including injection/front-matter/reference invalidation and exact source highlighting.
9. [009 — Retained Markdown model and global dependencies](tasks/009-retained-block-model.md): integrate changed-block parsing, source-span rebasing and document-wide link/footnote/navigation dependency invalidation.
10. [010 — Indexed retained layout and viewport](tasks/010-retained-layout.md): reuse unchanged mapped rows, maintain row/source indexes, update rendered cursor/selection and width-dependent geometry without rebuilding the full document on ordinary edits.
11. [011 — Public-pane interaction and cold performance](tasks/011-performance-acceptance.md): make input-to-frame, scrolling, memory and cold-open gates assert the approved envelope; diagnose any measured regressions in the same core path.
12. [012 — Standalone and embedded fidelity acceptance](tasks/012-host-parity-acceptance.md): exhaustive differential, conformance, lifecycle, API/privacy and host-parity validation; update performance documentation.

Each task's production changes and tests land together. Tasks 006–010 are
main-only because they establish or modify architecture and correctness
boundaries. No task may mark a prototype as production behavior or defer
incomplete fidelity to a later package.

### Active quality gate

`make check` remains the standard gate after every task. The final gate is
`make check`, `make bench`, the revised `make bench-realistic`, a new asserting
`make bench-interactions`, `make test-public-api`,
`make test-embedding-example`, and `make test-standalone-host`. The interaction
command and its measurement tests must be added as top-level Make targets in
the same implementation change. Existing release smoke and supply-chain
checks are not weakened. No new dependency is planned; if one becomes
necessary, the project's license/advisory/vendor checklist applies.

### Active acceptance criteria

1. At 1 MiB mixed Markdown and 100×41, at least 100 release public-pane
   samples per case show **p99 <50 ms key-to-owned-frame** for ordinary local
   source typing, local source line insert/delete, rendered line delete/undo,
   and return from Insert after a local edit; repeat top and midpoint cases.
   Each operation verifies the intended text/mode change and a fully current
   frame. No interstitial frame, stale editable layout or unprocessed key is
   allowed. The benchmark records input and frame phases and worst samples.
2. Source and rendered wheel scrolling on the warmed 1 MiB note remain under
   25 ms p99 key-to-frame and actually change the visible viewport. Save of
   unchanged text retains its existing no-rebuild guarantee. Resize, reload,
   first frame and globally invalidating edits have separately reported
   distributions and no hidden 50 ms claim; their results may not regress
   more than 10% against the fixed task-006 same-machine baseline without
   explicit plan-change review.
3. The public-pane cold first frame passes the existing ≤350 ms cells that
   passed after task 005, and the 1 MiB mixed cell reaches <500 ms worst of
   five at both 100×41 and 70×28. The borderline 448 KiB table cell is
   measured and must not exceed its fixed task-006 worst by more than 10%; its
   previous 350 ms assertion is superseded. Cold timing is subordinate to
   interaction criteria: do not trade a failing interaction for a passing
   cold frame. Retained heap, peak RSS and layout scaling are recorded and
   compared with task-006 baseline; the existing fixed-fixture 64 MiB
   retained-heap and 192 MiB peak-RSS limits remain hard gates, and no >10%
   realistic-fixture RSS/heap regression is accepted without plan-change
   review.
4. Differential tests compare incremental output with a fresh full build for
   exact cells, semantic styles, source-byte atoms, line numbers, fence regions,
   front matter, links/footnotes/jump targets, cursor/selection endpoints,
   wrap widths and operation results. Include Unicode, CRLF, duplicate text,
   escapes/entities, tables, lists, nested containers, unknown/known code
   fences, definition edits, front-matter boundary edits and random edit/undo
   sequences. Property tests exercise rendered↔source mapping and incremental
   highlighting equivalence; meta-tests prevent loss of cases.
5. Test-only or opt-in counters prove ordinary local edits reuse unchanged
   source/model/layout regions rather than rebuilding the whole 1 MiB note.
   Globally invalidating edits may expand work but must produce the same exact
   frame and record that expansion. The full builder remains a test oracle.
6. `EditorSession`, `EditorPane`, standalone and split-pane embedded hosts use
   the same core implementation and preserve all four public modes, lifecycle,
   save/external-change safety, input routing, semantic styling and public API
   boundaries. The complete final gate passes without warnings.

### Risks and scope limits

Markdown is context-sensitive; closing/opening a fence or changing a global
definition may invalidate most of a document. The 50 ms target applies to
the named ordinary local interactions, not every adversarial edit. The exact
scope is enforced by deterministic fixtures, and broad invalidations are
still tested for fidelity and measured. A failed feasibility gate or an
unavoidable target/fidelity conflict requires a new human decision, not
silent target relaxation. This package does not modify the integrating `oom`
project, add a loading UI/background job protocol, introduce partial editable
rendering, or claim universal 1 MiB constant-time behavior.

## Revision 3 — first rendered projection versus warmed returns

This section supersedes only revision 2's classification of the first-ever
source-to-rendered return as an ordinary <50 ms mode return. Tasks 001–010 and
their evidence are complete and unchanged. Revision 2 was approved at
`2026-09-28T21:18:54Z` with SHA-256
`f64d8e8b8d40d3e8272a387f31cfd00594847f70a9ac0c18f634303007918db2`.
Task 011 stopped at `PLAN_CHANGE_REQUIRED` after the five-process record
showed one 223.9–229.4 ms first-ever source-to-rendered frame per process,
while all 95 subsequent returns at each top/midpoint position completed in
1.0–1.5 ms. The five-trial 1 MiB mixed cold open-to-rendered frame also took
528–536 ms, above the independently approved <500 ms limit. See
`evidence/011-plan-change.md`, `evidence/incremental-layout.jsonl` and
`evidence/011-cold-initial.jsonl`.

### Revised decision and scope

The first successful source-to-rendered projection in a freshly opened
source-first document is a **cold first render**, even if the user made a local
source edit before leaving Insert. Measure it separately from warmed returns.
This classification is available exactly once per fresh document; a slow
subsequent return, local edit, or rebuild cannot be reclassified as cold. It
does not add a placeholder, loading mode, stale editable frame, background
preparation, second mutable text owner, or host-specific rendering path. All
other revision-2 architecture, fidelity, interaction, cold-open, memory and
host-parity requirements remain active.

### Revised task-011 acceptance

1. For 1 MiB mixed Markdown at 100×41 and both top and midpoint cursor
   positions, run at least five fresh public-pane processes with at least 21
   verified source edit/Insert-return cycles each. The first return in each
   process must prove that no rendered projection existed beforehand and must
   yield a complete current-text rendered frame in <500 ms worst-of-five
   key-to-owned-frame. Report its input/frame phases and all five individual
   observations. Do not count source open time inside this transition metric;
   record open-to-first-render separately.
2. The remaining at least 100 **warmed** returns per top/midpoint case must
   verify the local edit, mode and complete current-text frame, then pass both
   p99 <50 ms and worst <50 ms key-to-owned-frame. No later return may take
   the cold allowance. All other named ordinary local edit and scroll gates
   retain their revision-2 thresholds and verified sample counts.
3. Runner contract tests must reject a missing first-ever or warmed sample,
   wrong trial/cycle classification, fewer than 100 warmed returns, any
   unverified state change, malformed timing, or a slow later return hidden in
   the cold bucket. `make bench-interactions` uses at least five fresh
   processes ×21 cycles. Document the two distributions distinctly.
4. The **separate** 1 MiB mixed open-to-first-render limit remains <500 ms
   worst-of-five at both approved pane sizes. The currently measured 528–536
   ms is not accepted or folded into the source-to-rendered allowance. Task
   011 must investigate and meet it without degrading ordinary interactions,
   fidelity, the already passing cold cells, memory, or source-first behavior.
   If that cannot be done inside the approved synchronous architecture, stop
   for another plan-change decision; do not silently raise the limit.

Task 011 and task 012 remain the only unfinished tasks. The quality gate in
`gate.json` is unchanged. A new human `$feature-workflow approve` is required
before product implementation resumes.

## Acceptance follow-up — Round 2: interaction fidelity on the 1 MiB kitchen sink

This section adds acceptance work after round 1's completion; it does not
rewrite that round's task documents or evidence. The user's direct comparison
with `main` supersedes the round-1 assertion that the tested scrolling cases
adequately represented scrolling UX. The new `examples/kitchen-sink-1mb.md`
contains two approximately 390 KiB code fences and is a required fixture, not
an optional pathological case. Responsiveness and correct Select behavior
remain more important than reducing the cold first frame.

### Findings and architectural assessment

- The previous interaction runner sends one wheel-down and one wheel-up event
  per cycle on generated `mixed` content, immediately reversing position. It
  times `EditorPane::handle_input` and owned-frame construction, not a long
  Normal-mode keyboard-navigation stream or terminal output. Its passing
  result therefore does not contradict the user's sustained-navigation
  report. The exact cause of the new hitch is **not established**. Investigate
  core navigation, retained/flat viewport access, frame copy, event batching,
  terminal flush, and any post-open idle work; do not assume a particular
  component is guilty from code inspection alone.
- Characterwise Select currently gathers only source-backed rendered atoms
  (`rendered/nav.rs::character_source_ranges`), while line breaks have no
  glyph atom. `vim.rs::apply_selection_inner` deletes exactly those separate
  ranges. This explains why lowercase-`v` selection across prose, list items,
  table rows or code can erase glyphs yet leave blank source lines. The
  atom-only projection is also present in `HEAD`; this is a real general
  defect, not yet evidence of a branch-only regression. The user's new
  whole-physical-line rule applies to `d` and `c`, while existing Select
  yank/copy output must remain byte-for-byte unchanged.
- Character Select entry can convert all retained rows to a complete layout
  (`session.rs::enter_select`). Each motion recomputes character source ranges
  from all layout atoms (`refresh_character_selection`); rendering then
  projects selection rows across the complete layout, and App asks for
  selection metadata again. This explains why even a short selection is
  document-size-sensitive, and why both `main` and this branch feel slow.
- `RetainedBlockModel::prepare_edit` returns `Wide` for a block larger than
  `MAX_LOCAL_BLOCK_BYTES = 8 KiB`, and also for newline-changing edits in
  non-paragraph blocks. Both new fences far exceed this cap, so a local
  code-function deletion takes the whole-model path. Source injection parse
  entries overlapping an edit are invalidated and reparsed on demand. The
  current block-level retained granularity cannot deliver a bounded edit in
  these fences. This is a structural limitation, not a threshold to increase
  blindly.
- The projected Select mutation in `vim.rs::apply_selection_inner` obtains a
  complete owned source string, clones it, applies byte-range removals to the
  clone and replaces the whole Vim buffer. Narrow derived-cache edit envelopes
  do not make that authoritative mutation itself local. The feasibility gate
  must measure and address this cost while preserving a single undo and exact
  registers; improving only rendered rows cannot satisfy the deletion target.
- The user's approximate 160 MB (`main`) versus 172 MB (candidate) memory
  observation is recorded, but the 12 MB difference alone does not justify a
  memory redesign. Track comparable steady and peak RSS while fixing UX.

The architecture review finds two concrete costs in the current candidate:
  whole-layout materialization at the Select boundary, an 8 KiB block
  ceiling that makes the large-fence interaction path wholly non-incremental,
  and a whole-buffer projected mutation behind nominally narrow edit spans.
The one-owner core/pane separation, exact provenance, query reuse and indexed
cursor work remain useful. We should **not** discard them solely because the
current retained projection is incomplete. Equally, continuing to tune the
old wheel/cold benchmarks would be sunk-cost behavior. Treat the branch as an
unaccepted candidate. The first two new tasks form a decisive measurement and
bounded-feasibility gate. If a single coherent core projection cannot make
large-fence edits and Select responsive without fidelity loss or excessive
state coupling, stop at `PLAN_CHANGE_REQUIRED` and compare a source-centric
viewport pipeline or partial rollback; do not add loading states, stale
editable frames, a second mutable text owner, or a second host renderer.

### Implementation direction and decision gate

First, reproduce all three reported interactions against the *actual* static
1 MiB document in fresh and warmed standalone/pane sessions, and compare
sequentially with a clean `main` build on this machine. Instrument input,
source/injection parsing, Markdown model, retained-row access, selection
projection, pane frame, event-queue batching, terminal write/flush, and
steady/peak RSS. Include separate long uninterrupted `j`/`k` and arrow-key
runs positioned at rows 400–600 and inside each large fence; include a stop/resume run and
immediate post-open navigation. Use a PTY/terminal-output measurement alongside
the existing public-pane timing, with no fake loading frame. Record raw samples
and a reproduction transcript. The fixture may expose a terminal throughput
problem that headless pane timings alone cannot see.

Then build a bounded, test-only feasibility slice for (a) updating and
highlighting interior lines of a large fence without reparsing/re-rendering
the entire 390 KiB block, (b) projecting a short rendered selection from
indexed source/row spans without converting or scanning the complete layout,
and (c) applying the projected edit through the authoritative Vim buffer
without a routine whole-buffer replacement.
Tree-sitter changed ranges may propagate through an unclosed comment/string;
the prototype must include such cases and demonstrate correct wider
invalidation. Compare cells, semantic styles, byte-exact atoms, selections,
cursor positions, undo, and operation results to a fresh full build. Measure
the exact user-like 10–20-line Rust and Go function deletion. Proceed only if
the prototype shows a credible route to the interaction targets below with
one canonical `LiveDocument` transaction and one retained core projection.
Otherwise stop for a new architectural decision before production expansion.

If that gate passes, implement delete/change-specific physical-source-line
normalization in core across Markdown constructs, leaving yank/copy projection
unchanged; remove the measured keyboard
hitch at its actual layer; make Select motion/frame work proportional to the
selected/visible region; and integrate the proven intra-fence update into
source analysis, Markdown model, mapped rows and navigation. Full parser
rebuilds remain correct for delimiter/global reinterpretation and are measured
separately, but may not be the routine response to deleting ordinary code
lines. Standalone and embedded hosts must use the same core behavior.

### New task sequence

13. [013 — Reproduce real-fixture interaction and terminal latency](tasks/013-real-fixture-interaction-profile.md)
14. [014 — Bounded Select and large-fence feasibility gate](tasks/014-interaction-feasibility.md)
15. [015 — Characterwise full-line delete semantics](tasks/015-character-delete-lines.md)
16. [016 — Sustained rendered keyboard navigation](tasks/016-keyboard-navigation.md)
17. [017 — Indexed responsive Select projection](tasks/017-select-projection.md)
18. [018 — Incremental large-fence code edits](tasks/018-large-fence-edits.md)
19. [019 — Full interaction, fidelity and host acceptance](tasks/019-interaction-acceptance.md)

All new tasks are main-only: the first two decide architecture; the others
cross source provenance, mutation, rendering or input boundaries. No completed
round-1 task/evidence is changed. New production behavior and tests land
together, and a failed feasibility result is a plan-change stop, not a
permission to defer the requested behavior.

### Quality gate and measurable acceptance

`make check` remains the standard per-task gate. Add a discoverable
`make bench-acceptance-1mb-record` target for non-asserting diagnosis and an
asserting `make bench-acceptance-1mb` target for the exact-document scenarios.
The latter is added to `gate.json`'s final commands; all prior commands remain.
Harness contract tests enter `make test`. No external dependency is planned.

1. On the static 1 MiB document at 100×41, at least five fresh-process runs
   exercise separate 1,000-consecutive-key rendered Normal-mode `j`/`k` and
   arrow-key sequences, including both directions, a starting position in
   the 400–600-row region, separate starting positions inside each large
   fence, immediate post-open and stop/resume conditions.
   Every input must advance or correctly clamp the cursor; completed pane
   frames must show the current position. Key-to-owned-frame p99 must be
   <25 ms with no sample ≥50 ms. PTY input-to-terminal-flush distributions
   and output volume are reported against the same-machine `main` baseline;
   a sustained visible-output pause ≥100 ms or a p99 regression larger than
   both 10 ms and 10% against `main` requires investigation/plan-change
   review, not a passing result.
2. Character Select on 10–20 lines of each large fence updates the visible
   selection after every `j`/`k` or arrow-key motion. At least 100 samples per
   language show p99 <50 ms key-to-current-owned-frame, with no per-key
   whole-document layout conversion or all-atom scan. Character, line and
   block shapes, wrap, Unicode, tables, synthetic glyphs and source-byte
   provenance match the full-build oracle. Public full-selection queries
   remain correct; viewport drawing uses the same indexed core model.
3. With lowercase `v` or uppercase `V`, selecting all visible text of a
   physical source line and pressing `d` or `c` removes that source line and
   its line break. Cover prose, headings, list items, table rows and 10–20
   lines of each large code fence. For the fence case, surrounding code and
   both fence markers remain; one undo restores exact original bytes.
   Partial-line selections do not remove unselected text or line breaks; a
   wrapped physical line is removed only when all its rendered source-backed
   content is selected. Synthetic glyphs do not count as source text, and
   hidden Markdown delimiters are removed with a fully selected physical
   line. Existing `v`/`V`/Ctrl-V yank and copy bytes, register shape and
   clipboard output remain unchanged.
4. The 10–20-line code-fence `d`/`c` operations above, including source-cache
   update and the completed pane frame in the correct resulting mode (rendered
   Normal for `d`, source Insert for `c`), reach p99 <50 ms over at least 100
   verified edits per language on the 1 MiB fixture; repeated edit/undo cycles prove exact
   text and frame equivalence. A delimiter edit or a syntax construct that
   propagates to the rest of a fence may take a wider measured path, but may
   never publish stale highlighting or provenance. The 8 KiB limit may remain
   as a conservative fallback for other blocks; it cannot force routine
   whole-document rebuild for local code-fence edits.
5. Existing cold, interaction, memory, full-layout, API/privacy, standalone
   and embedded parity gates remain green. Record steady/peak RSS, but do not
   chase the reported 12 MB difference unless it breaches an existing gate or
   grows materially. The original `make check` and complete final gate pass
   without warnings; manual use of the exact document is documented for final
   human UX acceptance.

### Risks and out of scope

Keyboard/PTY output timing can depend on terminal and event cadence. Preserve
raw per-event and phase data and compare sequential same-machine baselines;
do not substitute a good median for a bad tail. A full 400 KiB fence is one
Markdown block but not necessarily one syntax invalidation region. Efficient
line-level updates must respect Rust raw strings, Go block comments, Unicode,
line endings, link/footer indices and exact wrapped-row source ownership.
The current public full-layout API remains supported even if the pane uses
indexed visible rows. Do not change Markdown syntax, introduce a dependency,
weaken current fidelity, or alter `oom`. A full-fence replacement, delimiter
removal, resize or reload is measured separately and is not misclassified as
an ordinary local deletion. The user-visible first frame remains complete;
there is no pending screen or stale edit mode.

## Revision 5 — sustained navigation acceptance clarification

This revision changes only round-2 task 016. The explicit current-branch
release binary was retested by the user in kitty at the reported geometry; the
user now observes no visual hitch or pause. Exact 63×229 pane and PTY traces
also find no branch-only navigation stall, including fresh and post-edit
states. The previous shell command `oom-edit` resolved to a separate clean
`main` checkout, so the first comparison cannot establish a current branch
regression. The earlier observation remains recorded, but its cause is
unconfirmed. See `evidence/016-investigation.md`.

Task 016 now verifies and retains the sustained 1 MiB navigation regression
gate instead of requiring a speculative renderer repair or an impossible
red-before-green test. A focused asserting navigation target covers the fixed
public-pane and standalone PTY scenarios; manual kitty acceptance is recorded
separately because PTY flush is not physical display timing. If a branch-only
stall recurs or the focused gate fails, investigate its measured cause before
changing product behavior. No key may be skipped and no stale frame accepted.

All round-2 final measurable acceptance criteria, tasks 017–019, the complete
final gate, and the standalone/embedded architecture requirements remain in
force. The user explicitly approved this narrow change before implementation
resumed.

## Revision 6 — recovery after final-gate memory failure and prose Select pause

This is a continuation of acceptance round 2, not a new package or a rewrite
of completed tasks 001–019. The final gate remains red. The user authorized
recovery from `RETRY_BUDGET_EXCEEDED` and explicitly rejected the approximately
700 ms prose Select mutation pause. The previous approval and failed-gate
record remain historical evidence; this revision requires new approval before
product-code implementation. Responsiveness has priority over shaving cold
first-frame milliseconds. No pending/loading frame, stale editable layout,
second mutable text, separate embedded renderer, or parser replacement is
authorized.

### Reproduced failures and current explanation

- A fresh optimized exact-fixture probe at source line 600, using lowercase
  `v`, 15 `j` motions, then `d`, measured 683.5 ms from key to end of input
  handling and another 348.6 ms for the completed 100×41 rendered frame.
  This confirms that the documented roughly 690/350 ms result was not merely
  one scheduling outlier. The selection yields three disjoint source ranges.
  `LiveDocument::refresh_derived` applies each `TextEdit` to the highlighter
  and retained Markdown model separately; a multi-change projection is then
  discarded by `EditorSession`, forcing a full rendered rebuild. The Vim
  multi-range path also materializes/clones/replaces the full text. Existing
  phase instrumentation attributes most handler time to repeated model and
  source refresh, but the complete optimization breakdown must be measured
  before changing ownership or invalidation semantics.
- A fresh 1 MiB Rust-fence full-layout probe measured 202.8 MiB peak RSS,
  versus the fixed 189.6 MiB baseline-plus-10% ceiling for that row. The
  final gate had failed all large Rust-fence peak-RSS rows while retaining
  other memory and latency gates. `highlight_fence_span` obtains the source
  injection parse tree during rendered layout, and `source_injection_tree`
  retains eligible trees in a 1 MiB-source-byte cache. This is a plausible
  size-correlated memory cause, **not yet allocation attribution**. The same
  tree accelerates subsequent fence edits, so discarding it without an edit
  measurement is not an accepted repair. The unchanged spell-overhead timing
  outlier passed on retry and does not justify a threshold change.

### Recovery strategy and decision points

1. Build a red-before-green, exact-document public-pane cycle gate for prose
   and mixed-Markdown 10–20-line Character Select `d`/`c` plus one-step undo.
   Include 100 verified cycles per operation at the representative line-600
   region, completed current-text frames in both target modes, raw input and
   frame phases, p99 and worst, and differential byte/cell/style/provenance
   checks. The gate should fail on the current candidate for latency, not
   because of a fixture mismatch. Keep the Rust/Go cycle gate intact. Record
   source/Markdown/row/projection/Vim phase and current/peak memory counters
   for the same transaction. For Rust-fence memory, use controlled
   same-process or fresh-process A/B measurements of cache/tree ownership and
   temporary layout allocation, without altering the production behavior in
   the diagnostic run. Establish whether a tree is necessary for the first
   responsive edit and what memory it actually accounts for.
2. Make projected disjoint edits one canonical transaction. Preserve the
   ordered `TextEdit` coordinate contract, one Vim undo/register operation,
   and the sole authoritative `LiveDocument` text owner. Apply highlighter
   tree edits and injection rebasing in sequence but perform at most one
   required Markdown reparse/reference rediscovery against final text.
   Incrementally rebuild only affected Markdown blocks and dependencies when
   their boundaries and global semantics are provably stable; a delimiter,
   reference/footnote or other propagating change must take the exact wide
   path. Avoid a routine full-buffer replace for a short multi-range edit.
3. Publish multiple retained block/row changes atomically against the final
   authoritative text. Rebase unchanged blocks, source atoms and row offsets;
   update only affected rows and preserve the public complete-layout API.
   If selected ranges cross constructs whose interpretation changes, widen
   exactly as correctness requires. Keep Select yank/copy unchanged and
   preserve the already-fast single contiguous fence path. Compare every
   resulting frame and operation to a fresh full-build oracle, including
   undo/redo, wrapped prose, headings, lists, tables, footnotes, references,
   Unicode and CRLF. The required ordinary 15-line mixed-prose case must not
   force a full 1 MiB model/layout rebuild.
4. Repair the measured Rust-fence RSS cause without discarding the edit
   accelerator or moving the cost into the first edit. Candidate tactics
   include reducing transient simultaneous allocations, reusing a parse
   tree already built for highlighting, or retaining less redundant derived
   material. Select the smallest evidence-backed change. Run both the fixed
   memory ceiling and the 100-cycle Rust/Go edit latency gate after each
   candidate; if they genuinely conflict, stop at `PLAN_CHANGE_REQUIRED`
   with measurements and a cost/risk decision rather than weakening either
   gate silently.

Task 020 is a feasibility gate. If bounded multi-range refresh or the fixed
memory ceiling cannot be achieved without a materially different lifecycle or
ownership model, stop for an explicit architecture decision. Do not keep
optimizing a design merely because tasks 001–019 are complete. Tasks 021–024
are conditional on the measured feasibility result; they are not permission
to deliver an unbounded fallback as the normal path.

### New task sequence

20. [020 — Recovery instrumentation and feasibility](tasks/020-recovery-feasibility.md)
21. [021 — Atomic multi-range live mutation](tasks/021-atomic-multi-range-mutation.md)
22. [022 — Retained multi-block and row publication](tasks/022-retained-multi-block-publication.md)
23. [023 — Rust-fence memory repair](tasks/023-rust-fence-memory.md)
24. [024 — Integrated recovery acceptance](tasks/024-recovery-acceptance.md)

All five tasks are main-only because each either makes an architectural
decision or crosses authoritative text, parser/model, rendered provenance, or
host-observable boundaries. Historical task documents and evidence remain
unchanged.

### Quality gate and acceptance

The standard command stays `make check`. The final command list remains the
existing eight-command `gate.json`; the new asserting
`make bench-acceptance-1mb-prose-cycles` target is appended, never substituted
for any existing command. Its runner contract tests join `make test` through
the Makefile. No new dependency is planned. The exhausted final-gate counter
is reset for this recovery planning revision; new final attempts may begin
only after approval and completed new tasks. The failed-gate evidence remains.

1. On `examples/kitchen-sink-1mb.md` at 100×41, at least 100 verified
   edit/undo cycles each for `d` and `c` on the representative 15-line
   mixed-prose selection have key-to-completed-owned-frame p99 <50 ms and no
   sample ≥100 ms. Report input and frame phases separately and retain raw
   records. The current ~1,032 ms combined delete is a red baseline, not an
   accepted wide-path exception. Include at least one other ordinary
   mixed-Markdown location so this is not a line-600 special case.
2. Every prose cycle removes the expected physical lines, preserves
   unaffected bytes, paints the exact target mode/current text, and one undo
   restores exact original bytes. Differential tests compare all cells,
   semantic styles, source atoms, cursor/selection, mappings, links,
   footnotes, source syntax and operation results after delete/change/undo.
   Partial-line, wrapped-line, character/line/block, Unicode, CRLF, and
   global-semantic edits keep their specified behavior. Select yank/copy
   payloads and clipboard behavior are unchanged.
3. The 128 KiB cold-pane and 256/512/1024 KiB layout Rust-fence peak-RSS
   rows pass the **unchanged** fixed baseline-plus-10% limits in
   `make bench-realistic` on this machine. All other realistic memory,
   cold-time, retained-heap, scaling and interaction limits remain green;
   especially the 100-cycle <50 ms Rust/Go code-fence edit gate. Record the
   before/after allocation cause and trade-off, not just green totals.
4. The standalone pane, embedded public pane, and core-only host use the
   same synchronous faithful core path. No terminal crate enters core. All
   public API/privacy, conformance, property, exact 1 MiB, standalone,
   embedded and full `make check` gates pass. A human kitty acceptance check
   remains requested for perceived smoothness; automated pane/PTY timing is
   not falsely called pixel-presentation timing.

### Risks and exclusions

Several small edits can alter Markdown parsing beyond their direct spans;
batched reparse and row publication must prove boundary stability before
retaining material. A strict 50 ms total at 1 MiB may require reducing
selection projection overhead in addition to the three repeated rebuilds.
Peak RSS is a process high-water mark, affected by temporary overlap and
allocator behavior; memory optimization must not simply postpone work to the
first edit or substitute a weaker measurement. The existing cold first-frame
budgets and broad semantic-change paths are preserved, but cold-first-frame
micro-optimization and a new background/partial-view lifecycle are out of
scope. If the feasibility gate disproves this approach, seek an explicit
plan change rather than a partial delivery or a raised threshold.

## Revision 7 — Rust-only RSS allowance with leak-stability proof

This is a recovery revision within acceptance round 2. It supersedes only
Revision 6's requirement that the four Rust-fence peak-RSS case families pass
the pre-incremental baseline-plus-10% cap. The user explicitly prefers the
measured responsive edit path over further memory-only complexity, provided
the additional RSS is not a leak. Completed tasks 001–022 and their evidence
remain immutable. Tasks 023–024 remain open and receive the amended criteria
below; product-code work pauses until this revision is approved.

### Evidence and decision

Task 020 attributed about 30.7 MiB of the 1 MiB Rust-fence RSS increase to
the retained source-injection tree. Removing that cache lowered RSS but made
the first exact-fixture Rust delete take about 708 ms key-to-frame, violating
the interaction priority. Task 023's narrow line-consumption candidate keeps
the cache and current fidelity. Five fresh-process measurements
(`evidence/023-rss-candidate.jsonl`, summarized in
`evidence/023-rss-decision.md`) put the worst 128 KiB cold-pane and
256/512/1024 KiB layout Rust-fence RSS rows at 110.79%, 110.80%, 110.61%,
and 110.74% of their fixed baselines. Those miss +10% by 0.26–1.34 MB.
The same record shows the other realistic memory and time rows within their
existing limits. Exact prose edit/undo, Rust/Go edit, and interaction gates
passed before the memory-only exploration.

The new cap is **baseline plus 15% for Rust-fence peak RSS only**, applied to
both 128 KiB cold-pane widths and the 256/512/1024 KiB Rust-fence layout
rows. The +10% limits for every other content class, all retained-layout heap
limits, cold-time limits, layout-scaling limits, and interaction limits remain
unchanged. The extra five percentage points provide a small reproducibility
margin over the observed approximately +10.8%, without excusing memory growth
over time. The benchmark must continue comparing against its hard-coded
pre-incremental baseline; it must not learn a new baseline from the candidate.

The preliminary 100-versus-400-cycle fresh-process Rust delete/undo comparison
on the exact 1 MiB mixed fixture peaked at 238.58 versus 237.82 MiB, with
the 400-cycle process flat across its sampled quarters. That is consistent
with a bounded retained cache, but does not prove that reloads, source/rendered
transitions, other fence languages, or longer histories are leak-free.

### Required implementation and tests

1. Keep the narrow highlighted-line ownership reduction and discard
   exploratory streaming or cache-eviction complexity that failed to deliver
   a robust +10% margin. Preserve the one live-text owner, retained source
   parse cache, exact highlighting/provenance, and common core path for
   standalone and embedded hosts. The first post-open Rust and Go edit must
   independently finish key-to-owned-frame in under 50 ms; no deferred parse
   cost may be hidden outside the existing 100-cycle p99 gate.
2. Add a discoverable `make bench-rss-stability` target with an asserting,
   exact-fixture public-pane process probe and parser contract tests wired
   into `make test`. Read current RSS (`VmRSS`), not merely monotone peak RSS,
   after a completed frame and restored document at fixed cycle checkpoints.
   Exercise Rust and Go delete/change/undo, repeated source↔rendered
   transitions, and reload. Warm the first 100 cycles, then check at cycles
   100, 200, 300, and 400 that no later checkpoint exceeds the cycle-100
   current RSS by 2 MiB; run three fresh processes per scenario. Require
   exact text, mode and frame at every cycle, and a final source-cache
   entry/byte bound through core tests. If an allocator plateau makes this
   deterministic bound inappropriate, stop for evidence and a revised
   decision rather than silently relaxing it. No finite benchmark can prove
   the absence of every leak, so the evidence must also review ownership and
   invalidation paths for monotonic retention.
3. Update `scripts/realistic_performance.py` and its contract tests so only
   the stated Rust-fence RSS comparisons use +15%. Keep the unchanged fixture
   identities, baseline bytes, case inventory, and all other thresholds.
   Record five fresh-process results, first-edit timing, cache ownership,
   and RSS-stability samples. Update `docs/performance.md` to distinguish
   bounded working-set increase from a leak and state the limited scope of
   the new cap.

### Task and gate changes

- Task 023's unchanged-RSS acceptance criterion is replaced by the Rust-only
  +15% criterion and the asserting leak-stability requirement. Its validation
  adds `make bench-rss-stability`; existing realistic, cycle, incremental and
  `make check` commands remain.
- Task 024 verifies the new cap and stability evidence alongside every prior
  fidelity, interaction, source-first, standalone and embedded criterion. Its
  validation adds `make bench-rss-stability`.
- The final gate appends `make bench-rss-stability` after
  `make bench-realistic`; no existing final command is removed. The standard
  gate remains `make check`. The new runner's contract tests must join the
  repository's ordinary test target, and its command must appear in
  `make help`.

### Revised final acceptance

The exact prose and Rust/Go edit/undo responsiveness targets, complete frame
fidelity, cold/source-first budgets, public core/pane API, terminal-free core,
and host parity are unchanged. Rust-fence peak RSS must pass the narrowly
revised +15% hard-baseline cap in five fresh processes; every other realistic
memory row remains at +10%. The 400-cycle RSS-stability gate must pass all
Rust/Go edit, mode-transition, and reload scenarios with no sustained growth
beyond its 2 MiB post-warmup bound and with bounded source-cache ownership.
All task-specific, `make check`, and final commands must pass. The manual kitty
UX check remains supplementary, not a substitute for automated gates.
