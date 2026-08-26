# Theme Support and Gutter Diagnostics

## Objective

Implement the complete theme-support package defined by the supplied PRD: replace static theme lookup with one owned TUI-private catalog; retain the three current themes without changing their existing semantic or body styling; add five attributed dark themes; load strict user-authored dark and light TOML palettes deterministically at startup; theme the entire line-number gutter independently; show asynchronously projected, provider-neutral Trouble severity markers in both source and rendered gutters; extend help, documentation, license notices, and compliance guards; and prove the hot path through retained pre-change and candidate performance evidence.

The result remains a private `oom-edit` implementation. `oom-edit-core` stays terminal-independent, the public `oom-edit` facade remains exactly `Args`, `ParseOutcome`, and `run`, and no dependency, network feature, runtime plugin, or second theme registry is introduced.

## Current behavior

- `crates/oom-edit/src/theme.rs` owns three static themes whose names and palette rows borrow `'static` data. The static `BUILTIN_THEMES` slice drives lookup, compatibility, fallback, tests, and cycling.
- `App` stores only a theme name, calls the global lookup while rendering, and calls the global cycle function before persisting the active light/dark config slot.
- Startup loads config and resolves the selected built-in before terminal setup, which is already the correct boundary for user-theme discovery and warnings.
- The source and rendered screens share `screens::editor::render_gutter`, but that renderer applies a fixed dim style and does not receive theme state. `UiSlot::Gutter` and `UiSlot::GutterCurrent` therefore do not currently control gutter cells.
- Core diagnostics are canonical, mutation-aware, and published through bounded `spell_tick` work. The TUI idle loop already advances one bounded unit at a time and yields to input. The Trouble overlay currently projects diagnostic offsets only when the overlay is opened or refreshed; there is no per-tab gutter marker summary.
- The existing test suite has strong theme completeness, exact-default, monochrome, renderer cell, startup, event-loop, public-API, dependency, and bundled-data compliance guards.
- `make bench` and `make bench-check` exercise core/spell performance but do not cover the TUI `App::render`, shared gutter painter, marker lookup, CPU/RSS evidence, or same-host comparison.
- `--licenses` currently embeds only the SCOWL license, while `THIRD-PARTY-NOTICES.md` and `scripts/check-data-licenses.sh` already provide the canonical pattern to extend.

## Proposed implementation

Work proceeds in the PRD-mandated order. First add a private TUI performance harness and make-owned measurement/comparison commands. Preserve the original five-trial evidence that exposed the source/rendered contract mismatch, add documented large-document rendered targets, repair the localized quadratic rendered-line-number calculation, and capture five clean post-repair/pre-theme baseline trials while theme, gutter, and marker behavior remains unchanged. A failure of either source or rendered absolute targets, non-quiescent editor, growing steady-state memory, or incomparable evidence stops the package rather than being discarded or silently rebaselined.

Next commit the exact pinned palette provenance and raw license inputs, expand the canonical notice surface, and make `--licenses` emit that canonical content. Refactor the theme representation into owned tables lowered from an application-owned semantic role palette, materialize the existing three themes plus Catppuccin Mocha, Dracula, Nord, Solarized Dark, and Tokyo Night, and make a single `ThemeCatalog` own lookup, compatibility, fallback, cycling, and rendering. Exact legacy palette assertions remain unchanged; only new body/gutter slots are asserted separately.

Add a strict startup-only user-theme loader for direct, lexically sorted `.toml` files beneath the existing config directory's `themes/` child. Enforce lowercase kebab-case filename identity, reserved built-in names, complete 18-role palettes, exact `#RRGGBB`, closed optional ANSI names, unknown-field rejection, independent per-file warnings, and a bounded 65,537-byte read that accepts exactly 65,536 bytes. Build the catalog and print warnings before terminal setup, then pass catalog ownership into `App` and use it for resolution and persistence.

Introduce a private gutter module containing a compact immutable source-line/severity snapshot and a resumable generation-tagged builder. Each tab owns its completed and pending presentation state. Open/replace starts empty; edits, provider reset, and spell disable invalidate completed and pending state in O(1); the existing idle loop begins a build only after a current diagnostic publication and advances a bounded number of offsets per unit. Rendering receives only the immutable completed snapshot and looks up visible numbered lines. Explicit severity priority is Error, Warning, Info, Hint, carried by fixed `E`, `W`, `I`, and `H` glyphs and an application-owned modifier.

