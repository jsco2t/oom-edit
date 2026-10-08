# Final Work-Package Evidence

## Work package

- ID: `2026-10-08-editor-pane-minimal-status-bar`
- Title: EditorPane minimal status bar

## Original objective

Give embedded `EditorPane` hosts an option to suppress ordinary status-row help and middle content, leaving the modal badge and right spelling/ruler indicators. Retain active command and search prompts. Write tests before implementation.

## Completed tasks

- Task 001 — Add and verify minimal EditorPane status row: [task evidence](001.md).

## Whole-package review

Reviewed the request, approved plan, task and evidence, public API, rendering path, docs, and cumulative source diff. The new option follows the existing pane presentation path; the core editor and input dispatch remain unchanged. No high-confidence defects or incomplete acceptance items remain. `git diff --check` passed. The pre-existing `.codex/config.toml` change was excluded from this work.

## High-confidence findings fixed

- Corrected the guide to state that transient notices hidden by minimal mode are not exported as host events.

## Final acceptance criteria

- Public construction option: `PaneOptions::minimal_status_bar` is public, documented, default false, and exercised by external-consumer/API tests.
- Minimal non-prompt row: external-consumer tests verify blank middle, mode badges, spelling count, and ruler with both `inline_hints` settings, dirty state, pending which-key, and a transient notice.
- Active prompts: tests verify ex/search text and cursor position on the status row.
- Metadata and modes: tests cover all four badges and verify `status()`, `hints()`, and `which_key()` remain available.
- Default and standalone behavior: default presentation comparison and existing standalone snapshots/parity tests pass.
- Verification: all focused commands and both `make check` runs pass.

## Final quality gate

| Command | Result |
| --- | --- |
| `make check` | PASS: 7/7 gates, zero failures |

## Cumulative diff

From baseline `04b37e53d1d3355f8fd0bd35946dca124d66bca4`, added one host presentation option, routed it through App/status rendering, added public regression/API tests, and documented the behavior and hidden-notice limitation. Changes remain uncommitted on branch `feature/editor-pane-minimal-status-bar`; HEAD therefore still equals the baseline SHA. The unrelated `.codex/config.toml` edit predates this package.

## Remaining non-blocking concerns

Minimal mode intentionally hides ordinary transient notices; the public API does not export them for separate host rendering.

## Post-completion review

The requested local review found one high-confidence test gap: the which-key assertion rendered before the hint delay elapsed. The regression test now renders after the 150 ms delay. Documentation review found no issues at the 80% threshold. After the fix, `make check` passed all seven gates and a separate full `make test` run passed.
