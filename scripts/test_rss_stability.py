"""Contract tests for the exact-fixture current-RSS stability gate."""

import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import rss_stability as stability


class RssStabilityTests(unittest.TestCase):
    def test_runner_contract_runs_in_both_repository_test_gates(self):
        makefile = (Path(__file__).resolve().parents[1] / "Makefile").read_text()
        test_target = makefile.split(".PHONY: test\n", 1)[1].split("\n", 1)[0]
        check_target = makefile.split(".PHONY: check\n", 1)[1].split(
            "\n.PHONY:", 1
        )[0]
        self.assertIn("rss-stability-test", test_target)
        self.assertIn("test_rss_stability.py", check_target)

    def sample(self, values=(200, 201, 202, 201), first_edit=20_000_000):
        fixture_hash = "0123456789abcdef"
        lines = [f"RSS-RUN\t{fixture_hash}\trust-delete\t400\t3000\t4100"]
        lines.append(f"FIRST-EDIT\t{fixture_hash}\trust-delete\t{first_edit}")
        for cycle, value in zip(stability.CHECKPOINTS, values):
            lines.append(
                f"RSS\t{fixture_hash}\trust-delete\t{cycle}\t{value * 1024 * 1024}"
                f"\t{fixture_hash}\tnormal\t4100"
            )
        return "\n".join(lines)

    def test_exact_identity_and_bounded_current_rss(self):
        parsed = stability.parse_sample(
            self.sample(), "0123456789abcdef", "rust-delete", 400, 3000
        )
        self.assertEqual(len(parsed["checkpoints"]), 4)
        self.assertEqual(parsed["first_edit_ns"], 20_000_000)
        self.assertEqual(stability.failures(parsed), [])

    def test_growth_and_first_edit_are_independent_failures(self):
        parsed = stability.parse_sample(
            self.sample((200, 202, 203, 203), 50_000_000),
            "0123456789abcdef", "rust-delete", 400, 3000,
        )
        self.assertEqual(len(stability.failures(parsed)), 2)

    def test_rejects_missing_or_wrong_checkpoint_and_frame(self):
        output = self.sample()
        for changed in (
            output.replace("\t200\t", "\t201\t", 1),
            output.replace("\tnormal\t4100", "\tinsert\t4100", 1),
            output.replace("\tnormal\t4100", "\tnormal\t4099", 1),
            output.replace("0123456789abcdef", "fedcba9876543210", 1),
            "\n".join(output.splitlines()[:-1]),
        ):
            with self.subTest(changed=changed):
                with self.assertRaises(ValueError):
                    stability.parse_sample(
                        changed, "0123456789abcdef", "rust-delete", 400, 3000
                    )

    def test_mode_and_reload_have_no_edit_timing_row(self):
        for scenario in ("go-mode", "go-reload"):
            with self.subTest(scenario=scenario):
                output = self.sample().replace("rust-delete", scenario).replace(
                    "\t3000\t", "\t20000\t", 1
                )
                without_edit = "\n".join(
                    line for line in output.splitlines()
                    if not line.startswith("FIRST-EDIT")
                )
                parsed = stability.parse_sample(
                    without_edit, "0123456789abcdef", scenario, 400, 20000
                )
                self.assertIsNone(parsed["first_edit_ns"])
                with self.assertRaises(ValueError):
                    stability.parse_sample(
                        output, "0123456789abcdef", scenario, 400, 20000
                    )

    def test_process_failure_cannot_be_counted_as_a_sample(self):
        failed = SimpleNamespace(returncode=1, stdout="RSS-RUN\tpartial", stderr="failed")
        with patch.object(stability.subprocess, "run", return_value=failed):
            with self.assertRaisesRegex(RuntimeError, "probe failed"):
                stability.run_probe(Path("probe"), Path("fixture"), "rust-reload", 3000, 400)

    def test_scenario_inventory_covers_both_languages_and_all_operations(self):
        self.assertEqual(len(stability.SCENARIOS), 8)
        self.assertEqual(
            {name for name, _ in stability.SCENARIOS},
            {f"{language}-{operation}" for language in ("rust", "go")
             for operation in ("delete", "change", "mode", "reload")},
        )


if __name__ == "__main__":
    unittest.main()