Wire the new document-body and gutter styles into both renderers. Fill the complete gutter rectangle before composing ordinary/current line numbers and markers. Reserve one existing separator cell for the marker, retain the gutter background under every layer, and ensure wrapped/synthetic continuation rows never claim markers. Finish with integration/documentation/compliance guards, record five final candidate trials on the same machine/toolchain, generate the comparison TSV, and run every final repository and performance gate.

## Architectural decisions

- `ThemeCatalog`, theme file DTOs, load warnings, origins, and palette lowering remain private to `oom-edit`; no public facade or core API changes are permitted.
- `Theme` and tier tables become owned values. A single ordered built-in declaration remains the source of truth for name, appearance compatibility, palette seed, order, and attribution identity. Valid user entries append in lexical filename order.
- `App` owns the catalog for the session lifetime and queries it by stable name. No fallback static lookup, separate user registry, or duplicated cycle list remains.
- `default-dark`, `default-light`, and `accessible` retain every existing semantic and UI assignment. The new body slot is terminal-default for them; new gutter-only values are separately asserted. `accessible`, `NO_COLOR`, and `TERM=dumb` remain color-free.
- The user format exposes only the PRD's 18 semantic RGB roles and optional closed ANSI roles. The application owns all modifiers, diagnostic glyphs, and monochrome lowering. No inheritance, include, hot reload, multi-directory precedence, arbitrary style scopes, or terminal escape input is accepted.
- Theme files are read only from the explicit config-root `themes/` direct children. Each file is independently bounded, decoded, parsed, and validated, yielding exactly one stable path-specific warning on rejection.
- Core diagnostics remain the only diagnostic authority. `GutterTroubleSnapshot` is a lossy App-owned projection containing only source line and highest severity; it never retains diagnostic messages, source text, or renderer layout DTOs.
- Marker invalidation uses existing observable spell pending/publication transitions around session mutations, so no new core mutation API is introduced. Once invalid, later keys cannot expose stale markers; motions do not discard a current snapshot.
- Marker construction is cooperative idle work with explicit item/byte bounds and generation cancellation. No thread, async runtime, channel, or duplicate document text is added.
- The shared gutter renderer accepts theme/tier plus an immutable marker snapshot and remains pure over visible line numbers. It never receives `EditorSession`, diagnostics, a provider, source text, or mutable cache state.
- ANSI-16 mappings are explicit semantic choices, never RGB quantization. Monochrome is shared and application-controlled.
- Pinned palette files, raw licenses, provenance, mapping notes, canonical notices, registry attribution, and `--licenses` are cross-checked by the existing bundled-data gate. Palette data does not change Cargo dependencies.
- TUI performance infrastructure stays private/test-only. New developer workflows are exposed through documented make targets; portable measurement uses the platform `/usr/bin/time` through a make-owned script and no new Rust dependency.
- NFR-1 remains construction plus the first highlighted 1 MiB source viewport frame below 150 ms. The rendered contract is separate: on the pinned 1 MiB/approximately 100-column fixture, cold full layout is below 250 ms worst, end-to-end rendered first frame is below 350 ms worst, retained layout heap is at most 64 MiB, and one-shot peak RSS is at most 192 MiB on the baseline machine. Doubling 256 KiB → 512 KiB → 1 MiB increases cold-layout time and retained heap by at most 2.25× per step.
- The existing per-row prefix scan in rendered line-number projection is replaced with one source line index or an equivalent linear-time method. This focused repair must preserve exact row/source numbering and provenance; viewport-lazy rendering, compact atom redesign, and other large renderer architecture changes remain out of scope.
- Baseline, candidate, and comparison TSV files are written beside the supplied PRD under `evidence/performance/`. Their fixed schema, five-trial completeness, environment comparability, aggregation, and thresholds are validated before comparison.
- The original failed TSV is retained under a distinct pre-optimization evidence name. The comparison baseline is captured only after the approved performance repair and before theme/gutter/marker production changes, using a bumped fixture version so incompatible case semantics cannot compare.

## Work included

