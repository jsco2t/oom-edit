# Markdown Clipboard Copy Policy

## Objective

Make clipboard copy preserve the actual Markdown source by default, including
syntax that rendered Select hides, while providing a persistent configuration
setting that makes every Markdown-backed clipboard write emit sanitized plain
text instead. Keep the existing edit, register, put, undo, and terminal transport
semantics intact.

## Current behavior and root cause

The preceding `2026-08-25-rendered-select-clipboard` workflow made plain rendered
Select yanks publish through `Effect::ClipboardWrite`, but it reused the payload
created from `RenderedSelection.source_ranges`. Those ranges deliberately model
the source bytes owned by visible rendered atoms so delete, change, and yank
operations can preserve surrounding Markdown syntax. Parser-hidden delimiters
such as inline-code backticks, emphasis markers, and link destinations therefore
do not belong to the operation ranges.

For example, the visible atoms for `` `App` `` own `App`, not the surrounding
backticks. Reusing those edit-safe ranges for clipboard presentation turns the
source into plain-looking text. Existing tests derived expected clipboard output
from the same `source_ranges`, so they validated internal consistency rather than
the user-visible requirement. The current range concatenation is not a complete
plain-text formatter either: raw entities and escapes can still survive.

The preceding workflow is immutable historical context. Its changes are present
as pre-existing uncommitted working-tree state and must be preserved. This new
package owns only the corrective behavior and configuration described here.

## Proposed implementation

Separate clipboard presentation from edit/register projection inside
`oom-edit-core`. Keep `RenderedSelection.source_ranges` and the private
`vim::ProjectedSelection` operation model unchanged. Before a rendered selection
is consumed, build a copy-only representation containing:

- a Markdown form that preserves complete selected Markdown constructs; and
- a rendered plain-text form that strips Markdown syntax and excludes synthetic
  renderer decorations.

For the Markdown form, retain or derive parser-provided full source spans for
inline constructs. When all visible source-backed content of a construct is
selected, include the complete construct source span, such as backticks,
emphasis/strong/strikethrough delimiters, link destinations, image syntax,
escapes, and entity spelling. A partial selection must not pull in unselected
content or unmatched delimiters. Character, line, and block selection must keep
their established logical shapes and must not copy synthetic borders, padding,
continuation prefixes, list decorations, or link-index markers.

For the plain-text form, use renderer/parser knowledge rather than raw-range
concatenation. The result must match the selected rendered content: Markdown
syntax is removed, entities and escapes are decoded, inline-code normalization is
honored, generated decorations are excluded, and logical line/block boundaries
are preserved. For clipboard writes originating from raw source-mode Markdown,
core will construct the same dual representation with an in-tree sanitizer based
on the already-present Markdown parser and a deterministic fallback for partial
fragments. URL-only link-index copies use identical values because their content
is format-invariant.

Replace the single-string clipboard effect payload with a project-owned typed
clipboard content DTO carrying both representations. `oom-edit-core` remains the
only owner of Markdown interpretation and never performs terminal I/O. The TUI
`App` chooses one representation immediately before calling its existing
injected `ClipboardSink`.

Add a TUI-owned configuration section with a default of Markdown:

```toml
[clipboard]
copy_format = "markdown" # alternative: "plain-text"
```

Configuration loading, defaulting, round-tripping, and validation will follow
the existing serde/TOML patterns. The setting applies to outgoing system
clipboard content only. Internal Vim register text and shape, including cached
`"+p`, remain governed by the existing editing model and never read the desktop
clipboard. The existing OSC 52 size limit, encoding, and error reporting apply to
the representation selected by `App`.

## Architectural decisions

- Operation-safe source ranges remain unchanged. Expanding them to include
  hidden Markdown would make delete and change consume syntax they currently and
  intentionally preserve.
- Clipboard formatting is a core-owned projection because source provenance,
  inline parsing, wrapping, tables, and normalization belong in the reusable
  editor core. The TUI owns only the persisted preference and final choice.
- The public effect boundary becomes a deliberate, project-owned typed DTO. No
  parser, renderer, terminal, or `hjkl` type crosses the crate-root facade.
- Complete constructs regain their complete source syntax only when their visible
  content is wholly selected. Partial selections never expand beyond the user's
  selected content.
- The plain-text preference affects every outgoing Markdown-backed clipboard
  write, not merely rendered character selection. Format-invariant URL copies
  remain unchanged.
- Clipboard presentation does not replace the authoritative live text, mutate
  selection state, or change unnamed, numbered, named, black-hole, or shared
  system-register behavior.
