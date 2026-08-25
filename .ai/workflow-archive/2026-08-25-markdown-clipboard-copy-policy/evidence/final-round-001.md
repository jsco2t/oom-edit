# Final Work-Package Evidence

## Work package

- ID: `2026-08-25-markdown-clipboard-copy-policy`
- Title: `Markdown Clipboard Copy Policy`

## Original objective

Make rendered Markdown copies preserve their actual source syntax by default,
including inline-code backticks, while providing a persistent option to force
sanitized rendered plain text for every outgoing Markdown-backed clipboard write.

## Completed tasks

- Task 001 — Build dual-format clipboard projection — `evidence/001.md`
- Task 002 — Wire clipboard format configuration — `evidence/002.md`

## Whole-package review

The cumulative review traced rendered selection provenance through the private
Vim operation boundary, the core-owned dual-format effect, TUI startup config,
App representation selection, and the existing OSC 52 sink. Markdown expansion
is copy-only; edits and register payloads retain their established source ranges
and shapes. The preference is resolved once at the external sink boundary, and
synthetic URL effects use equal representations. Public facade and dependency
hygiene guards remain intact.

## High-confidence findings fixed

- Block plain text initially reparsed each wrapped row as an isolated Markdown
  fragment, which could expose unmatched delimiters. It now projects directly
  from selected source-backed rendered atoms.
- Size-limit coverage initially proved only the Markdown-oversized/plain-accepted
  direction. Reciprocal Markdown-accepted/plain-oversized coverage was added,
  including zero emitted bytes on both rejection paths.
- README wording could imply the raw source size was always measured. It now
  states that the selected outgoing representation is measured.
- Adding the startup preference exceeded the repository's constructor-argument
  lint. Configured defaults now travel through the owned `AppStartupOptions`
  boundary without suppressing Clippy.

## Final acceptance criteria

- [x] With default configuration, copying the complete rendered source
      `` `App` consumes `Effect::ClipboardWrite` through an injected
      `ClipboardSink`. `` emits that exact Markdown, including every backtick.
  - Evidence: byte-exact core integration and injected-App tests assert the
    complete example; Markdown is the enum/config default.
- [x] Complete inline code, emphasis, strong, strikethrough, link, image,
      nested, escaped, and entity constructs preserve exact Markdown source.
  - Evidence: the conformance construct matrix independently asserts both
    Markdown and rendered plain-text representations.
- [x] Partial construct selections contain only selected content and add no
      unmatched delimiters, destinations, or other unselected bytes.
  - Evidence: focused partial emphasis coverage and the character operator
    matrix pass with byte-exact payloads.
- [x] `copy_format = "plain-text"` emits the syntax-free example, decodes
      entities/escapes, normalizes inline code, and excludes generated glyphs.
  - Evidence: exact App example, core construct/entity tests, table tests, and
    source-provenance selection matrices all pass.
- [x] Character, line, and block selections remain correct across wrapping,
      tables, repeated content, multiline content, and UTF-8.
  - Evidence: selection-shape conformance, resize, table, Unicode, and block-row
    tests assert logical boundaries and both clipboard forms.
- [x] Plain and explicitly targeted yanks retain register shape semantics, and
      puts, delete, change, undo, empty selections, and source Normal-mode
      non-emission do not regress.
  - Evidence: Vim wrapper tests, conformance meta-tests, and the App preference
    invariance test pass across unnamed, numbered, named, black-hole, and shared
    system-register behavior.
- [x] URL-only copies are identical in both formats, and synthetic layout glyphs
      enter neither payload.
  - Evidence: invariant DTO/App tests and table/link-index provenance tests pass.
- [x] Missing configuration defaults to Markdown; valid values round-trip;
      invalid values use existing typed fallback; documentation is accurate.
  - Evidence: focused config defaults/serde/save-load/error tests plus README and
    changelog review.
- [x] App sends only the configured representation; sink feedback, canonical
      OSC 52 output, and the 100 KiB limit operate on the chosen bytes.
  - Evidence: single-write injected-sink assertions, reciprocal size-boundary
    tests, write/flush failure tests, and padded RFC 4648 vectors pass.
- [x] The public API exposes only the intended project-owned DTO, with no new
      dependency, mutable text owner, terminal dependency in core, or parallel
      routing path.
  - Evidence: curated-facade compile guards, dependency hygiene, architecture
    inspection, and supply-chain gates pass.
- [x] `make check` passes after each task and at final acceptance.
  - Evidence: both task evidence files record their passing standard gate; the
    final gate passed all 7 checks.

## Final quality gate

| Command      | Result |
| ------------ | ------ |
| `make check` | PASS — 7 passed, 0 failed |

## Cumulative diff

From baseline `6bf7c7aee91601e2d9cad201d0db3777e47561b5`, the package adds a
core-owned `ClipboardContent` DTO, Markdown-aware rendered copy projection,
default rendered-yank publication with preserved registers, explicit TUI
clipboard-format configuration, selected-representation App routing, stronger
OSC 52 boundary/error coverage, curated API guards, documentation, and extensive
conformance/integration regressions. No dependency changed.

## Remaining non-blocking concerns

None
