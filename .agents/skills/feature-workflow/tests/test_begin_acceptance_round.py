import json
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))

from begin_acceptance_round import begin_acceptance_round  # noqa: E402
from plan_hash import calculate_plan_hash  # noqa: E402


class AcceptanceRoundTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.workflow = Path(self.temporary.name) / "workflow"
        (self.workflow / "tasks").mkdir(parents=True)
        (self.workflow / "evidence").mkdir()
        (self.workflow / "request.md").write_text("# Request\n")
        (self.workflow / "plan.md").write_text("# Plan\n")
        (self.workflow / "gate.json").write_text('{"standard_commands": []}\n')
        self._write_task("001", "Initial work", round_number=None)
        self._write_done_state(schema_version=1)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _write_task(
        self, task_id: str, title: str, round_number: int | None
    ) -> dict:
        task_path = f"tasks/{task_id}-{title.lower().replace(' ', '-')}.md"
        evidence_path = f"evidence/{task_id}.md"
        (self.workflow / task_path).write_text(f"# Task {task_id}: {title}\n")
        (self.workflow / evidence_path).write_text(f"# Evidence — Task {task_id}\n")
        task = {
            "id": task_id,
            "path": task_path,
            "status": "COMPLETE",
            "attempts": 1,
            "implementation": "main",
            "evidence": evidence_path,
        }
        if round_number is not None:
            task["round"] = round_number
        return task

    def _write_done_state(self, schema_version: int) -> None:
        task = {
            "id": "001",
            "path": "tasks/001-initial-work.md",
            "status": "COMPLETE",
            "attempts": 1,
            "implementation": "main",
            "evidence": "evidence/001.md",
        }
        state = {
            "schema_version": schema_version,
            "work_id": "2026-08-25-example",
            "title": "Example",
            "phase": "DONE",
            "created_at": "2026-08-25T00:00:00Z",
            "updated_at": "2026-08-25T01:00:00Z",
            "approved_at": "2026-08-25T00:10:00Z",
            "completed_at": "2026-08-25T01:00:00Z",
            "archived_at": None,
            "baseline_sha": "a" * 40,
            "final_sha": "b" * 40,
            "plan_revision": 1,
            "approved_plan_sha256": calculate_plan_hash(self.workflow),
            "current_task": None,
            "tasks": [task],
            "task_retry_limit": 3,
            "final_retry_limit": 2,
            "final_attempts": 1,
            "stop_continuations": 0,
            "max_stop_continuations": 50,
            "block_reason": None,
        }
        (self.workflow / "state.json").write_text(json.dumps(state, indent=2) + "\n")
        (self.workflow / "status.md").write_text("old status\n")
        (self.workflow / "evidence" / "final.md").write_text("# Final\n")

    def _state(self) -> dict:
        return json.loads((self.workflow / "state.json").read_text())

    def test_schema_one_done_package_reopens_without_losing_history(self) -> None:
        result = begin_acceptance_round(
            self.workflow, "2026-08-25T02:00:00Z"
        )
        state = self._state()

        self.assertEqual(
            result,
            {
                "acceptance_round": 2,
                "next_task_id": "002",
                "preserved_final_evidence": "evidence/final-round-001.md",
            },
        )
        self.assertEqual(state["schema_version"], 2)
        self.assertEqual(state["phase"], "PLANNING")
        self.assertEqual(state["plan_revision"], 2)
        self.assertEqual(state["acceptance_round"], 2)
        self.assertEqual(state["tasks"][0]["round"], 1)
        self.assertEqual(state["completed_rounds"][0]["task_ids"], ["001"])
        self.assertIsNone(state["approved_plan_sha256"])
        self.assertIsNone(state["approved_at"])
        self.assertIsNone(state["completed_at"])
        self.assertIsNone(state["final_sha"])
        self.assertTrue(
            (self.workflow / "evidence" / "final-round-001.md").is_file()
        )
        self.assertFalse((self.workflow / "evidence" / "final.md").exists())
        status = (self.workflow / "status.md").read_text()
        self.assertIn("Acceptance round: `2`", status)
        self.assertIn("Prior completed rounds: `1`", status)

    def test_completed_follow_up_can_open_another_round(self) -> None:
        begin_acceptance_round(self.workflow, "2026-08-25T02:00:00Z")
        state = self._state()
        second_task = self._write_task("002", "Acceptance fix", round_number=2)
        state["tasks"].append(second_task)
        state.update(
            {
                "phase": "DONE",
                "approved_at": "2026-08-25T02:10:00Z",
                "completed_at": "2026-08-25T03:00:00Z",
                "final_sha": "c" * 40,
                "approved_plan_sha256": calculate_plan_hash(self.workflow),
            }
        )
        (self.workflow / "state.json").write_text(json.dumps(state, indent=2) + "\n")
        (self.workflow / "evidence" / "final.md").write_text("# Round 2 Final\n")

        result = begin_acceptance_round(
            self.workflow, "2026-08-25T04:00:00Z"
        )
        reopened = self._state()

        self.assertEqual(result["acceptance_round"], 3)
        self.assertEqual(result["next_task_id"], "003")
        self.assertEqual(len(reopened["completed_rounds"]), 2)
        self.assertEqual(reopened["completed_rounds"][1]["task_ids"], ["002"])
        self.assertTrue(
            (self.workflow / "evidence" / "final-round-001.md").is_file()
        )
        self.assertTrue(
            (self.workflow / "evidence" / "final-round-002.md").is_file()
        )

    def test_plan_drift_refuses_without_mutating_done_package(self) -> None:
        (self.workflow / "plan.md").write_text("# Changed plan\n")
        original_state = (self.workflow / "state.json").read_text()
        original_status = (self.workflow / "status.md").read_text()

        with self.assertRaisesRegex(ValueError, "approved planning artifacts changed"):
            begin_acceptance_round(self.workflow, "2026-08-25T02:00:00Z")

        self.assertEqual((self.workflow / "state.json").read_text(), original_state)
        self.assertEqual((self.workflow / "status.md").read_text(), original_status)
        self.assertTrue((self.workflow / "evidence" / "final.md").is_file())
        self.assertFalse(
            (self.workflow / "evidence" / "final-round-001.md").exists()
        )


if __name__ == "__main__":
    unittest.main()
