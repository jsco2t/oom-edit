#!/usr/bin/env python3

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import tempfile

from plan_hash import calculate_plan_hash, repository_workflow_root


def fail(message: str) -> None:
    raise ValueError(message)


def task_title(workflow: Path, task: dict) -> str:
    path = workflow / task["path"]
    try:
        heading = path.read_text().splitlines()[0]
    except (FileNotFoundError, IndexError):
        return task["path"]
    prefix = f"# Task {task['id']}: "
    return heading.removeprefix(prefix)


def render_status(workflow: Path, state: dict) -> str:
    lines = [
        "# Work-Package Status",
        "",
        f"Work ID: `{state['work_id']}`",
        "",
        f"Title: `{state['title']}`",
        "",
        f"Phase: `{state['phase']}`",
        "",
        f"Plan revision: `{state['plan_revision']}`",
        "",
        f"Acceptance round: `{state['acceptance_round']}`",
        "",
        f"Prior completed rounds: `{len(state['completed_rounds'])}`",
        "",
        "Approved: no",
        "",
        "Current task: none",
        "",
        "## Tasks",
        "",
    ]
    for task in state["tasks"]:
        checked = "x" if task["status"] == "COMPLETE" else " "
        details = task["status"]
        if task["status"] == "COMPLETE":
            details = task.get("implementation") or "complete"
        attempts = task.get("attempts", 0)
        attempt_label = "attempt" if attempts == 1 else "attempts"
        lines.append(
            f"- [{checked}] {task['id']} — {task_title(workflow, task)} — "
            f"round {task.get('round', 1)} — {details} — "
            f"{attempts} {attempt_label}"
        )
    lines.extend(
        [
            "",
            "## Package quality gate",
            "",
            "Pending acceptance-follow-up planning and re-approval",
            "",
            "## Current blocker",
            "",
            "None",
            "",
        ]
    )
    return "\n".join(lines)


def write_atomic(path: Path, contents: str) -> None:
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=path.parent, delete=False
    ) as handle:
        handle.write(contents)
        handle.flush()
        os.fsync(handle.fileno())
        temporary = Path(handle.name)
    try:
        os.replace(temporary, path)
    except Exception:
        temporary.unlink(missing_ok=True)
        raise


def begin_acceptance_round(workflow: Path, timestamp: str) -> dict:
    state_path = workflow / "state.json"
    try:
        state = json.loads(state_path.read_text())
    except FileNotFoundError as error:
        fail(f"no active workflow state at {state_path}")
    except json.JSONDecodeError as error:
        fail(f"invalid workflow state: {error}")

    if state.get("phase") != "DONE":
        fail("acceptance follow-up requires phase DONE")

    expected_hash = state.get("approved_plan_sha256")
    if not expected_hash:
        fail("DONE package has no approved plan hash")
    try:
        actual_hash = calculate_plan_hash(workflow)
    except FileNotFoundError as error:
        fail(str(error))
    if actual_hash != expected_hash:
        fail(
            "approved planning artifacts changed: "
            f"expected {expected_hash}, got {actual_hash}"
        )

    required = ["approved_at", "completed_at", "baseline_sha", "final_sha"]
    missing = [field for field in required if not state.get(field)]
    if missing:
        fail(f"DONE package is missing required state: {', '.join(missing)}")

    tasks = state.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        fail("DONE package has no task state")
    for task in tasks:
        if task.get("status") != "COMPLETE":
            fail(f"task {task.get('id', '<unknown>')} is not COMPLETE")
        task_path = task.get("path")
        if not task_path or not (workflow / task_path).is_file():
            fail(f"task {task.get('id', '<unknown>')} has no task document")
        evidence = task.get("evidence")
        if not evidence or not (workflow / evidence).is_file():
            fail(f"task {task.get('id', '<unknown>')} has no evidence file")

    completed_round = int(state.get("acceptance_round", 1))
    completed_rounds = state.get("completed_rounds", [])
    if not isinstance(completed_rounds, list):
        fail("completed_rounds must be a list")
    for entry in completed_rounds:
        evidence = entry.get("final_evidence")
        if not evidence or not (workflow / evidence).is_file():
            fail(f"completed round {entry.get('round')} has no final evidence")
    if any(int(entry.get("round", 0)) == completed_round for entry in completed_rounds):
        fail(f"acceptance round {completed_round} is already preserved")

    final_source = workflow / "evidence" / "final.md"
    final_relative = f"evidence/final-round-{completed_round:03d}.md"
    final_target = workflow / final_relative
    if not final_source.is_file():
        fail("DONE package has no evidence/final.md")
    if final_target.exists():
        fail(f"refusing to overwrite {final_target}")

    for task in tasks:
        task.setdefault("round", 1)
    task_ids = [
        task["id"] for task in tasks if int(task["round"]) == completed_round
    ]
    if not task_ids:
        fail(f"acceptance round {completed_round} has no tasks")

    completed_rounds.append(
        {
            "round": completed_round,
            "plan_revision": state["plan_revision"],
            "approved_at": state["approved_at"],
            "completed_at": state["completed_at"],
            "approved_plan_sha256": expected_hash,
            "final_sha": state["final_sha"],
            "task_ids": task_ids,
            "final_evidence": final_relative,
        }
    )

    state.update(
        {
            "schema_version": 2,
            "phase": "PLANNING",
            "updated_at": timestamp,
            "approved_at": None,
            "completed_at": None,
            "final_sha": None,
            "plan_revision": int(state["plan_revision"]) + 1,
            "acceptance_round": completed_round + 1,
            "completed_rounds": completed_rounds,
            "approved_plan_sha256": None,
            "current_task": None,
            "final_attempts": 0,
            "stop_continuations": 0,
            "block_reason": None,
        }
    )

    next_task = max(int(task["id"]) for task in tasks) + 1
    result = {
        "acceptance_round": state["acceptance_round"],
        "next_task_id": f"{next_task:03d}",
        "preserved_final_evidence": final_relative,
    }

    status_path = workflow / "status.md"
    old_status = status_path.read_text() if status_path.exists() else None
    final_source.rename(final_target)
    try:
        write_atomic(status_path, render_status(workflow, state))
        write_atomic(state_path, json.dumps(state, indent=2) + "\n")
    except Exception:
        if final_target.exists() and not final_source.exists():
            final_target.rename(final_source)
        if old_status is None:
            status_path.unlink(missing_ok=True)
        else:
            write_atomic(status_path, old_status)
        raise
    return result


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Preserve a DONE workflow round and reopen acceptance planning."
    )
    parser.add_argument(
        "--workflow-root",
        type=Path,
        default=None,
        help="workflow directory (defaults to <git-root>/.ai/workflow)",
    )
    args = parser.parse_args()
    workflow = args.workflow_root or repository_workflow_root()
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    try:
        result = begin_acceptance_round(workflow.resolve(), timestamp)
    except (OSError, TypeError, ValueError) as error:
        raise SystemExit(f"cannot begin acceptance round: {error}") from error
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
