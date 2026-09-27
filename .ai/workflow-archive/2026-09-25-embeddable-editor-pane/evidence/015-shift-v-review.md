# Task 015 Shift+V review and test-forward evidence

Reviewed in the main thread at 2026-09-27T14:39:40Z, approved revision 5.
Scope: 12 changed tracked files atop prior candidate
`ab30fa474a8cd816e018cbe9c3a5f5577706b2ee`; no completed task document changed.

## Behavioral evidence

- Initial test authoring compile error (`EditorSession::text` instead of the
  supported `document`) retained in `/tmp/oom-edit-task015-shift-v-red.log`;
  not claimed as behavioral red evidence.
- `/tmp/oom-edit-task015-shift-v-behavior-red.log`: runnable unchanged production
  routing fails Shift entry (Normal versus Select) and mixed-modifier line
  cancellation (Select versus Normal); Ctrl/Alt boundaries already pass.
- `/tmp/oom-edit-task015-shift-v-host-red.log`: shared translation passes, but
  real public-host mode assertion fails for shifted V. Seven existing cases pass.
- `/tmp/oom-edit-task015-shift-v-green.log`: `make test-shift-v` passes all three
  core cases, native standalone-adapter case, public-host case, complete eight-case
  embedding suite and actual standalone shared-vector test.

## Main-thread review

Reviewed the actual diff, surrounding Normal/Select routing, host adapters,
selection transitions, make target and coverage projections. No unresolved
high-confidence findings. The only production change is moving uppercase V
out of the unmodified-only match in each rendered handler; Ctrl retains earlier
block priority, Alt is excluded, and uppercase Shift is accepted. Search, pending
Vim/operator grammar and register handling remain before this routing. Inputs
are not normalized again or stripped of Shift. Existing lowercase v, Y, Ctrl+V
and escape behavior remains unchanged.

Tests assert exact UTF-8 physical line ranges under wrapping, emitted modes,
character/block-to-line switching, both cancellation forms, Ctrl/Alt boundaries,
translated events through standalone and embedded policies, exact line deletion,
undo and unchanged disk bytes. The actual standalone event adapter is exercised
with cursor capability on/off; it is not mislabeled native enhanced-protocol
verification. Shared corpus grows from 15 to 17, retaining every previous row
and asserting preserved Shift with both existing enhancement scenarios. Five
executable coverage rows are added, with no removed requirements or lowered gates.

No public API, dependency, ownership, parser, snapshot, unsafe or performance
threshold changes. Changelog reports an unreleased fix. Candidate commit and
full exact-SHA quality/performance/platform/consumer gates follow; this document
is not Task 015 completion or macOS/publication evidence.
