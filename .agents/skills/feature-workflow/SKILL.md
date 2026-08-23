---
name: feature-workflow
description: Plan, human-approve, autonomously execute, and archive an ordered software work package.
---

# Feature Workflow

You are the primary engineer and workflow owner.

A workflow represents one work package.

A work package may contain:

- one feature;
- several related features;
- one bug;
- a batch of bugs;
- a mixture of related features and bugs;
- a bounded maintenance milestone.

Use subagents sparingly.

The primary thread owns nearly all reasoning, orchestration, implementation,
review, validation, and acceptance.

All paths are relative to the Git repository root.

Active workflow root:

`.ai/workflow/`

Archive root:

`.ai/workflow-archive/`

---

# Invocation modes

Supported explicit invocations are:

- `$feature-workflow <work description>`
- `$feature-workflow revise <feedback>`
- `$feature-workflow approve`
- `$feature-workflow resume`
- `$feature-workflow status`
- `$feature-workflow archive`
- `$feature-workflow history`

Determine the mode only from the explicit invocation.

Never infer approval.

Never infer archive intent.

---

# Global workflow invariant

Exactly one of these conditions must hold:

1. `.ai/workflow/state.json` exists:
   one work package is active, blocked, awaiting approval, DONE, or otherwise
   still occupies the active workflow slot.

2. `.ai/workflow/state.json` does not exist:
   no work package is active and a new package may begin.

Never create a second active work package.

Never overwrite an existing work package.

Never automatically delete a DONE work package.

---

# Work ID format

Every work package receives a stable work ID.

Use:

`YYYY-MM-DD-<filesystem-safe-slug>`

Examples:

`2026-08-23-features-1-2-3`

`2026-08-30-bugs-a-b-c`

`2026-09-06-authentication-cleanup`

Use the local calendar date.

Generate the slug from the work-package title.

Use:

- lowercase letters;
- digits;
- hyphens.

Do not use spaces.

If:

`.ai/workflow-archive/<work_id>/`

already exists, append:

`-02`

then:

`-03`

and so forth.

Never overwrite an archive.

---

# Mode: start

Start mode applies when the invocation contains a new work description rather
than:

- revise;
- approve;
- resume;
- status;
- archive;
- history.

## 1. Check for an existing workflow

If:

`.ai/workflow/state.json`

exists, read it.

Do not create a new workflow.

If:

`phase == DONE`

tell the human:

"The previous work package is complete but has not been archived. Run
`$feature-workflow archive` before starting another work package."

Stop.

For any other phase:

report the active work package, phase, and current task.

Stop.

A new work package may begin only when:

`.ai/workflow/state.json`

does not exist.

## 2. Initialize directories

Create:

`.ai/workflow/`

`.ai/workflow/tasks/`

`.ai/workflow/evidence/`

Create `.ai/workflow-archive/` if it does not exist.

## 3. Determine identity

Create:

- a concise human-readable title;
- a filesystem-safe work ID.

Examples:

Title:

`Features 1, 2, and 3`

Work ID:

`2026-08-23-features-1-2-3`

## 4. Record the request

Write the human's complete work description to:

`.ai/workflow/request.md`

Preserve requirements.

Normalize formatting if useful.

Do not silently change meaning.

## 5. Capture baseline

Run:

`git rev-parse HEAD`

Record the result as:

`baseline_sha`

## 6. Initialize state

Create:

`.ai/workflow/state.json`

with:

```json
{
  "schema_version": 1,
  "work_id": "<work-id>",
  "title": "<human-readable title>",
  "phase": "PLANNING",
  "created_at": "<UTC timestamp>",
  "updated_at": "<UTC timestamp>",
  "approved_at": null,
  "completed_at": null,
  "archived_at": null,
  "baseline_sha": "<git HEAD>",
  "final_sha": null,
  "plan_revision": 1,
  "approved_plan_sha256": null,
  "current_task": null,
  "tasks": [],
  "task_retry_limit": 3,
  "final_retry_limit": 2,
  "final_attempts": 0,
  "stop_continuations": 0,
  "max_stop_continuations": 50,
  "block_reason": null
}
```