1. A real `App::render`/`TestBackend` TUI performance harness covering distinct source-first-frame and rendered-first-frame cases, source and rendered steady-state frames, edit/scroll boundaries, memory stability, and idle quiescence, plus deterministic work counters and TSV writer/parser/comparator support.
2. Makefile integration for relaxed debug TUI smoke, exact release TUI cases, five-trial evidence recording, CPU/RSS normalization on macOS/Linux, and baseline/candidate comparison.
3. A focused linear-time rendered-line-number repair; repository performance documentation; deterministic large-layout time, heap, RSS, and 2.25× size-scaling gates; retained failed pre-optimization evidence; and a clean five-trial post-repair/pre-theme baseline.
4. Exact pinned palette and license assets for Catppuccin Mocha, Dracula, Nord, Solarized Dark, Tokyo Night, and Tokyo Night's credited Enkia origin; complete provenance, separate oom-edit mapping notes, full canonical notices, and hash guards.
5. Owned palette tables and one builder from the semantic role palette into all `SemanticStyle` and `UiSlot` rows at TrueColor, ANSI-16, and Monochrome.
6. The exact eight-theme built-in registry and catalog order, exact legacy-default preservation, complete accessibility carriers, body/gutter surface slots, and curated built-in anchor/contrast assertions.
7. Strict deterministic custom-theme parsing, bounded discovery, typed warnings, per-file isolation, catalog integration, mode-aware resolution, fallback, cycling, persistence, and startup warning order.
8. Per-tab immutable gutter marker summaries, bounded pending builds, explicit severity deduplication, generation cancellation, mutation/provider/disable invalidation, input preemption, and idle quiescence.
9. Full-rectangle gutter theming and one-cell `E`/`W`/`I`/`H` markers in source Insert and rendered modes, including wrapping, synthetic rows, relative/multi-digit numbers, narrow areas, scrolling, and monochrome.
10. End-to-end private integration tests for file-loaded custom themes, startup fallback/warnings, persistence, real diagnostic publication, both renderers, tab isolation, and edit clearing.
11. CLI help, README theme schema/location/built-in documentation, canonical `--licenses`, public API/dependency/color-locality guards, and strengthened data-license compliance.
12. Five-trial candidate evidence, generated median/worst comparison, absolute/scaling checks, and the complete final repository gate.

Every code or script change receives tests in the same task. Tests follow existing colocated Rust tables, `TestBackend` cell assertions, temporary directories, injected environment/services, and shell compliance patterns. Symbol snapshots are used only when glyph/layout coverage adds value; color/background behavior is asserted on cells.

## Task sequence

1. `tasks/001-performance-harness-and-baseline.md`
2. `tasks/002-theme-provenance-and-notices.md`
3. `tasks/003-owned-theme-catalog-and-builtins.md`
4. `tasks/004-custom-theme-loading-and-startup.md`
5. `tasks/005-gutter-trouble-state.md`
6. `tasks/006-app-idle-and-tab-integration.md`
7. `tasks/007-gutter-rendering-and-markers.md`
8. `tasks/008-integration-documentation-and-guards.md`
9. `tasks/009-candidate-performance-and-comparison.md`

Tasks execute strictly in this order. Task 001 must retain the failed diagnostic evidence, land the approved focused performance repair and documentation, then capture a clean baseline before Tasks 003–007 change theme/gutter/marker production behavior. Task 009 must use that exact post-repair baseline and matching environment metadata.

## Quality gate

The standard per-task gate is `make check`. It is the repository's authoritative aggregate and runs format checking, Clippy with warnings denied, the workspace build, the complete Rust and workflow-helper test suites, cargo-deny, cargo-audit, and bundled-data license compliance. Running its components again as separate standard commands would duplicate the same gate without increasing coverage.

The final gate runs `make fmt-check`, `make check`, `make bench-check`, and `make bench` exactly as required by the PRD and repository definition of done. `make bench-check` includes deterministic debug shape/scaling checks; `make bench` asserts both source and rendered release ceilings plus layout heap and process RSS. The gate then runs the make-owned TSV comparison against the retained external post-repair baseline and candidate evidence. There is no separate type-checker target in this Rust repository: compilation and type checking are covered by `make check`/`make build`, while warning-fatal lint is covered by the same aggregate.

