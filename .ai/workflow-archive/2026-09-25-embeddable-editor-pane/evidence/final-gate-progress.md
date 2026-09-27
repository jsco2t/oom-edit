# Final gate — attempt 1 checkpoint

Target: `87d5b48f766eb35f30c2136e23d2a4448329713b`, clean main and published
`v0.6.0`. Whole-package review completed with zero findings above confidence 80.
Commands below ran in the exact `gate.json` order after that review.

| Command | Observed result | Durable diagnostic log |
| --- | --- | --- |
| `make ci` | PASS, exit 0, 2026-09-27 17:55 UTC | `target/workflow-verification/final-ci.log` |
| `make build-examples` | PASS, exit 0, 17:55 UTC | `target/workflow-verification/final-examples.log` |
| `make doc` | PASS, exit 0, 17:55 UTC | `target/workflow-verification/final-doc.log` |
| `make bench-check` | PASS, exit 0, 17:56 UTC | `target/workflow-verification/final-bench-check.log` |
| `make bench` | PASS, exit 0, 17:57 UTC | `target/workflow-verification/final-bench.log` |

CI completed all seven normal checks, strict 191-case/91-requirement coverage,
examples, warning-fatal documentation and complete debug/release benchmarks.
Representative release measurements: first-frame worst 39.14 ms (<150 ms),
1 MiB layout worst 161.78 ms (<250 ms), heap 46,869,472 bytes (<=64 MiB), owned
200x60 frame p95 529.869 microseconds (<=1 ms). No warnings/errors/advisory
findings. The separate remaining benchmark has the identical unchanged limits.

All five final commands passed in order, attempt 1. Complete final evidence is
`final.md`; this checkpoint is not itself an archival instruction.
