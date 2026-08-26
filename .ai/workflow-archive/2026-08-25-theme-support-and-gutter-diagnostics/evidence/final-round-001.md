# Final Evidence — Theme Support and Gutter Diagnostics

## Approved package

- Work ID: `2026-08-25-theme-support-and-gutter-diagnostics`
- Plan revision: 2
- Acceptance round: 1
- Approved plan SHA-256: `0b09d4a8f3f905039fbf414d3b32adbf8c4db19e6d731995b9b2580c1e72d9da`
- Completed tasks: 9 of 9

## Integrated acceptance

- [x] Theme provenance, immutable bundled assets, canonical notices, and pre-terminal license output are complete and guarded.
- [x] One owned catalog supplies the exact built-in set, compatibility, fallback, cycling, custom startup-only loading, resolution, rendering, and active-slot persistence.
- [x] Source and rendered gutters use complete theme styling and exact accessible diagnostic markers without changing core/public ownership boundaries.
- [x] Diagnostics publish through bounded, cancellable, generation-safe per-tab idle work; synchronous mutations invalidate immediately and clean idle/render paths become quiescent.
- [x] README/help/performance documentation, Make targets, public/dependency/static-loading guards, and license checks describe and enforce the implemented surface.
- [x] The original source startup NFR and the additional full rendered layout/rendering/memory NFR both pass automated absolute and scaling gates.
- [x] Five final trials for all 15 TUI cases match the frozen baseline environment; every relative, absolute, memory, RSS, scaling, cancellation, and quiescence comparison passes.
- [x] No Cargo dependency, lockfile, vendor tree, public core facade, or terminal-free core boundary changed.

## Final review

The complete baseline diff and all nine task/evidence pairs were reviewed together for requirement coverage, state ownership, lifecycle invalidation, renderer geometry, accessibility carriers, startup ordering, strict parsing, license identity, Make discoverability, performance measurement integrity, and architecture constraints. No unresolved issue or deferred work remains.

## Final gates

| Command | Result |
| --- | --- |
| `make fmt-check` | PASS |
| `make check` | PASS — fmt, lint, build, full tests, deny, audit, and data licenses |
| `make bench-check` | PASS — spell, core, and 15-case TUI debug matrix |
| `make bench` | PASS — exact release core, spell, layout, and 15-case TUI gates |
| `make tui-perf-compare BASELINE=…/baseline.tsv CANDIDATE=…/candidate.tsv OUTPUT=…/comparison.tsv` | PASS — all rows |

## Performance result highlights

- Final-gate 1 MiB cold rendered layout: 170.45 ms average, 170.71 ms worst, 46,869,472 retained heap bytes.
- Recorded rendered first-frame RSS: 167,657,472-byte median, 168,198,144-byte worst.
- Recorded 50,000-marker snapshot: 800,000 bytes retained versus a 1,204,096-byte limit.
- Recorded off-screen scaling from 5,000 to 50,000 markers: source +1.23% wall/+1.23% CPU; rendered -0.85% wall/-0.92% CPU.
- Recorded idle projection/cancellation worst: 48,291 ns / 16,208 ns, both below 1 ms.

## Remaining concerns

None