- No dependency is needed. Existing parser, provenance, configuration, and OSC
  52 infrastructure are sufficient.
- No alternate clipboard backend, desktop clipboard read path, runtime setting
  command, or source-mode implicit publication is introduced.

## Work included

1. A core-owned dual-format clipboard DTO and copy-only Markdown/plain-text
   projection for rendered and source-backed clipboard writes.
2. Construct-aware coverage for complete and partial inline constructs,
   character/line/block shapes, wrapping, tables, Unicode, entities, escapes,
   repeated text, and synthetic output.
3. Public API facade/compile guards and conformance updates for the changed typed
   effect boundary.
4. A persisted `[clipboard].copy_format` setting whose default is `markdown` and
   whose alternate value is `plain-text`.
5. App/startup wiring that chooses the configured form before invoking the
   existing sink, plus documentation and changelog updates.
6. Regression coverage proving editing registers, puts, deletion, change, undo,
   OSC 52 limits, and error handling remain unchanged.

## Task sequence

1. `tasks/001-build-dual-format-clipboard-projection.md`
2. `tasks/002-wire-clipboard-format-configuration.md`

## Quality gate

The standard and final gate is `make check`, the repository's build system of
record. It runs formatting verification, Clippy with warnings denied, a workspace
build, the complete test suite, dependency/license/advisory checks, and bundled
data license verification. Each task first runs `make test`, followed by the full
standard gate.

There is no separate type-check command because the build and Clippy stages in
`make check` perform Rust compilation and type checking. No dependency/vendor
workflow is included because this plan adds or upgrades no dependency.

## Risks

- Expanding edit ranges instead of constructing a copy-only projection would
  regress delete/change semantics and could remove surrounding Markdown.
- Treating every touched construct as complete would surprise users by copying
  unselected delimiters, destinations, or sibling content. Partial-selection
  tests must be byte-exact.
- A plain-text formatter built from raw source ranges would mishandle entities,
  escapes, code normalization, tables, or synthetic decorations. Tests must use
  hardcoded user-visible expectations rather than derive them from the same
  projection under test.
- Changing the effect payload is a public API change. Every downstream pattern
  match and compile-time facade guard must move atomically.
- Applying the preference to internal registers would change `p`/`P` behavior and
  lose linewise/blockwise metadata. The preference must be resolved only at the
  external sink boundary.
- The OSC 52 limit is measured on the chosen outgoing bytes. Tests must ensure
  selecting a representation cannot bypass oversize rejection or error feedback.
- The worktree already contains the preceding workflow's implementation. Those
  changes are prerequisite context, not cleanup candidates, and must not be
  reverted or rewritten outside this plan's overlap.

## Out of scope

- Changing rendered delete, change, register, put, undo, cursor, or selection
  semantics.
- Full Vim `clipboard=unnamedplus` behavior or implicit source Normal-mode yanks.
- Reading the desktop clipboard, querying OSC 52, or changing cached `"+p` into a
  live operating-system clipboard read.
- Native AppKit, X11, or Wayland backends; shell helper programs; clipboard
  crates; terminal capability probing; or alternate transports.
- A runtime command, command-palette row, or per-document override for the copy
  format.
- Changing the 100 KiB raw-input limit, Base64 encoding, BEL terminator, or
  best-effort terminal acknowledgement contract.
- Modifying the archived predecessor workflow or external notebook documents.

## Final acceptance criteria

- With default configuration, copying the complete rendered source
  `` `App` consumes `Effect::ClipboardWrite` through an injected
  `ClipboardSink`. `` emits that exact Markdown, including every backtick.
- Complete inline code, emphasis, strong, strikethrough, link, image, nested,
  escaped, and entity constructs preserve their exact Markdown source under the
  default format.
- Partial construct selections contain only selected content and never add
  unmatched delimiters, link destinations, or other unselected bytes.
- With `copy_format = "plain-text"`, the example emits `App consumes
  Effect::ClipboardWrite through an injected ClipboardSink.`; Markdown syntax is
  stripped, entities/escapes are decoded, inline code is normalized, and
  renderer-generated glyphs are excluded.
- Character, line, and block selections remain correct across wrapping, tables,
  repeated content, multiline content, and UTF-8, with their established logical
  boundaries preserved.
- Plain and explicitly targeted rendered yanks retain existing unnamed, `"0`,
  named, black-hole, and shared system-register shape semantics; `p`/`P`, delete,
  change, undo, empty selections, and source Normal-mode non-emission do not
  regress.