No dependency change is planned. `Cargo.lock` and `vendor/` must remain unchanged. If implementation proves a dependency is necessary, the approved plan is no longer valid and execution must stop for explicit revision rather than silently adding it.

## Risks

- The static-to-owned conversion can alter exact legacy styles or create a second lookup path. Exact old tables, a single catalog owner, and source-level registry guards mitigate this.
- Theme parsing is a startup attack surface. Bounded reads before decode/parse, strict DTOs, direct-child discovery, closed identifiers/colors, and one-file isolation constrain it.
- Full license/provenance assets must match immutable upstream bytes. All copies are verified against PRD-pinned hashes and canonical notices; newer branch tips are not substituted.
- Marker projection could accidentally move into open, mutation, layout, or paint. Work counters, immutable renderer signatures, bounded idle tests, and performance evidence guard every boundary.
- A pending build could publish after an edit, replacement, provider change, spell disable, or tab switch. Per-tab generations and atomic completed-snapshot replacement prevent stale publication.
- Styling layers can erase the gutter background or displace line numbers. Exact cell matrices cover the whole rectangle, narrow widths, continuations, marker composition, and both modes.
- Performance evidence can be noisy or incomparable. The comparator rejects machine/toolchain/schema/fixture mismatches and incomplete trials; the workflow stops rather than discarding or rebaselining evidence.
- Process RSS is allocator/OS-sensitive, so the 192 MiB ceiling is enforced on the recorded one-shot baseline environment while the portable primary memory guard deterministically counts retained `RenderedLayout` capacities against 64 MiB. The original 598 ms evidence remains historical and is never overwritten without preservation.
- Writing performance evidence beside the external PRD and retrieving immutable upstream license inputs may cross runtime filesystem/network authorization boundaries. Execution will request the required authorization when those approved tasks reach that boundary.

## Out of scope

- Gruvbox, Rosé Pine, bundled light siblings, or any palette not named in the PRD.
- Theme inheritance, includes, arbitrary style scopes, Helix/VS Code/Neovim compatibility, user-controlled modifiers/glyphs, multiple theme directories, hot reload, file watching, plugins, or network retrieval at runtime/build time.
- Moving themes, terminal colors, gutter rendering, or App-owned marker projection into `oom-edit-core`.
- A theme-list command, automatic custom-theme contrast rejection, or rejecting a custom theme whose gutter equals its body.
- A worker thread, async runtime, allocator instrumentation, unsafe profiling hooks, or new measurement dependency.
- Per-theme symbol-only golden snapshots or live interactive terminal screenshot automation.
- Viewport-lazy rendered construction, provenance-atom compaction, renderer storage redesign, changes to core diagnostic decoration projection, provider semantics, Markdown parsing, editing behavior, public exports, Cargo dependencies, `Cargo.lock`, or `vendor/`.

## Final acceptance criteria

