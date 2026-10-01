# Responsiveness architecture options — investigation, not approval

## Decision criterion

Interaction responsiveness takes priority over the existing 350 ms cold
rendered-first-frame gate. A roughly 500 ms first fully rendered frame may be
acceptable if editing, scrolling, and mode transitions remain smooth. The
current published limit and asserting benchmark have not been changed; a plan
revision must reconcile them before a product change is accepted.

The repeated same-machine profile in `interaction-profile.md` found that
already-built source/rendered wheel scrolling is fast, but 1 MiB structural
source edits take 80 ms median / 217 ms p99, return to rendered takes 205 / 257
ms, rendered line delete takes 644 / 661 ms, and reload takes 517 / 525 ms.
The middle-of-document samples match the top-of-document results.

## Current dependency and ownership boundary

`LiveDocument` owns Vim's authoritative text plus a complete `Highlighter`,
front matter, and spell state. Its mutation gateway refreshes derived state
synchronously. Source viewport rendering always returns fully highlighted
lines. The rendered builder borrows that complete highlighter for fenced code
and front matter, builds a full `BlockModel` and `RenderedLayout`, then maps the
cursor against the finished layout. `EditorPane::render` returns an owned frame
synchronously; App also requests layout during resize and scroll-follow.

Consequently, moving only the final frame draw to another thread would not
remove the measured input-handler stalls. Any nonblocking design must account
for source parsing, rendered layout, cursor mapping, and mode/input semantics.

## Options and costs

| Option | Benefit | Cost or limitation | Assessment |
| --- | --- | --- | --- |
| Narrow synchronous optimizations | Keep all current APIs and exact behavior | Injection work is about 5 ms on neutral edits and about 30 ms after structural edits; the all-row gutter pass is inside a 1–2 ms rendered scroll. Parser and layout stalls remain. | Pursue only new measured hotspots; these candidates do not close the current gap. |
| Background **full rendered layout** from an immutable text/version/width snapshot | Can move the roughly 200–270 ms rebuild off the event path while reusing the exact parser-leaf/provenance builder | Does not address 80–217 ms structural source edit stalls or the larger synchronous input phase of rendered deletion. A pending rendered surface and a public completion protocol are required. | Insufficient on its own for the stated responsiveness priority. |
| Background source parse **and** full rendered layout | Could keep the event loop free during both dominant phases; one owner may publish only current-generation results | Changes the synchronous derived-cache contract. Until the new parse is ready, either visible source styling is temporarily incomplete/stale, input waits, or the editor shows an explicit preparing state. Concurrent jobs may increase CPU contention and peak RSS (already about 188 MiB against a 192 MiB first-frame limit). | Requires an explicit UX/contract choice and memory proof before selection. |
| Incremental/progressive exact parsing and rendered layout | Could make current text immediately usable without a pending full-layout wait | Tree-sitter structural edits can invalidate broad Markdown context; the rendered model has document-wide footnotes, link indices, jump targets, wrapping, and source mapping. This is a substantial second state-management problem with high parity risk. | Do not select without evidence that a pending state is unacceptable and a feasibility prototype succeeds. |

## If background preparation is later approved

The safe shape is a core-owned pure preparation job over one immutable text
snapshot and explicit width/presentation inputs. `LiveDocument` remains the
sole owner of mutable text. Results carry a document generation and width;
the session accepts them only after validating both, then atomically installs
the complete highlighter/layout state and remaps the current cursor and
selection. Work must be coalesced and bounded across tabs and panes; no
highlighter mutex may enter the key path. Worker failure or cancellation must
leave the authoritative Vim text and file identity intact.

That sketch is **not** an approved architecture: it does not yet resolve what
the source or rendered pane displays during preparation, which editor keys
remain actionable, how a core-only host drives completion without duplicating
the standalone engine, or whether the memory and p99 interaction limits can
be met. The current `EditorSession::render_layout` reference-returning API and
`EditorPane::render` owned-frame API are synchronous, so an asynchronous path
would require deliberately curated public status/completion semantics and API
privacy tests.

## Test-forward acceptance conditions for any selected design

- Repeat the 100-sample public-pane interaction matrix at 144 KiB and 1 MiB,
  including top/middle structural edits, source/rendered wheel scrolling,
  rendered delete/undo, return from Insert, resize, and reload. Separate input
  handling and frame time, and compare p95, p99, worst, and peak RSS.
- Assert fully ready frames are byte-for-byte equivalent in cells, semantic
  styles, source atoms, selections, cursor mapping, front matter, code fences,
  links, and footnotes, through both core-only and pane hosts.
- Exercise rapid edit/undo, resize storms, tab switch/close, reload, and stale
  worker completion. No old-generation result may replace newer text or width.
- Keep file identity, atomic save, external-change protection, and the four
  public modes unchanged. Any temporary preparation state must be explicitly
  specified and tested; it must not be counted as the first fully rendered
  frame.

The next human choice is whether a brief, explicit preparation state after a
large structural edit or rendered transition is acceptable. Without that
choice, no background or progressive implementation should begin.