## 7. Investigate the repository

Use the PRIMARY THREAD.

Do not spawn a worker during planning.

Inspect enough of the repository to understand:

- current architecture;
- relevant implementation;
- related tests;
- repository conventions;
- build system;
- formatter;
- linter;
- type checker where relevant;
- standard test commands;
- likely affected modules;
- compatibility requirements;
- important invariants.

Do not modify product code.

## 8. Determine the quality gate

Discover actual repository commands.

Do not invent generic commands.

Create:

`.ai/workflow/gate.json`

using:

```json
{
  "standard_commands": [
    "<command>",
    "<command>"
  ],
  "final_commands": [
    "<command>",
    "<command>"
  ]
}
```

`standard_commands` must contain the normal completion criteria appropriate
to this repository.

Examples include, where actually applicable:

- formatting checks;
- lint;
- static analysis;
- compilation;
- type checking;
- unit/integration tests.

`final_commands` should contain the complete package-level validation.

It may equal `standard_commands`.

If a class of validation genuinely does not exist in the repository, do not
invent one.

Explain the omission in `plan.md`.

## 9. Write plan.md

Create:

`.ai/workflow/plan.md`

using:

# <Work-package title>

## Objective

Describe the complete approved outcome.

## Current behavior

Describe relevant existing behavior.

## Proposed implementation

Describe the overall implementation strategy.

## Architectural decisions

Record decisions that future implementation tasks must respect.

## Work included

Enumerate the features/bugs/items represented by this work package.

Include relevant test planning. All code changes are tested. 

## Task sequence

List every task document in exact execution order.

## Quality gate

Document the commands from gate.json and why they are appropriate.

## Risks

Record meaningful technical risks.

## Out of scope

Define explicit boundaries.

## Final acceptance criteria

Define package-level observable acceptance criteria.

Do not use vague acceptance criteria.

## 10. Generate task documents

Create:

`.ai/workflow/tasks/001-<slug>.md`

`.ai/workflow/tasks/002-<slug>.md`

etc.

Every task must use:

# Task NNN: <name>

Delegation: main-only

or:

Delegation: worker-eligible

## Goal

Define one coherent result.

## Context

Explain why the task exists and how it fits into the work package.

## Scope

### In scope

Concrete work owned by this task.

### Out of scope

Related work that this task must not perform.

## Implementation requirements

Specific implementation requirements and constraints.

## Acceptance criteria

- [ ] Objectively inspectable criterion.
- [ ] Objectively inspectable criterion.

## Validation

List exact task-specific validation commands.

## Dependencies

List earlier task IDs or:

`None`

## Expected areas of change

List likely modules/files.

This is guidance, not a hard restriction.

## Risks / notes

Record important invariants and edge cases.

### Delegation classification

Use:

`Delegation: worker-eligible`

only when ALL are true:

- the task is bounded;
- architecture has already been decided;
- acceptance criteria are clear;
- implementation is primarily local or mechanical;
- the result can be independently reviewed afterward.

Use:

`Delegation: main-only`

for:

- architectural changes;
- cross-cutting design;
- migrations with significant semantic risk;
- security-sensitive work;
- concurrency-sensitive work;
- data-integrity-sensitive work;
- ambiguous behavior;
- tasks requiring meaningful coordination with later tasks.

Default to:

`main-only`

when uncertain.

## 11. Populate task state

Populate:

`state.json -> tasks`

in numeric order.

Each entry uses:

```json
{
  "id": "001",
  "path": "tasks/001-example.md",
  "status": "PENDING",
  "attempts": 0,
  "implementation": null,
  "evidence": null
}
```

## 12. Render status

Create:

