# Editor DRD coverage

`edit_drd.tsv` maps all 91 editor DRD requirements to named executable cases,
literal fixtures, expected outcomes and make-owned workflows. A requirement can
have several cases: no single entry is intended to stand in for a whole test
family. FR-066 is the sole exclusion because the DRD explicitly classifies
comment/format preservation as Nice-to-Have; semantic TOML preservation remains
covered.

Run `make coverage-check` for the strict gate. It rejects planned cases, missing
or unknown requirements, duplicate IDs, disabled or stubbed cases, external or
nonexistent locations and unknown make targets. Rust cases must appear in the
compiled workspace test inventory. An ignored release test additionally needs
an exact make recipe that actually runs it with `--ignored --exact`; benchmark
harness functions are run by their recorded benchmark targets. Python case
definitions are checked directly. `make drd-coverage-test` mutation-tests these
guards. `make ci` runs coverage alongside the full test and performance suites.

The inventory is traceability, not a coverage-percentage claim or an automatic
proof of assertion quality. Main-thread review checks each fixture and oracle
against the executable test. Behavioral red/green observations, unchanged
extraction characterizations and complete gate results are retained in
`.ai/workflow/evidence/001.md` through the corresponding implementation task's
evidence document. Task 014 records final resolution and integrated review;
Task 015 separately requires immutable-revision and native-platform evidence.
