"""Contract tests for repeated public-pane interaction measurements."""

import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import interaction_performance as perf


class InteractionPerformanceTests(unittest.TestCase):
    def row(self, case="source-type", step="insert", total=31, mode="insert",
            width=100, height=41):
        identity = perf.FIXTURE_IDENTITIES[("mixed", 147456)]
        return (f"INTERACTION\t{identity}\t{case}\t{step}\t{width}\t{height}\t{mode}"
                f"\t10\t20\t{total}\t1024")

    def test_sample_rows_require_identity_case_and_complete_phase_timing(self):
        identity = perf.FIXTURE_IDENTITIES[("mixed", 147456)]
        row = self.row()
        sample = perf.parse_sample(row, "mixed", 147456, "source-type")
        self.assertEqual((sample["width"], sample["height"], sample["mode"]),
                         (100, 41, "insert"))
        self.assertEqual(sample["input_ns"], 10)
        self.assertEqual(sample["render_ns"], 20)
        self.assertEqual(sample["total_ns"], 31)
        self.assertEqual(sample["peak_rss_bytes"], 1024)
        for invalid in (
            row.replace("source-type", "rendered-scroll"),
            row.replace(":147456:", ":147457:"),
            row.replace("\t100\t41\t", "\t99\t41\t"),
            row.replace("\t100\t41\t", "\t100\t40\t"),
            row.replace("\tinsert\t", "\tnormal\t"),
            row.replace("\t10\t", "\t0\t"),
            row.replace("\t31\t", "\t29\t"),
            row.replace(identity[-16:], "0123456789abcdef"),
            row.replace("\t1024", "\t-1"),
            row.replace("\t1024", "\t0"),
        ):
            with self.assertRaises(ValueError):
                perf.parse_sample(invalid, "mixed", 147456, "source-type")

    def test_nearest_rank_percentiles_and_worst_keep_tail_visible(self):
        values = list(range(1, 101))
        summary = perf.summarize(values)
        self.assertEqual(summary["median"], 50.5)
        self.assertEqual(summary["p95"], 95)
        self.assertEqual(summary["p99"], 99)
        self.assertEqual(summary["worst"], 100)
        with self.assertRaises(ValueError):
            perf.summarize([])

    def test_sample_rejects_missing_interaction_step(self):
        row = self.row()
        result = SimpleNamespace(returncode=0, stdout=f"{row}\n{row}\n", stderr="")
        with patch.object(perf.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(ValueError, "out-of-order or incomplete interaction steps"):
                perf.sample(Path("probe"), "mixed", 147456, "source-type", 1)

    def test_sample_rejects_reordered_complete_steps(self):
        insert = self.row()
        delete = self.row(step="delete")
        result = SimpleNamespace(
            returncode=0, stdout=f"{insert}\n{insert}\n{delete}\n{delete}\n", stderr=""
        )
        with patch.object(perf.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(ValueError, "out-of-order"):
                perf.sample(Path("probe"), "mixed", 147456, "source-type", 2)

    def test_case_inventory_covers_local_and_global_mutations(self):
        for case in (
            "source-type-mid", "source-line", "source-line-mid", "source-frontmatter",
            "source-reference", "fence-language", "fence-language-mid",
            "fence-unknown", "fence-unknown-mid",
            "source-scroll-mid", "rendered-scroll-mid", "rendered-edit",
            "rendered-edit-mid", "save", "resize", "reload",
        ):
            self.assertIn(case, perf.CASES)
        self.assertEqual(perf.CASE_CLASSES["source-reference"], "mixed-reference")

    def test_return_gate_separates_exactly_one_first_render_per_trial(self):
        rows = [
            {"case": "exit-insert", "step": "escape", "trial": trial,
             "cycle": cycle, "total_ns": 220_000_000 if cycle == 0 else 1_000_000}
            for trial in range(5) for cycle in range(21)
        ]
        self.assertEqual(perf.gate_failures(rows, "exit-insert", 5, 21), [])
        cold_over = [dict(row) for row in rows]
        cold_over[0]["total_ns"] = 500_000_000
        self.assertIn("exit-insert first rendered projection",
                      perf.gate_failures(cold_over, "exit-insert", 5, 21))
        warmed_over = [dict(row) for row in rows]
        warmed_over[1]["total_ns"] = 50_000_000
        self.assertIn("exit-insert warmed return",
                      perf.gate_failures(warmed_over, "exit-insert", 5, 21))
        short = [row for row in rows if row["cycle"] < 20]
        with self.assertRaisesRegex(ValueError, "five trials and 21 cycles"):
            perf.gate_failures(short, "exit-insert", 5, 20)

    def test_return_gate_rejects_missing_or_reclassified_cycles(self):
        rows = [
            {"case": "exit-insert-mid", "step": "escape", "trial": trial,
             "cycle": cycle, "total_ns": 1_000_000}
            for trial in range(5) for cycle in range(21)
        ]
        for invalid in (rows[1:], rows[:-1],
                        [*rows[:21], dict(rows[21], cycle=1), *rows[22:]]):
            with self.assertRaisesRegex(ValueError, "sample count|trial/cycle"):
                perf.gate_failures(invalid, "exit-insert-mid", 5, 21)

    def test_gate_rejects_local_and_scroll_tails(self):
        for case, limit in (("source-line", 50_000_000),
                            ("source-scroll-mid", 25_000_000)):
            rows = [
                {"case": case, "step": step, "trial": trial, "cycle": cycle,
                 "total_ns": 1_000_000}
                for trial in range(5) for cycle in range(21)
                for step in perf.CASES[case]
            ]
            self.assertEqual(perf.gate_failures(rows, case, 5, 21), [])
            rows[0]["total_ns"] = limit
            rows[2]["total_ns"] = limit
            self.assertIn(f"{case}/{perf.CASES[case][0]}",
                          perf.gate_failures(rows, case, 5, 21))


if __name__ == "__main__":
    unittest.main()
