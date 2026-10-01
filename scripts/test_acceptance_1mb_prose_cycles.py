"""Contract tests for the exact-note prose selection latency gate."""

import subprocess
import unittest
from pathlib import Path

from acceptance_1mb_edit_cycles import parse_cycles
from acceptance_1mb_prose_cycles import SCENARIOS, gate_failures


class ProseCycleContractTests(unittest.TestCase):
    def test_inventory_covers_two_locations_and_both_operations(self) -> None:
        self.assertEqual(len(SCENARIOS), 4)
        self.assertEqual({line for _, line, _ in SCENARIOS}, {130, 600})
        self.assertEqual({case for case, _, _ in SCENARIOS},
                         {"select-delete-cycles", "select-change-cycles"})

    def test_gate_requires_every_cycle_and_strict_boundaries(self) -> None:
        cycles = [{"cycle": index, "total_ns": 49_999_999,
                   "undo_total_ns": 49_999_999} for index in range(100)]
        self.assertEqual(gate_failures(cycles, 100), [])
        self.assertTrue(gate_failures(cycles[:-1], 100))
        self.assertTrue(gate_failures(cycles, 99))
        self.assertTrue(gate_failures([*cycles[:-2], {"cycle": 98,
                                                    "total_ns": 50_000_000,
                                                    "undo_total_ns": 49_999_999},
                                       {"cycle": 99,
                                        "total_ns": 50_000_000,
                                        "undo_total_ns": 49_999_999}], 100))
        self.assertTrue(gate_failures([*cycles[:-1], {"cycle": 99,
                                                    "total_ns": 100_000_000,
                                                    "undo_total_ns": 49_999_999}], 100))
        self.assertTrue(gate_failures([*cycles[:-1], {"cycle": 98,
                                                    "total_ns": 1,
                                                    "undo_total_ns": 49_999_999}], 100))
        self.assertTrue(gate_failures([*cycles[:-1], {**cycles[-1],
                                                    "undo_total_ns": 100_000_000}], 100))

    def test_parse_requires_current_text_mode_and_all_rows(self) -> None:
        scenario = SCENARIOS[0]
        header = "RUN-CYCLES\tca92723f82226a02\tselect-delete-cycles\t600\tj\tflat\t2\t600"
        rows = [f"CYCLE\tca92723f82226a02\tselect-delete-cycles\t600\t{index}"
                "\t100\t200\tdeadbeefdeadbeef\t43651\tnormal\t300\t400"
                for index in range(2)]
        output = "\n".join([header, *rows])
        self.assertEqual(len(parse_cycles(output, scenario, 2,
                                          "ca92723f82226a02", 43662)), 2)
        for broken in ("\n".join([header, rows[0]]),
                       output.replace("\tnormal", "\tinsert"),
                       output.replace("\t300\t", "\t0\t"),
                       output.replace("\t43651\t", "\t43661\t")):
            with self.assertRaises(ValueError):
                parse_cycles(broken, scenario, 2, "ca92723f82226a02", 43662)

    def test_make_targets_are_discoverable_and_gate_rejects_small_count(self) -> None:
        root = Path(__file__).resolve().parent.parent
        help_text = subprocess.check_output(["make", "help"], cwd=root, text=True)
        self.assertIn("bench-acceptance-1mb-prose-cycles", help_text)
        self.assertIn("bench-acceptance-1mb-prose-cycles-record", help_text)
        result = subprocess.run(
            ["python3", "scripts/acceptance_1mb_prose_cycles.py",
             "--binary", "unused", "--fixture", "unused", "--source-root", ".",
             "--gate", "--count", "99"], cwd=root, capture_output=True,
            text=True, check=False)
        self.assertEqual(result.returncode, 2)
        self.assertIn("at least 100 cycles", result.stderr)


if __name__ == "__main__":
    unittest.main()
