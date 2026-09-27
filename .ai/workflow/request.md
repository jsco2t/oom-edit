# Request

Implement the complete Delivery Requirements Document at
`/home/duser/Developer/sources/personal/notebook/projects/oom/v1/oom-edit-drd.md`.

User's complete work description:

> We have a DRD document here: /home/duser/Developer/sources/personal/notebook/projects/oom/v1/oom-edit-drd.md
>
> I want to implement all of it please. Please make sure we are test forward and have a clear emphasis on not regressing functionality **OR** performance. A regression in either is not acceptable.
>
> When this change is done, if I am using `oom-edit` I should see no differnce in the fact that the change has been implemented.

The DRD's linked implementation contracts and automated test plan are part of the referenced delivery specification. The final visible-behavior constraint applies to the existing standalone experience, except for the DRD's explicitly identified intentional safety and disk-watching changes.

Additional user requirement received during planning:

> This may not be in the DRD but please also make sure that the documentation (including AGENTS.md) is updated. Also include a DEVELOPER.md doc (or update it) to document how to integrate `oom-edit` into another rust project.

## Task 007 validation decision — 2026-09-25

Task 007's `make bench` invocation exposed a first-frame absolute-limit failure already present in the unchanged v0.5.0 baseline on this machine. Asked whether to optimize that core benchmark during Task 007 or resolve it in Task 014, the user answered: "Resolve in Task 014". The existing limit and final acceptance requirement remain unchanged.

## First-frame sequencing decision — 2026-09-27

After Tasks 008–009 completed, Task 010's required full `make bench` conflicted with the Task-007-only exception above. Asked whether to extend the known-failure deferral through Task 010 or resolve the failure before completing Task 010, the user explicitly answered: "resolve it now please". This authorizes moving the first-frame optimization into Task 010's opening work, with regression tests and the unchanged full release and standard gates before continuing the standalone migration. Task 014 still re-verifies all integrated performance requirements; no threshold, gate or other scope is waived.

## Same-host comparison input decision — 2026-09-27

Task 014's exact comparison rejected the original Task 001 measurement because
it records 6 logical CPUs while this host now reports 8. Asked to use the preserved
Task 010 same-host baseline, keeping original evidence and all performance limits
unchanged, the user explicitly answered: "yes - approved". Only the incomplete
Tasks 014/015 comparison baseline paths change to
`evidence/010-baseline-current-host.tsv`. That artifact measures the same unchanged
baseline commit. All completed task documents/evidence, comparator logic,
thresholds, requirements and separate publication authority remain unchanged.

## Shift+V parity amendment — 2026-09-27

Native Linux README smoke exposed a pre-existing line-Select routing defect:
terminals report uppercase V with Shift, which the rendered Normal/Select
handlers reject. Asked for the narrow strict-parity amendment, the user explicitly
approved: "you are approved to fix the Shift+V bug you found". Task 015 may fix
that routing and add terminal-realistic regression tests, then commit and rerun
the exact candidate gates. This is the only additional intentional standalone
behavior change. Completed tasks/evidence, all performance limits, native macOS
verification, and separate publication authorization remain unchanged.
