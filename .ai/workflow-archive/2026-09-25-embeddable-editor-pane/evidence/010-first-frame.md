# Evidence — Task 010 first-frame prerequisite

## Scope and authorization

The user's 2026-09-27 "resolve it now please" moved the previously recorded
first-frame failure into Task 010's opening prerequisite. Approved revision 3
hash remains `d10dd746bb31ecadc41ab8212c3d775437526ac8801b5304675c3f8bbe606389`.
This evidence completes that prerequisite, not the standalone migration or the
work package. Completed Tasks 001–009 and their historical evidence are unchanged.

## Diagnosis and implementation

The unchanged full release benchmark reproduced source first-frame worst
206.68 ms against the original <150 ms budget. Profiling the same seeded 1 MiB
fixture showed about 183 ms in block parsing and 375 ms in cold rendered layout.
Generated block/inline parsers explicitly disabled release optimization, and
ordinary prose produced repeated word/whitespace/punctuation reductions.
Rendered literal-leaf normalization and wrapping repeatedly allocated/cloned
per-character strings and temporary source atoms.

- Both checked-in generated parsers now honor the existing Cargo compiler
  profile when `OPT_LEVEL` is nonzero; level-zero debug behavior is unchanged.
- The block scanner reuses its already-complete table-header scan for a narrow
  prose token: unindented ASCII-letter-starting top-level lines at document
  start or after a completed blank, without unescaped table pipes. Original
  container, continuation, table, fence, HTML and definition paths remain.
  Full block parsing is synchronous and complete inline parsing remains intact.
- Literal parser leaves use a byte-exact fast path only when raw and displayed
  text are identical and contain no line endings; transformed escapes/entities
  and code normalization retain the original paths.
- Wrapping borrows the styled projection without cloning fragments/constructing
  temporary source atoms, borrows display groups and moves original mapped
  fragments to output. Styles and source ownership remain attached.
- Added make-owned profiling, focused regression and offline grammar
  regeneration workflows. Original runtime dependency versions/graph, MIT
  licensing and pinned C headers are unchanged. Restored shared grammar inputs
  originate from the patch's recorded exact upstream commit.

Discarded diagnostic alternatives (including dense-table rewriting) are not in
the final tree. No scanner instrumentation, profiling compiler flags, runtime
grammar loading, new Cargo dependency, weakened limit or altered fixture remains.

## Test-forward and main-thread review

- Build-profile guard reproduced red before the compiler correction.
- Three parser-leaf regressions passed against the original implementation
  before its extraction/optimization and afterward: UTF-8/combining/ZWJ byte
  groups, escaped ownership, entities and normalized code line endings.
- Borrowed styled projection equals the original owned projection, including
  empty fragments, Unicode, synthetic spacing and multiple semantic styles.
- Exact mapped-wrap output fingerprint was captured before moving/borrowing
  optimization and remains unchanged across repeated/Unicode/zero-width text,
  generated ownership, widths 0–17 and hanging indents 0–5.
- Fourteen block/inline fixtures were characterized against the pre-batching
  parser. Every named node's kind, byte range and point range, all highlighted
  source lines and reference labels retain the original SHA-256 fingerprint
  `204fe1f5973599c2f2778221ba4c4b84ebf0325c4d1b74c34ed2c392521d9dd7`.
- Structural edits at every UTF-8 boundary of eight prose/CRLF/Setext/reference/
  quote/list/fence/HTML fixtures compare incremental and fresh highlighting;
  newline, heading, pipe, reference, Unicode and fence insertions plus every
  single-scalar deletion pass. Existing randomized incremental and rendered
  provenance/property suites also pass in the full gate.
- Five generator tests cover exact CLI pin, ABI/directive drift rejection,
  extension-environment isolation, pinned header retention and failure cleanup.
- Scratch regeneration through `make grammar-generate` with Tree-sitter 0.26.3
  produces byte-identical `parser.c`, `grammar.json` and `node-types.json`.
  Parser SHA-256: `03bbb725e88815c2ba21f62f03f214fb848da01e8308107f3ba6ca1df4bc2d4e`.
  The original upstream grammar had first been reproduced byte for byte before
  adding the token. The temporary official Linux-x64 CLI archive checksum was
  verified against its published release digest before execution:
  `4f65c8d9ba32a3e37198302569b3306f037f12d8313e3f28cdf1b80c9f2b3a3a`.

Reviewed scanner eligibility, serialized blank-line state, speculative scanning,
unchanged inline/source ranges, release/debug compiler behavior, fragment move
ownership and synthetic source-less padding. No public editor API or mode change.

## Validation

| Command | Result |
| --- | --- |
| `make test-first-frame` | PASS: byte-exact leaves/wrapping, build contract, original named-tree/highlighting fingerprint and structural-edit equivalence |
| `make grammar-generation-test` | PASS: 5 cases, included in both full test paths |
| `make grammar-generate TREE_SITTER=… GRAMMAR_OUTPUT=…` + byte comparisons | PASS: exact pinned regeneration in scratch |
| `make bench` | PASS: unchanged complete release gates, all 15 standalone TUI cases and owned-frame conversion |
| `make bench-check` | PASS: complete spell/core/TUI asserting debug performance gates |
| `make check` | PASS: 7/7 format, warning-fatal lint, warning-free build, entire test suite, deny, audit and bundled-data licensing |
| `git diff --check` | PASS |

The first sandboxed `make check` passed code/tests but could not write Cargo's
advisory cache for deny/audit. Repeating the entire unchanged command with
approved cache/network access passed every gate; no audit finding was waived.

Selected final release observations on this host:

| Measurement | Worst | Original limit |
| --- | --- | --- |
| Core 1 MiB source first-frame | 39.49 ms | <150 ms |
| Core 1 MiB cold rendered layout | 159.24 ms | <250 ms |
| Core 1 MiB retained rendered layout | 46,869,472 bytes | ≤64 MiB |
| Complete standalone source/rendered first-frame and rendered RSS | PASS | <150 / <350 ms; ≤192 MiB |
| 200×60 owned-frame conversion p95 | 482.725 µs | ≤1 ms |

Task 010 now proceeds to the standalone public-pane migration. Its full parity,
same-host comparison and task completion gates still apply; Task 014 retains
integrated re-verification.