- The catalog contains exactly `default-dark`, `catppuccin-mocha`, `dracula`, `nord`, `solarized-dark`, `tokyo-night`, `accessible` for dark cycling and `default-light`, `accessible` for light cycling, with lexical compatible user themes appended; lookup, validation, resolution, rendering, and persistence use that single catalog.
- `default-dark`, `default-light`, and `accessible` retain all existing semantic/body/status/cursor/syntax values and selection behavior. Only independently asserted body/gutter additions exist; the defaults' body remains terminal-default and `accessible` remains color-free.
- Each of the five added bundled dark themes uses the PRD-pinned palette inputs, has complete TrueColor/ANSI-16/Monochrome lowering, a body surface, a distinct bundled gutter surface, curated contrast checks, and exact attribution metadata. Gruvbox and Rosé Pine are absent.
- A complete dark or light user TOML file in the config `themes/` directory is selectable through CLI, environment, the active config slot, and cycling without recompilation. Strict schema, exact color syntax, closed ANSI names, filenames, reserved names, 65,536/65,537-byte boundaries, deterministic ordering, per-file isolation, and startup-only behavior are covered by tests.
- Missing/invalid/unreadable/non-UTF-8/oversized/colliding themes produce exactly one ordered path-specific pre-terminal warning per rejected file, do not invalidate valid siblings, and cause only an unavailable selected name to fall back to the matching default—never implicitly to `accessible`.
- The entire gutter rectangle in both source and rendered modes uses the selected gutter background, normal/current number styles, and preserves the marker/number/background composition across numbered, wrapped, synthetic, scrolled, clipped, and below-document rows.
- Every numbered source line with a published diagnostic displays exactly one fixed severity glyph (`E`, `W`, `I`, or `H`), highest severity wins in the explicit order Error > Warning > Info > Hint, and glyph plus modifier remains meaningful without color.
- Open, replace, edit, provider reset, and spell disable perform only O(1) marker invalidation and no diagnostic scan or offset projection. Bounded idle units build per-tab summaries, yield to input, cancel stale generations, publish atomically only when complete, and become quiescent.
- Rendering consumes only an immutable completed summary, looks up only visible numbered rows, never advances diagnostics or marker construction, does not grow snapshot memory, and remains stable across viewport/wrapping/horizontal-scroll changes.
- CLI help describes built-in or user theme names; README documents all built-ins, user directory/schema, selection/fallback/cycling, and limitations; `--licenses` emits the exact canonical `THIRD-PARTY-NOTICES.md` content before terminal setup.
- Provenance and raw license assets match every pinned SHA/revision/source in the PRD; canonical notices contain each complete applicable license and declared attribution; registry, provenance, notices, raw assets, and CLI output have one-to-one drift-prevention coverage under `make data-license-check`.
- Public API guards still expose only `Args`, `ParseOutcome`, and `run`; core remains terminal/network independent; colors stay confined to the TUI theme module; no dependency, lockfile, or vendor change occurs.
- The TUI performance harness covers all PRD source/rendered/open/edit/scroll/projection/quiescence cases, fixed TSV schema, CPU/RSS normalization, rendered-layout and marker-snapshot memory, visible-row work, and comparator boundary behavior through make-owned commands.
- NFR-1's source first frame remains below 150 ms. The pinned 1 MiB rendered workload stays below 250 ms cold layout and 350 ms end-to-end first frame, owns at most 64 MiB of retained layout heap, stays below 192 MiB one-shot peak RSS on the evidence host, and grows time/heap by at most 2.25× for each 256 KiB → 512 KiB → 1 MiB doubling.
- The original failed five-trial evidence is retained distinctly; five complete post-repair/pre-theme baseline trials and five matching final-candidate trials are retained at the PRD evidence paths, and `comparison.tsv` proves all median/worst timing, CPU, RSS, layout/snapshot-memory, viewport/scaling, quiescence, NFR-1, NFR-2, and rendered targets pass without unexplained retry or silent rebaseline.
- `make fmt-check`, `make check`, `make bench-check`, `make bench`, and the baseline/candidate comparison command all pass with no warnings or errors.

## Acceptance follow-up — Round 2: Leading gutter diagnostics

### Observed acceptance failure

The completed round-1 gutter puts a diagnostic glyph in the first trailing content-gap cell. A numbered row therefore composes as `number + marker + separator`, represented by the reported `  73W|Line of text`. The accepted visual contract is instead `marker + aligned number + separator`, represented by `W 73 |Line of text`.

### Repository finding and root cause

Both source and rendered modes already use the single private `screens::editor::render_gutter` painter. It obtains a right-aligned label from `status_bar::build_gutter`, then derives `marker_column` as the label length minus the two-cell content gap. That calculation deliberately selects the first trailing gap cell, which directly causes the reported placement. The existing `status_bar::gutter_width` already reserves a leading sign/alignment column and enough trailing separation, so no new column or layout-width change is required.

### Additive fix strategy

- Change only the shared gutter composition so a published `E`, `W`, `I`, or `H` replaces the first leading alignment cell at normal usable widths; every remaining label cell keeps its current terminal coordinate.
- Preserve the existing right-aligned absolute/relative number field after the marker, including `+`/`-` signs and digit-boundary alignment; do not insert a column or shift the formatter output.
- Preserve the final content gap/separator, total gutter width, source/rendered text origin, viewport width, cursor mapping, wrapping, and horizontal-scroll behavior.
- Keep unmarked rows visually equivalent to the existing gutter: the leading marker cell remains blank, numbers retain their alignment, and the separator remains clear.
- Preserve whole-gutter theming and marker role/modifier styling at TrueColor, ANSI-16, and Monochrome tiers.
- Define deterministic clipped behavior: a present marker remains the first visible cell when any gutter cell is available, and composition never writes outside the clipped area.
- Update the README marker description to state that the fixed severity glyph precedes the aligned line number.

