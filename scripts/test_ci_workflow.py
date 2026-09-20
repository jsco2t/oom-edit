import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
MAKEFILE = ROOT / "Makefile"
CONTRIBUTING = ROOT / "CONTRIBUTING.md"
DENY_CONFIG = ROOT / "deny.toml"
DENY_SCRIPT = ROOT / "scripts" / "cargo-deny.sh"


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

    def test_deny_keeps_yanked_and_warning_failures_enabled(self):
        deny_config = DENY_CONFIG.read_text(encoding="utf-8")
        self.assertIn('yanked = "deny"', deny_config)
        self.assertNotIn("disable-yank-checking", deny_config)
        self.assertIn("DENY_FLAGS := check -D warnings", self.makefile)

    def test_deny_runs_outside_repository_with_explicit_manifest(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            temporary_root = Path(temporary_directory)
            fake_bin = temporary_root / "bin"
            fake_bin.mkdir()
            scratch = temporary_root / "scratch"
            scratch.mkdir()
            captured_calls = temporary_root / "calls"

            fake_cargo = fake_bin / "cargo"
            fake_cargo.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                '{ pwd; printf \'%s\\n\' "$@"; printf \'__CALL__\\n\'; } '
                '>> "${CAPTURED_CALLS}"\n',
                encoding="utf-8",
            )
            fake_cargo.chmod(0o755)

            environment = os.environ.copy()
            environment["PATH"] = f"{fake_bin}{os.pathsep}{environment['PATH']}"
            environment["TMPDIR"] = str(scratch)
            environment["CAPTURED_CALLS"] = str(captured_calls)

            subprocess.run(
                ["bash", str(DENY_SCRIPT), "__DENY_FLAGS_SENTINEL__"],
                cwd=ROOT,
                env=environment,
                check=True,
            )

            calls = [
                record.splitlines()
                for record in captured_calls.read_text(encoding="utf-8").split(
                    "__CALL__\n"
                )
                if record
            ]
            self.assertEqual(len(calls), 2)
            fetch_working_directory = Path(calls[0][0])
            self.assertEqual(
                calls[0][1:],
                [
                    "fetch",
                    "--locked",
                    "--manifest-path",
                    str(ROOT / "Cargo.toml"),
                ],
            )
            self.assertNotEqual(fetch_working_directory, ROOT)
            self.assertFalse(fetch_working_directory.exists())
            self.assertEqual(calls[1], [str(ROOT), "deny", "__DENY_FLAGS_SENTINEL__"])


if __name__ == "__main__":
    unittest.main()
