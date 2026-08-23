#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


STOP_PHASES = {
    "AWAITING_APPROVAL",
    "DONE",
    "BLOCKED",
    "PLAN_CHANGE_REQUIRED",
    "RETRY_BUDGET_EXCEEDED",
}


def now() -> str:
    return (
        datetime.now(timezone.utc)
        .replace(microsecond=0)
        .isoformat()
    )


def repository_root(cwd: str) -> Path:
    return Path(
        subprocess.check_output(
            ["git", "-C", cwd, "rev-parse", "--show-toplevel"],
            text=True,
        ).strip()
    )


def calculate_plan_hash(workflow: Path) -> str:
    files = [
        workflow / "request.md",
        workflow / "plan.md",
        workflow / "gate.json",
        *sorted((workflow / "tasks").glob("*.md")),
    ]

    digest = hashlib.sha256()

    for path in files:
        if not path.exists():
            return ""

        digest.update(
            path.relative_to(workflow).as_posix().encode()
        )
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")

    return digest.hexdigest()


def save_state(path: Path, state: dict) -> None:
    state["updated_at"] = now()

    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(
        json.dumps(state, indent=2) + "\n",
        encoding="utf-8",
    )
    temporary.replace(path)


payload = json.load(sys.stdin)

if payload.get("hook_event_name") != "Stop":
    raise SystemExit(0)

try:
    root = repository_root(payload.get("cwd") or ".")
except Exception:
    raise SystemExit(0)

workflow = root / ".ai" / "workflow"
state_path = workflow / "state.json"

if not state_path.exists():
    raise SystemExit(0)

try:
    state = json.loads(
        state_path.read_text(encoding="utf-8")
    )
except Exception as exc:
    print(
        json.dumps(
            {
                "continue": False,
                "stopReason": (
                    f"Workflow state is unreadable: {exc}"
                ),
            }
        )
    )
    raise SystemExit(0)


phase = state.get("phase")

# These phases intentionally allow the Codex turn to end.
if phase in STOP_PHASES:
    raise SystemExit(0)


# Once approved, detect any modification of the frozen plan.
expected_hash = state.get("approved_plan_sha256")

if expected_hash:
    actual_hash = calculate_plan_hash(workflow)

    if actual_hash != expected_hash:
        state["phase"] = "PLAN_CHANGE_REQUIRED"
        state["block_reason"] = (
            "Approved request, plan, quality gate, or task "
            "documents changed after approval."
        )
        save_state(state_path, state)

        print(
            json.dumps(
                {
                    "continue": False,
                    "stopReason": (
                        "Approved plan changed. "
                        "Human re-approval is required."
                    ),
                }
            )
        )
        raise SystemExit(0)


# Bound automatic continuation so a confused agent cannot loop forever.
continuations = (
    int(state.get("stop_continuations", 0)) + 1
)
limit = int(
    state.get("max_stop_continuations", 50)
)

state["stop_continuations"] = continuations

if continuations > limit:
    state["phase"] = "BLOCKED"
    state["block_reason"] = (
        "Autonomous continuation budget exceeded "
        f"({limit}). Human review is required."
    )

    save_state(state_path, state)

    print(
        json.dumps(
            {
                "continue": False,
                "stopReason": state["block_reason"],
            }
        )
    )
    raise SystemExit(0)


save_state(state_path, state)

# Explicitly re-invoke the workflow Skill. This is useful after
# long runs and context compaction because the authoritative workflow
# instructions and state get loaded again.
print(
    json.dumps(
        {
            "decision": "block",
            "reason": (
                "$feature-workflow resume\n\n"
                "The approved workflow is still active. "
                "Read .ai/workflow/state.json and continue "
                "from the recorded phase. Do not repeat "
                "completed tasks. Do not ask for permission "
                "between tasks. Stop only at "
                "AWAITING_APPROVAL, DONE, BLOCKED, "
                "PLAN_CHANGE_REQUIRED, or "
                "RETRY_BUDGET_EXCEEDED."
            ),
        }
    )
)