### Architecture and test impact

The change remains in the private shared TUI gutter renderer. It does not alter `GutterTroubleSnapshot`, diagnostic publication, `status_bar::gutter_width`, core APIs, App state, dependencies, or performance evidence schemas. Exact cell/string tests will cover marked and unmarked rows, all four severities and tiers, absolute and hybrid-relative numbering, 9/10/999/1000 boundaries, narrow clipping, wrapped/synthetic rows, rendered horizontal scroll, and an App-published diagnostic in both modes. Existing source/rendered viewport and cursor-origin tests must remain green.

### Round-2 task sequence

10. `tasks/010-relocate-gutter-markers-before-line-numbers.md`

Task 010 is the only round-2 task and depends on the immutable round-1 delivery.

### Round-2 risks

- Moving the glyph without preserving the formatter's alignment field could shift signs or multi-digit numbers. Exact boundary matrices guard the full row.
- Changing the computed gutter width would reduce document width and disturb cursor/wrap behavior. The fix must keep width and body origin unchanged.
- Applying number styling over the marker or marker styling over padding could regress theme/accessibility behavior. Per-cell tier assertions keep the layers distinct.
- Very narrow terminal areas can truncate the number field. Explicit clipped tests require bounded marker-first output without panic or out-of-area writes.

### Round-2 out of scope

- New marker glyphs, severities, configuration, animation, or user-controlled marker placement.
- Changes to gutter width, line-number policy, relative-number semantics, separator geometry, diagnostic projection, Trouble state, or source/rendered layout ownership.
- Re-recording or rebaselining the frozen five-trial round-1 performance evidence. Live `make bench-check` and `make bench` continue to enforce the changed renderer.
- Core/public API, Cargo dependency, lockfile, vendor, theme-palette, or licensing changes.

### Round-2 acceptance criteria

- At the normal computed gutter width, a warning row renders in the reported order `W 73 |Line of text`: the marker is the first gutter cell, alignment padding and the complete right-aligned line number follow it, and the existing final separator gap remains immediately before document text.
- Unmarked rows retain the corresponding `  73 |Line of text` geometry, and marked versus unmarked rows start document text at the identical terminal column.
- `E`, `W`, `I`, and `H` all use the new leading position in source and rendered modes while retaining their exact role color, bold modifier, monochrome carrier, and gutter background.
- Absolute, current-line, and signed hybrid-relative numbers remain complete and aligned across 9/10/999/1000 document boundaries; a marker never overwrites a digit, sign, or final separator cell.
- Wrapped and synthetic continuation rows remain marker-free; rendered markers remain tied to numbered source rows across viewport movement and horizontal scrolling.
- Clipped gutters remain safe and deterministic, with a present marker in the first visible cell whenever width is nonzero.
- Gutter width, document-body origin, source viewport width, cursor placement, rendered layout, diagnostic state, and render-time state/memory behavior remain unchanged.
- README wording matches the leading-marker contract, and `make fmt-check`, `make check`, `make bench-check`, `make bench`, and the retained-evidence comparison all pass.

## Acceptance follow-up — Round 3: Compact gutter content gap

### Observed acceptance failure

The completed round-2 gutter correctly places the diagnostic marker before the aligned line number, but it retains the existing two-cell gap between the line number and document text. An unmarked row therefore reads `  73  |Line of text`; the accepted compact geometry is `  73 |Line of text`. The diagnostic marker is already correct and must not change.

### Repository finding and root cause

`crates/oom-edit/src/widgets/status_bar.rs` defines `GUTTER_CONTENT_GAP` as two cells. Both `format_gutter_cell` and `gutter_width` derive their geometry from that constant, and both source and rendered screens use the resulting shared gutter width and `screens::editor::render_gutter` painter. Reducing the shared content gap from two cells to one removes exactly the unwanted trailing cell while retaining the existing number field. Because the marker renderer replaces the first leading alignment cell established in round 2, no marker-placement logic or diagnostic state change is required.

