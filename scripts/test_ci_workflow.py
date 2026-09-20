import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
MAKEFILE = ROOT / "Makefile"
CONTRIBUTING = ROOT / "CONTRIBUTING.md"


class CiWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.workflow = WORKFLOW.read_text(encoding="utf-8")
        cls.makefile = MAKEFILE.read_text(encoding="utf-8")

    def test_triggers_every_pull_request_and_pushes_only_to_main(self):
        self.assertIn("on:\n  pull_request:\n  push:\n", self.workflow)
        self.assertRegex(
            self.workflow,
            r"(?m)^  push:\n    branches:\n      - main$",
        )

    def test_job_is_bounded_and_least_privilege(self):
        self.assertIn("permissions:\n  contents: read\n", self.workflow)
        self.assertRegex(self.workflow, r"(?m)^    timeout-minutes: [1-9][0-9]*$")
        self.assertNotIn("services:", self.workflow)
        self.assertNotRegex(
            self.workflow.lower(),
            r"\b(docker|emulator|simulator)\b",
        )

    def test_actions_and_audit_tools_are_immutably_pinned(self):
        action_lines = re.findall(
            r"(?m)^\s+-?\s*uses: ([^\s#]+)(?:\s+#\s+([^\s]+))?$",
            self.workflow,
        )
        self.assertEqual(len(action_lines), 2)
        for action, release in action_lines:
            self.assertRegex(action, r"^[^@\s]+@[0-9a-f]{40}$")
            self.assertRegex(release, r"^v[0-9]+\.[0-9]+\.[0-9]+$")

        self.assertIn(
            "tool: cargo-audit@0.22.1,cargo-deny@0.19.6",
            self.workflow,
        )
        self.assertIn("fallback: none", self.workflow)

    def test_workflow_uses_only_the_make_owned_ci_entry_point(self):
        run_commands = re.findall(r"(?m)^\s+run:\s+(.+)$", self.workflow)
        self.assertEqual(run_commands, ["make ci"])

    def test_make_ci_runs_release_build_before_check(self):
        target = re.search(
            r"(?m)^ci:.*\n(?P<recipes>(?:\t.*\n)+)",
            self.makefile,
        )
        self.assertIsNotNone(target)
        recipes = target.group("recipes").splitlines()
        self.assertEqual(recipes, ["\t$(MAKE) build-release", "\t$(MAKE) check"])

    def test_contract_test_is_in_both_test_paths(self):
        self.assertRegex(
            self.makefile,
            r"(?m)^test: .*\bci-workflow-test\b",
        )
        command = (
            "PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover "
            "-s scripts -p 'test_ci_workflow.py'"
        )
        self.assertGreaterEqual(self.makefile.count(command), 2)

    def test_ci_target_is_documented(self):
        documentation = CONTRIBUTING.read_text(encoding="utf-8")
        self.assertIn("| `make ci` |", documentation)


if __name__ == "__main__":
    unittest.main()
