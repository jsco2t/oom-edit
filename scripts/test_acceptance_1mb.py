"""Contract tests for exact-fixture interaction samples."""

from __future__ import annotations

import unittest
import subprocess
from pathlib import Path
from unittest.mock import patch

from acceptance_1mb import (
    FIXTURE_SHA256, MAIN_REVISION, NAVIGATION, SELECTION, fixture_identity,
    navigation_scenarios, operation_gate_fails, parse_run, revision_identity, scenarios,
    selection_motion_scenarios,
)
from acceptance_1mb_pty import (
    EXTRA_REGIONS, FRAME_END, KEYS, REGIONS, poll_timeout, verify_trace,
    visible_source_line,
)
from acceptance_1mb_core import parse_output
from acceptance_1mb_edit_cycles import SCENARIOS, parse_cycles


class AcceptanceRunnerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture_hash = "ca92723f82226a02"

    def rows(self, *, case: str = "nav", count: int = 3) -> str:
        common = f"{self.fixture_hash}\t{case}\t500\tj\tflat"
        rows = [f"RUN\t{common}\t{count}\t500\t400000000\t100000000\t100000000"]
        for index in range(count):
            rows.append(
                f"KEY\t{common}\t{index}\t{index * 1000}\t100\t200\t"
                f"{index * 1000 + 300}\t0\t{501 + index}\t10\t5\t"
                f"{'normal' if case == 'nav' else 'select'}\ttrue"
            )
        if case != "nav":
            mode = "normal" if case == "select-delete" else "insert"
            rows.append(
                f"EDIT\t{common}\t{count}\t100\t200\t43662\t43646\t"
                f"deadbeefdeadbeef\t{mode}"
            )
        return "\n".join(rows)

    def parse(self, output: str, *, case: str = "nav", count: int = 3):
        return parse_run(output, fixture_hash=self.fixture_hash, case=case,
                         line=500, motion="j", state="flat", count=count)

    def test_fixture_is_exact_static_document(self) -> None:
        root = Path(__file__).resolve().parent.parent
        self.assertEqual(fixture_identity(root / "examples/kitchen-sink-1mb.md"),
                         self.fixture_hash)
        self.assertEqual(len(FIXTURE_SHA256), 64)

    def test_edit_cycle_contract_requires_every_exact_result(self) -> None:
        self.assertEqual(len(SCENARIOS), 4)
        scenario = SCENARIOS[0]
        header = ("RUN-CYCLES\tca92723f82226a02\tselect-delete-cycles\t3000"
                  "\tj\tflat\t2\t3000")
        cycles = [
            f"CYCLE\tca92723f82226a02\tselect-delete-cycles\t3000\t{index}"
            "\t100\t200\tdeadbeefdeadbeef\t43647\tnormal\t300\t400"
            for index in range(2)
        ]
        output = "\n".join([header, *cycles])
        self.assertEqual(len(parse_cycles(output, scenario, 2,
                                          self.fixture_hash, 43662)), 2)
        for broken in (
            "\n".join([header, cycles[0]]),
            output.replace("\t43647\t", "\t43653\t"),
            output.replace("\t200\t", "\t0\t"),
            output.replace("\t300\t", "\t0\t"),
            output.replace("\tnormal", "\tinsert"),
            output.replace("deadbeefdeadbeef", self.fixture_hash),
        ):
            with self.assertRaises(ValueError):
                parse_cycles(broken, scenario, 2, self.fixture_hash, 43662)

    def test_operation_gate_matches_approved_fence_latency_scope(self) -> None:
        slow = {"input_ns": 600_000_000, "render_ns": 200_000_000,
                "removed_lines": 11}
        fast = {"input_ns": 20_000_000, "render_ns": 10_000_000,
                "removed_lines": 11}
        incomplete = {**fast, "removed_lines": 1}
        self.assertFalse(operation_gate_fails("select-delete-prose", [slow]))
        self.assertTrue(operation_gate_fails("select-delete-prose", [incomplete]))
        for language in ("rust", "go"):
            for operation in ("delete", "change"):
                scenario = f"select-{operation}-{language}"
                self.assertTrue(operation_gate_fails(scenario, [slow]))
                self.assertTrue(operation_gate_fails(scenario, [incomplete]))
                self.assertFalse(operation_gate_fails(scenario, [fast]))

    def test_scenario_inventory_covers_regions_directions_and_states(self) -> None:
        inventory = scenarios()
        self.assertEqual(len(NAVIGATION), 6)
        self.assertEqual(len(SELECTION), 3)
        self.assertEqual(len(inventory), 18)
        self.assertEqual(sum(count == 1000 for _, _, _, count in inventory), 12)
        self.assertEqual({state for _, _, state, _ in inventory}, {"flat", "retained"})
        self.assertEqual({motion for _, (_, _, motion), _, _ in inventory},
                         {"j", "k", "down", "up"})
        self.assertEqual(navigation_scenarios(), inventory[:12])
        self.assertEqual({category for category, _, _, _ in navigation_scenarios()},
                         {"navigation"})
        select_motions = selection_motion_scenarios()
        self.assertEqual(len(select_motions), 12)
        self.assertEqual({state for _, _, state, _ in select_motions},
                         {"flat", "retained"})
        self.assertEqual({count for _, _, _, count in select_motions}, {15})

    def test_every_key_and_operation_result_is_parsed(self) -> None:
        run, keys, edit = self.parse(self.rows())
        self.assertEqual(run["cold_ns"], 400_000_000)
        self.assertEqual([key["source_line"] for key in keys], [501, 502, 503])
        self.assertIsNone(edit)
        _, keys, edit = self.parse(self.rows(case="select-delete"),
                                   case="select-delete")
        self.assertEqual(len(keys), 3)
        self.assertEqual(edit["removed_lines"], 16)

    def test_missing_wrong_order_or_wrong_fixture_is_rejected(self) -> None:
        rows = self.rows().splitlines()
        for broken in (
            "\n".join(rows[:-1]),
            "\n".join([rows[0], rows[2], rows[1], rows[3]]),
            "\n".join(row.replace(self.fixture_hash, "0000000000000000") for row in rows),
        ):
            with self.assertRaises(ValueError):
                self.parse(broken)

    def test_bad_timing_cursor_mode_and_edit_result_are_rejected(self) -> None:
        rows = self.rows().splitlines()
        for broken in (
            "\n".join(row.replace("\t100\t200\t", "\t0\t200\t") for row in rows),
            "\n".join(row.replace("\t10\t5\tnormal", "\t41\t5\tnormal") for row in rows),
            "\n".join(row.replace("\tnormal\ttrue", "\tinsert\ttrue") for row in rows),
            "\n".join(row.replace("\tnormal\ttrue", "\tnormal\tfalse") for row in rows),
        ):
            with self.assertRaises(ValueError):
                self.parse(broken)
        selected = self.rows(case="select-change").replace("\tinsert", "\tnormal")
        with self.assertRaises(ValueError):
            self.parse(selected, case="select-change")

    def test_pty_contract_rejects_missing_keys_and_output(self) -> None:
        self.assertEqual(FRAME_END, b"\x1b[?2026l")
        self.assertEqual(set(KEYS), {"j", "k", "down", "up"})
        self.assertEqual(len(REGIONS), 6)
        self.assertEqual({key for _, _, key in REGIONS}, set(KEYS))
        self.assertEqual(EXTRA_REGIONS, (("fresh-down", 0, "down"),))
        serial = [{"index": index, "sent_ns": 1000 + index * 1000,
                   "flush_proxy_ns": 1200 + index * 1000, "latency_ns": 200,
                   "output_bytes": 10, "frames_since_key": 1,
                   "key_output_sha256": "a" * 64} for index in range(3)]
        sent = [{"index": index, "sent_ns": 1000 + index * 1000}
                for index in range(3)]
        frames = [{"flush_proxy_ns": 3500, "keys_sent": 3, "gap_ns": 0,
                   "keys_during_gap": 3}]
        verify_trace(serial, sent, frames, 3, 0)
        with self.assertRaises(ValueError):
            verify_trace(serial, sent, frames, 3, None)
        for bad_serial, bad_sent, bad_frames in (
            (serial[:-1], sent, frames),
            (serial, sent[:-1], frames),
            (serial, sent, []),
            (serial, sent, [{**frames[0], "keys_sent": 2}]),
            ([{**serial[0], "output_bytes": 0}, *serial[1:]], sent, frames),
            ([{**serial[0], "key_output_sha256": "bad"}, *serial[1:]], sent, frames),
        ):
            with self.assertRaises(ValueError):
                verify_trace(bad_serial, bad_sent, bad_frames, 3, 0)

    def test_pty_poll_deadline_race_never_passes_negative_timeout(self) -> None:
        self.assertEqual(poll_timeout(100, 101), 0.0)
        self.assertEqual(poll_timeout(1_000_000_000, 0), 0.001)

    def test_pty_reads_gutter_when_status_updates_are_partial(self) -> None:
        output = (b"\x1b[42;2H19913\x1b[62;2H19933"
                  b"\x1b[63;213H0 199\x1b[63;221H3:1")
        self.assertEqual(visible_source_line(output), 19933)
        self.assertIsNone(visible_source_line(b"\x1b[63;221H3:1"))

    def test_pty_rejects_nonpositive_first_frame_timeout(self) -> None:
        root = Path(__file__).resolve().parent.parent
        result = subprocess.run(
            ["python3", "scripts/acceptance_1mb_pty.py", "--binary", "unused",
             "--fixture", "unused", "--source-root", ".", "--role", "candidate",
             "--first-frame-timeout-s", "0"],
            cwd=root, capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("first-frame timeout must be positive", result.stderr)

    def test_select_motion_gate_requires_100_samples_per_scenario(self) -> None:
        root = Path(__file__).resolve().parent.parent
        result = subprocess.run(
            ["python3", "scripts/acceptance_1mb.py", "--binary", "unused",
             "--fixture", "unused", "--source-root", ".", "--role", "candidate",
             "--select-motion-gate", "--trials", "6"],
            cwd=root, capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("requires seven trials", result.stderr)

    def test_main_revision_requires_frozen_clean_tree(self) -> None:
        with patch("acceptance_1mb.subprocess.check_output", return_value=MAIN_REVISION + "\n"):
            with patch("acceptance_1mb.subprocess.run") as git_diff:
                git_diff.return_value.returncode = 0
                self.assertEqual(revision_identity(Path("/tmp/main"), "main"), MAIN_REVISION)
                git_diff.return_value.returncode = 1
                with self.assertRaises(ValueError):
                    revision_identity(Path("/tmp/main"), "main")
        with patch("acceptance_1mb.subprocess.check_output", return_value="f" * 40):
            with self.assertRaises(ValueError):
                revision_identity(Path("/tmp/main"), "main")

    def test_core_phase_probe_requires_all_ordered_records(self) -> None:
        rows = [f"CORE\t3000\t{phase}\t100" for phase in
                ("source-init", "full-layout", "select-entry")]
        rows.extend(f"CORE\t3000\tmotion\t{index}\t100\t200"
                    for index in range(15))
        self.assertEqual(len(parse_output("\n".join(rows), 3000)), 18)
        for broken in (rows[:-1], [*rows[:4], rows[5], rows[4], *rows[6:]],
                       [rows[0].replace("3000", "20000"), *rows[1:]]):
            with self.assertRaises(ValueError):
                parse_output("\n".join(broken), 3000)

    def test_make_help_lists_exact_document_workflows(self) -> None:
        root = Path(__file__).resolve().parent.parent
        help_text = subprocess.check_output(["make", "help"], cwd=root, text=True)
        for target in (
            "bench-acceptance-1mb-record", "bench-acceptance-1mb",
            "bench-acceptance-1mb-navigation",
            "bench-acceptance-1mb-select-motion",
            "bench-acceptance-1mb-pty-record", "bench-acceptance-1mb-core-phases",
            "bench-acceptance-1mb-mutation-phases",
            "bench-acceptance-1mb-feasibility",
            "bench-acceptance-1mb-injection-parse",
            "bench-acceptance-1mb-fence-model",
            "bench-acceptance-1mb-selection-prototype",
            "bench-acceptance-1mb-code-rows",
            "bench-acceptance-1mb-source-prototype",
            "bench-acceptance-1mb-row-splice",
            "bench-acceptance-1mb-vim-splice",
            "bench-acceptance-1mb-baseline-prepare",
            "bench-acceptance-1mb-baseline-record",
            "bench-acceptance-1mb-baseline-pty-record",
            "bench-acceptance-1mb-baseline-core-phases",
            "bench-acceptance-1mb-compare", "acceptance-1mb-perf-test",
        ):
            self.assertIn(target, help_text)

    def test_ci_gate_runs_new_measurement_contracts(self) -> None:
        root = Path(__file__).resolve().parent.parent
        makefile = (root / "Makefile").read_text(encoding="utf-8")
        check_recipe = makefile.split("\ncheck: ##", 1)[1].split("\n.PHONY:", 1)[0]
        self.assertIn("test_interaction_performance.py", check_recipe)
        self.assertIn("test_acceptance_1mb.py", check_recipe)


if __name__ == "__main__":
    unittest.main()