### Additive fix strategy

- Reduce the shared gutter content gap from two cells to one and let the existing source and rendered layout calculations consume the resulting width.
- Preserve the current line-number field width, right alignment, signed hybrid-relative labels, and leading diagnostic-marker cell; remove only one trailing blank cell.
- Preserve the marker's fixed first-column position, severity glyph, role style, bold/monochrome carrier, background composition, and source-line association.
- Make the source and rendered document body start exactly one terminal column earlier. Keep cursor placement, viewport width, wrapping, horizontal scrolling, provenance, and clipping internally consistent with that intentional geometry change.
- Update exact row/cell tests and only those golden fixtures whose body placement changes by the expected single column.

### Architecture and test impact

The change remains in the existing private shared gutter formatter and its TUI consumers. It does not introduce a second geometry path or alter diagnostic projection, marker snapshots, themes, App state, core/public APIs, dependencies, or performance evidence schemas. Exact tests will cover marked and unmarked line 73 rows, absolute and hybrid-relative 9/10/999/1000 boundaries, source/rendered text origins, cursor and viewport calculations, wrapping/synthetic rows, narrow areas, horizontal scrolling, and intentional golden shifts. The live performance gates continue to exercise the revised geometry without replacing frozen round-1 or round-2 evidence.

### Round-3 task sequence

11. `tasks/011-reduce-gutter-right-padding.md`

Task 011 is the only round-3 task and depends on the immutable round-2 delivery.

### Round-3 risks

- Removing a cell from the number field instead of the content gap could misalign signs or digit-boundary labels. Exact formatter matrices require the number field to remain unchanged.
- A stale hard-coded gutter width could make source/rendered origins, cursor coordinates, wrapping, or snapshots disagree. Cross-mode geometry tests require every consumer to derive the one-column reduction consistently.
- Marker composition currently depends on the formatted gutter shape. Marked and unmarked exact-cell tests require the marker to remain at column zero with unchanged styling.
- Very narrow areas could expose subtraction or clipping assumptions after the width reduction. Explicit zero/small-width tests retain bounded, panic-free behavior.

### Round-3 out of scope

- Moving, restyling, renaming, configuring, or otherwise changing diagnostic markers, severities, projection, publication, or Trouble state.
- Changing line-number policy, relative-number semantics, leading alignment, number-field width, gutter theming, or separator ownership beyond removing the single trailing blank cell.
- Re-recording or rebaselining frozen performance evidence. Live `make bench-check` and `make bench` continue to enforce the updated renderer.
- Core/public API, Cargo dependency, lockfile, vendor, palette, license, configuration, or documentation-schema changes.

### Round-3 acceptance criteria

- At normal width, an unmarked line 73 has the exact gutter string `  73 ` and conceptual row `  73 |Line of text`, with one and only one blank cell between the final digit and document text.
- A warning on line 73 has the exact gutter string `W 73 ` and conceptual row `W 73 |Line of text`; the marker remains the first gutter cell and differs from round 2 only because the same trailing blank cell was removed.
- The computed gutter width decreases by exactly one cell at every document digit boundary while the number field remains unchanged: documents ending at lines 9, 10, and 100 use five cells, and line 1000 uses six.
- Absolute, current-line, and signed hybrid-relative rows retain complete signs/digits and alignment across 9/10/999/1000 boundaries, with exactly one trailing content-gap cell.
- Source and rendered document bodies, cursor coordinates, and available text widths move consistently by exactly one column; wrapping, viewport movement, provenance mapping, selection, and horizontal scrolling remain correct.
- Marker glyph, position, severity priority, foreground/background/modifier behavior, accessibility carrier, snapshot state, and render-time work/memory behavior are unchanged.
- Wrapped and synthetic rows remain marker-free and fully gutter-themed; zero/small-width areas remain bounded and panic-free.
- Any updated golden fixture differs only by the intentional one-column gutter/body shift, and no unrelated docs, APIs, dependencies, lockfile, vendor tree, theme assets, or retained evidence changes occur.
- `make fmt-check`, `make check`, `make bench-check`, `make bench`, and the retained-evidence comparison all pass.