- URL-only synthetic link copies are identical in both formats, and no synthetic
  table border, padding, continuation prefix, list decoration, or link-index
  marker enters either selection payload.
- Missing clipboard configuration defaults to Markdown; valid values round-trip;
  invalid values fail through existing configuration error handling; and README
  and changelog text accurately describe the setting and external-only scope.
- `App` sends only the configured representation to the injected sink, and sink
  failures, success feedback, canonical OSC 52 output, and the 100 KiB limit are
  evaluated against those chosen bytes.
- The crate-root public API and compile-time guards expose only the intended
  project-owned clipboard DTO, with no new dependency, second mutable text owner,
  terminal dependency in core, or parallel routing path.
- `make check` passes after each task and at final acceptance.

## Acceptance follow-up — Round 2

### Observed acceptance failure

Rendered character selection of the complete escaped-backtick fragment shown in
`examples/kitchen-sink.md` emits this Markdown clipboard representation:

```text
backtick escaping: `backticks` inside code ``
```

The opening two-backtick delimiter and its delimiter-adjacent space are lost,
while the closing delimiter remains. A linewise selection of the same source is
exact, which isolates the defect to character/block copy reconstruction rather
than configuration selection, OSC 52 encoding, or the clipboard sink.

### Root cause and repository evidence

`pulldown-cmark` reports the complete valid code-span token as one `Event::Code`
range, including both delimiter runs. In `rendered/blocks.rs`, `mapped_leaf`
aligns the rendered code payload against that raw token greedily from its first
byte. When the rendered payload itself begins with a literal backtick, the first
opening delimiter is incorrectly assigned as that visible atom's source. The
copy-only `markdown_for_ranges` logic therefore sees the first selected atom as
starting at the construct boundary and concludes there is no missing opening
prefix to restore. This produces exactly the user-reported malformed fragment.

Existing clipboard coverage uses single-backtick spans whose payloads do not
begin with backticks, and the existing inline-code provenance tests cover
newline and escaped-pipe normalization but not delimiter-like payload edges.
The escaped acceptance case therefore passed beneath both layers of validation.

### Additive fix strategy

Correct code-span provenance where parser leaves are constructed. Code payload
alignment must operate inside the parser-confirmed opening and closing delimiter
runs rather than treating delimiter bytes as candidate visible payload. Preserve
the current normalization behavior for spaces, newlines/CRLF, table-cell escaped
pipes, Unicode display groups, and repeated text. The copy projection can then
continue using parser construct spans plus visible-source coverage without a
special-case repair or a second provenance model.

Add byte-exact tests for delimiter runs of different lengths, literal backticks
at payload boundaries, delimiter-adjacent spaces, repeated backticks, multiline
normalization, and table cells. Add an end-to-end rendered selection regression
using the exact kitchen-sink fragment and verify both Markdown and plain-text
representations. Cover complete character and block selection as applicable,
partial selection non-expansion, unchanged linewise behavior, and the App's
default Markdown sink choice.

### Architecture and test constraints

- Keep source provenance attached at parser leaves; do not patch copied strings
  with substring searches or post-hoc column correction.
- Keep `RenderedSelection.source_ranges` and the private Vim operation projection
  unchanged so delete, change, registers, put, and undo retain their semantics.
- Keep Markdown interpretation in `oom-edit-core`; the TUI continues to choose
  only between the prepared representations.
- Add no dependency and introduce no new public API.
- Use hardcoded expected strings for the escaped-backtick regressions.

### New task sequence

3. `tasks/003-fix-multi-backtick-code-span-copy.md`

### Risks and out-of-scope boundaries

The main risk is fixing the reported opener while shifting source ownership for
normalized code content or partial selections. Regression coverage must prove
that every visible display group remains UTF-8 safe and monotonically mapped to
the correct raw payload bytes. Configuration schema, clipboard transport,
runtime commands, and broader Markdown rendering behavior remain out of scope.

### Round 2 acceptance criteria

- Complete rendered character selection of the exact kitchen-sink fragment
  emits its source byte-for-byte and emits syntax-free plain text containing the
  literal inner backticks but not the outer two-backtick delimiter runs.
- Code spans using one or more backticks retain exact complete source syntax
  when selected, even when their payload begins or ends with literal backticks
  or contains repeated delimiter-like text.
- Partial selections do not acquire outer delimiters or unselected payload.
- Existing inline-code normalization, table, wrapping, Unicode, line/block
  selection, edit/register/put/undo, and copy-format preference behavior remains
  green.
- The default App path sends the exact escaped-backtick Markdown representation,
  while `plain-text` sends only its rendered payload.
- `make check` passes.