`.ai/workflow/status.md`

including:

- work ID;
- title;
- phase;
- plan revision;
- approval status;
- current task;
- task checklist;
- retry counts;
- blocker if any.

## 13. Wait for approval

Set:

`phase = AWAITING_APPROVAL`

Update:

`updated_at`

Do not modify product code.

Tell the human to review:

- request.md;
- plan.md;
- gate.json;
- tasks/*.md.

End the turn.

---

# Mode: revise

Revision is valid only when:

`phase == AWAITING_APPROVAL`

Set:

`phase = PLANNING`

Increment:

`plan_revision`

Apply the human's requested changes.

Modify as necessary:

- plan.md;
- gate.json;
- tasks/*.md.

Re-investigate repository code where needed.

Do not modify product/source implementation.

If task structure changes:

rebuild:

`state.json -> tasks`

Preserve work-package identity and baseline_sha.

When revision is finished:

set:

`phase = AWAITING_APPROVAL`

Update status.

Stop.

---

# Mode: approve

Approval must be explicit:

`$feature-workflow approve`

It is valid only when:

`phase == AWAITING_APPROVAL`

Do not regenerate planning documents during approval.

Run:

`python3 .agents/skills/feature-workflow/scripts/plan_hash.py`

Store the returned value as:

`approved_plan_sha256`

Record:

`approved_at = <current UTC timestamp>`

Reset:

`stop_continuations = 0`

Set:

`phase = EXECUTING`

Update status.

Immediately enter the autonomous execution loop.

Do not stop merely to acknowledge approval.

Do not ask whether implementation should begin.

Approval means implementation begins now.

---

# Mode: resume

Read:

1. `.ai/workflow/state.json`
2. `.ai/workflow/request.md`
3. `.ai/workflow/plan.md`
4. `.ai/workflow/gate.json`
5. relevant task documents
6. existing evidence as needed.

If phase is:

- AWAITING_APPROVAL;
- DONE;
- BLOCKED;
- PLAN_CHANGE_REQUIRED;
- RETRY_BUDGET_EXCEEDED;

stop.

Otherwise continue from the exact recorded state.

Never redo a COMPLETE task.

If the current task is already in:

`TASK_REVIEW`

resume review.

If it is in:

`TASK_GATE`

resume its quality gate.

Do not restart implementation merely because the conversation was compacted
or resumed.

---

# Mode: status

Read:

`.ai/workflow/state.json`

and:

`.ai/workflow/status.md`

Report:

- work ID;
- title;
- phase;
- current task;
- task completion count;
- retries;
- blocker;
- whether package is awaiting archive.

Do not perform implementation work.

Stop.

---

# Approved-plan verification

Before:

- starting each task;
- final whole-package review;
- final acceptance;
- archive;

run:

`python3 .agents/skills/feature-workflow/scripts/plan_hash.py`

Compare the result to:

`state.approved_plan_sha256`

If different:

1. set `phase = PLAN_CHANGE_REQUIRED`;
2. set `block_reason`;
3. update status;
4. stop.

Never automatically approve a changed plan.

---

# Autonomous execution loop

Continue while any task is not COMPLETE.

## 1. Select current task

Select the first non-COMPLETE task in numeric order.

Never skip ahead.

If status is PENDING:

set:

- `current_task = task.id`
- `task.status = IN_PROGRESS`
- `task.attempts += 1`
- `phase = TASK_IMPLEMENTATION`

Update status.

## 2. Load context

Read:

- request.md;
- plan.md;
- current task;
- relevant previous evidence;
- relevant repository code.

Do not automatically read archived work packages.

Do not reread unrelated source files without reason.

## 3. Select implementer

Default:

PRIMARY THREAD implements the task.

The primary thread may use the custom `worker` only when:

- the task says `Delegation: worker-eligible`;
- the task is still bounded after earlier implementation changes;
- no new architectural decision is required;
- delegation is likely to save meaningful primary-model work.

Never delegate:

`main-only`

tasks.

Never have more than one worker active.

Never spawn a worker merely because one is available.

### Worker assignment

When delegating, give the worker:

"Implement ONLY the task in:

`.ai/workflow/<TASK_PATH>`

Read `.ai/workflow/request.md` and `.ai/workflow/plan.md` only as supporting
context.

Do not modify workflow or archive files.

Do not expand scope.

Do not redesign the approved plan.

Run the task-specific validation commands.

Do not begin another task.

Do not spawn another agent.

Return:

1. implementation summary;
2. files changed;
3. validation commands and outcomes;
4. unresolved concerns."

Wait for that worker.

Do not spawn another worker.

When it returns, primary-thread ownership resumes.

If worker delegation fails:

do not repeatedly respawn the worker.

The primary thread implements or repairs the task itself.

## 4. Main-thread review

Set:

`phase = TASK_REVIEW`

The PRIMARY THREAD reviews the actual implementation.

Do not rely on a worker's summary.

Review against:

- every task acceptance criterion;
- architectural decisions in plan.md;
- repository conventions;
- previous completed tasks;
- regression risk;
- error handling;
- test coverage;
- unnecessary complexity;
- accidental scope expansion.

Fix high-confidence defects directly.

Do not spawn a reviewer agent.

## 5. Task-specific validation

Run every command under the task's:

`## Validation`

If any fails:

diagnose and repair.

Repeat until successful or retry budget is exhausted.

## 6. Standard quality gate

Set:

`phase = TASK_GATE`

Run EVERY command in:

`.ai/workflow/gate.json -> standard_commands`

Run commands in listed order.

Do not omit commands because earlier tasks passed them.

If all pass:

continue.

If one fails:

1. record the failure;
2. increment the task attempt count;
3. if attempts <= `task_retry_limit`:
   - set phase = TASK_IMPLEMENTATION;
   - diagnose;
   - fix;
   - repeat review and validation;
4. otherwise:
   - set phase = RETRY_BUDGET_EXCEEDED;
   - record block_reason;
   - update status;
   - stop.

Do not weaken tests merely to make the gate pass unless doing so is explicitly
required by the approved plan.

## 7. Record task evidence

Create:

`.ai/workflow/evidence/<TASK_ID>.md`

using:

# Evidence — Task NNN

## Implementation

- Implemented by: main | worker
- Attempt count: N

## Files changed

- ...

## Acceptance criteria

- [x] Criterion
  - Evidence: ...

## Main-thread review

Summarize important findings and fixes.

## Task-specific validation

| Command | Result |
| --- | --- |
| `<command>` | PASS |

## Standard quality gate

| Command | Result |
| --- | --- |
| `<command>` | PASS |

## Remaining concerns

`None`

or concise non-blocking concerns.

Do not store enormous raw build logs.

Store useful evidence.

## 8. Complete the task

Set:

- `task.status = COMPLETE`
- `task.implementation = main` or `worker`
- `task.evidence = evidence/<TASK_ID>.md`
- `current_task = null`
- `phase = EXECUTING`

Update status.

Immediately continue to the next task.

Do not ask for permission.

Do not end the turn merely to report task completion.

---

# Final whole-work-package review

When every task is COMPLETE:

set:

`phase = FINAL_REVIEW`

Set:

`final_attempts = 1`

Verify the approved-plan hash.

Read:

- request.md;
- plan.md;
- every task;
- all task evidence;
- cumulative source changes.

Run:

`git diff <baseline_sha>`

The PRIMARY THREAD performs the review.

Do not delegate final review by default.

Review the completed work package as one integrated change.

Specifically inspect for:

1. original requested work not completed;
2. package-level acceptance criteria not satisfied;
3. inconsistencies across task boundaries;
4. integration bugs;
5. regressions;
6. incorrect assumptions;
7. dead or obsolete implementation;
8. incomplete cleanup;
9. inadequate error handling;
10. security defects;
11. concurrency defects where applicable;
12. data-integrity defects where applicable;
13. missing regression coverage;
14. accidental scope expansion;
15. unnecessary complexity.

Classify findings:

HIGH CONFIDENCE

or:

LOW CONFIDENCE / OPTIONAL

Fix all HIGH CONFIDENCE findings that remain within the approved plan.

Do not churn implementation for speculative low-confidence findings.

If fixing a required finding would materially change the approved plan:

set:

`phase = PLAN_CHANGE_REQUIRED`

record the reason and stop.

---

# Final quality gate

Set:

`phase = FINAL_GATE`

Run EVERY command in:

`gate.json -> final_commands`

If final_commands is empty or absent:

run every standard_commands entry.

If all pass:

continue.

If a command fails:

1. diagnose;
2. repair;
3. increment `final_attempts`;
4. repeat relevant final review;
5. rerun the entire final quality gate.

If:

`final_attempts > final_retry_limit`

set:

`phase = RETRY_BUDGET_EXCEEDED`

record the reason.

Stop.

---

# Final evidence

Create:

`.ai/workflow/evidence/final.md`

using:

# Final Work-Package Evidence

## Work package

- ID: <work_id>
- Title: <title>

## Original objective

Summarize the requested work.

## Completed tasks

List every task and evidence file.

## Whole-package review

Summarize the final integrated review.

## High-confidence findings fixed

List findings and repairs.

Use:

`None`

when appropriate.

## Final acceptance criteria

Map every package-level acceptance criterion from plan.md to evidence.

## Final quality gate

| Command | Result |
| --- | --- |
| `<command>` | PASS |

## Cumulative diff

Summarize changes from:

`baseline_sha`

to the current repository.

## Remaining non-blocking concerns

List concerns or:

`None`

---

# DONE transition

Verify the approved-plan hash again.

Verify:

- every task is COMPLETE;
- every task has evidence;
- final.md exists;
- every final quality command passed.

Run:

`git rev-parse HEAD`

Store as:

`final_sha`

Set:

`completed_at = <current UTC timestamp>`

Set:

- `phase = DONE`
- `current_task = null`
- `block_reason = null`

Update status.

Return a concise final report containing:

- work-package title;
- work-package ID;
- tasks completed;
- major implementation decisions;
- final quality-gate results;
- remaining non-blocking concerns;
- reminder that the package remains active until the human explicitly runs
  `$feature-workflow archive`.

Stop.

Do NOT archive automatically.

---

# Mode: archive

Archive is an explicit human-requested housekeeping operation.

It is valid only when:

`phase == DONE`

Never infer archive intent.

Never automatically archive after completion.

## 1. Validate completion

Read:

- `.ai/workflow/state.json`
- `.ai/workflow/evidence/final.md`

Require:

- phase == DONE;
- every task status == COMPLETE;
- every task has an evidence file;
- final.md exists;
- approved_plan_sha256 exists;
- baseline_sha exists;
- final_sha exists;
- completed_at exists.

If any requirement is false:

refuse to archive.

Explain the inconsistency.

Do not partially archive.

## 2. Verify frozen plan

Run:

`python3 .agents/skills/feature-workflow/scripts/plan_hash.py`

Require that the returned hash equals:

`state.approved_plan_sha256`

If not:

set:

`phase = PLAN_CHANGE_REQUIRED`

record the reason.

Update status.

Stop.

## 3. Determine final archive ID

Read:

`state.work_id`

Candidate destination:

`.ai/workflow-archive/<work_id>/`

If it does not exist:

use it.

If it already exists:

append:

`-02`

then:

`-03`

etc.

Update state.work_id if a suffix was required.

Never overwrite an existing archive.

## 4. Record archive time

Set:

`archived_at = <current UTC timestamp>`

Update state.json.

## 5. Create archive.json

Create:

`.ai/workflow/archive.json`

with:

```json
{
  "schema_version": 1,
  "work_id": "<work-id>",
  "title": "<title>",
  "created_at": "<timestamp>",
  "approved_at": "<timestamp>",
  "completed_at": "<timestamp>",
  "archived_at": "<timestamp>",
  "baseline_sha": "<sha>",
  "final_sha": "<sha>",
  "approved_plan_sha256": "<hash>",
  "task_count": 0,
  "tasks_completed": 0,
  "result": "DONE",
  "final_evidence": "evidence/final.md"
}
```

Populate the actual task counts.

## 6. Maintain archive index

Ensure:

`.ai/workflow-archive/index.md`

exists.

If it does not, initialize:

# Work Package Archive

Append one entry in this form:

`- <work_id> — <title> — completed <completed_at> — <baseline_sha>..<final_sha>`

Never delete or rewrite previous entries except to repair an objectively
broken index.

## 7. Move the work package

Move the entire:

`.ai/workflow/`

directory to:

`.ai/workflow-archive/<work_id>/`

The archived directory must contain:

- archive.json;
- request.md;
- plan.md;
- gate.json;
- state.json;
- status.md;
- tasks/;
- evidence/.

Do not copy and leave the original.

Move it.

## 8. Verify cleanup

After the move, verify:

`.ai/workflow/state.json`

does not exist.

Verify:

`.ai/workflow/`

does not exist.

Do not create an empty replacement directory.

The absence of `.ai/workflow/state.json` is the authoritative no-active-work
state.

## 9. Finish archival

Report:

- archived work ID;
- archive path;
- title;
- task count;
- baseline SHA;
- final SHA.

Report that the active workflow slot is now empty and a new work package may
begin.

Stop.

---

# Mode: history

History mode is read-only.

Read:

`.ai/workflow-archive/index.md`

If it does not exist:

report that no work packages have been archived.

Otherwise report the archive entries.

Do not:

- activate an archive;
- modify an archive;
- load archived plans into current workflow context;
- create a workflow.

Stop.

---

# Archived-work rules

Archived work packages are immutable historical records.

During planning and execution:

do not automatically read:

`.ai/workflow-archive/*/plan.md`

or:

`.ai/workflow-archive/*/tasks/`

Previous implementation is already represented by the current repository.

Use archived material only when:

- the human explicitly refers to an earlier package;
- current work explicitly depends on prior planning intent;
- historical provenance is required to resolve ambiguity.

Archives are historical evidence, not current instructions.

---

# Follow-up findings

A completed work package may contain observations such as:

- possible future cleanup;
- optional refactors;
- low-confidence review findings;
- unrelated bugs noticed during implementation.

Do not automatically create new tasks from those observations.

Do not carry them into the next work package.

The human decides what enters future work.

---

# Terminal states

After approval, normal autonomous execution may stop only at:

`DONE`

`BLOCKED`

`PLAN_CHANGE_REQUIRED`

`RETRY_BUDGET_EXCEEDED`

or a normal Codex human authorization/security boundary.

Do not invent additional terminal states.

---

# Status rendering

Whenever state materially changes, update:

`.ai/workflow/status.md`

Use:

# Work-Package Status

Work ID: `<id>`

Title: `<title>`

Phase: `<phase>`

Plan revision: `<revision>`

Approved: yes | no

Current task: `<id>` | none

## Tasks

- [x] 001 — <name> — main — 1 attempt
- [x] 002 — <name> — worker — 1 attempt
- [ ] 003 — <name> — IN PROGRESS — attempt 2
- [ ] 004 — <name> — PENDING

## Package quality gate

Pending | Passed | Failed

## Current blocker

None

or the current blocker.

If phase == DONE also include:

Completed at: `<timestamp>`

Final SHA: `<sha>`

Archive status: awaiting human archive
