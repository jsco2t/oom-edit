"""Acceptance and integrity contracts for public-pane performance comparisons."""

import csv
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import interaction_performance as perf
import interaction_performance_compare as comparison


def dataset():
    return [
        {"fixture": perf.FIXTURE_IDENTITIES[("mixed", 1024 * perf.KIB)],
         "case": case, "step": step, "width": 100, "height": 41,
         "mode": "insert" if case.startswith("source-") else "normal",
         "trial": trial, "cycle": cycle, "phase": perf.sample_phase(case, cycle),
         "input_ns": 800_000, "render_ns": 1_000_000, "total_ns": 2_000_000,
         "peak_rss_bytes": 32 * 1024 * 1024}
        for case in perf.GATE_CASES for trial in range(perf.GATE_TRIALS)
        for cycle in range(perf.GATE_ITERATIONS) for step in perf.CASES[case]
    ]


class InteractionComparisonTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def write(self, name, rows):
        path = self.root / name
        path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
        return path

    def compare(self, baseline, candidate):
        return comparison.compare(
            comparison.load_evidence(self.write("baseline.jsonl", baseline)),
            comparison.load_evidence(self.write("candidate.jsonl", candidate)),
        )

    def cli(self, baseline, candidate, output):
        return subprocess.run(
            [sys.executable, str(Path(comparison.__file__)), "--baseline", str(baseline),
             "--candidate", str(candidate), "--output", str(output)],
            capture_output=True, text=True, check=False,
        )

    def test_unchanged_and_faster_complete_datasets_pass(self):
        baseline = dataset()
        self.assertEqual(len(baseline), 2310)
        for faster in (False, True):
            candidate = [dict(row) for row in baseline]
            if faster:
                for row in candidate:
                    for metric in comparison.METRICS:
                        row[metric] //= 2
            result = self.compare(baseline, candidate)
            self.assertEqual(len(result), 96)
            self.assertTrue(all(row["status"] == "PASS" for row in result))
            for case in ("exit-insert", "exit-insert-mid"):
                groups = {row["phase"]: row["samples"] for row in result if row["case"] == case}
                self.assertEqual(groups, {"first-render": 5, "warmed": 100})

    def test_each_metric_regression_fails_below_absolute_limits(self):
        for metric in comparison.METRICS:
            with self.subTest(metric=metric):
                baseline = dataset()
                candidate = [dict(row) for row in baseline]
                for row in candidate:
                    if row["case"] == "source-type" and row["step"] == "insert":
                        row[metric] *= 2
                        row["total_ns"] = max(row["total_ns"], row["input_ns"] + row["render_ns"])
                result = self.compare(baseline, candidate)
                failed = [row for row in result if row["status"] == "FAIL"]
                self.assertTrue(any(row["metric"] == metric for row in failed))
                self.assertTrue(all(row["case"] == "source-type" and row["step"] == "insert"
                                    and row["failures"] == "median regression" for row in failed))

    def test_both_allowances_are_strict_for_all_metrics(self):
        for metric in comparison.METRICS:
            for baseline, allowance in (
                ((10 * 1024 * 1024, 1_048_576), (40 * 1024 * 1024, 2_097_152))
                if metric == "peak_rss_bytes" else ((500_000, 100_000), (2_000_000, 200_000))
            ):
                with self.subTest(metric=metric, baseline=baseline):
                    self.assertFalse(comparison.median_regresses(baseline, baseline + allowance, metric))
                    self.assertTrue(comparison.median_regresses(baseline, baseline + allowance + 1, metric))
                    self.assertFalse(comparison.median_regresses(baseline, baseline - 1, metric))

    def test_cold_and_warmed_regressions_are_independent_and_tails_are_retained(self):
        baseline = dataset()
        candidate = [dict(row) for row in baseline]
        for row in candidate:
            if row["case"] == "exit-insert" and row["cycle"] == 0:
                row["total_ns"] = 4_000_000
            if row["case"] == "source-type" and row["step"] == "insert" and row["cycle"] == 20:
                row["total_ns"] = 10_000_000
        result = self.compare(baseline, candidate)
        cold = next(row for row in result if row["case"] == "exit-insert"
                    and row["phase"] == "first-render" and row["metric"] == "total_ns")
        warm = next(row for row in result if row["case"] == "exit-insert"
                    and row["phase"] == "warmed" and row["metric"] == "total_ns")
        self.assertEqual((cold["status"], warm["status"]), ("FAIL", "PASS"))
        tail = next(row for row in result if row["case"] == "source-type"
                    and row["step"] == "insert" and row["metric"] == "total_ns")
        self.assertEqual((tail["candidate_median"], tail["candidate_p99"], tail["candidate_worst"]),
                         (2_000_000, 10_000_000, 10_000_000))
        self.assertEqual(tail["status"], "PASS")

    def test_both_datasets_must_pass_existing_absolute_gates(self):
        for side in ("baseline", "candidate"):
            for case, phase, limit in (
                ("source-line", "ordinary", perf.LOCAL_LIMIT_NS),
                ("source-scroll-mid", "ordinary", perf.SCROLL_LIMIT_NS),
                ("exit-insert", "first-render", perf.FIRST_RENDER_LIMIT_NS),
                ("exit-insert-mid", "warmed", perf.LOCAL_LIMIT_NS),
            ):
                with self.subTest(side=side, case=case, phase=phase):
                    baseline, candidate = dataset(), dataset()
                    rows = baseline if side == "baseline" else candidate
                    for row in rows:
                        if row["case"] == case and row["phase"] == phase:
                            row["total_ns"] = limit
                    failed = [row for row in self.compare(baseline, candidate)
                              if row["status"] == "FAIL"]
                    self.assertTrue(any(f"{side}: {case}" in row["failures"] for row in failed))

    def test_invalid_inventory_and_metadata_are_rejected(self):
        good = dataset()
        mutations = [
            good[:-1], good[1:], good + [good[0]],
            [row for row in good if row["case"] != "exit-insert"],
            [good[1], good[0], *good[2:]],
        ]
        for field, value in (
            ("trial", 5), ("cycle", 21), ("step", "delete"), ("case", "save"),
            ("fixture", perf.FIXTURE_IDENTITIES[("mixed", 144 * perf.KIB)]),
            ("fixture", "oom-edit-realistic-v1:mixed:1048576:0123456789abcdef"),
            ("width", 99), ("height", 40), ("mode", "normal"), ("phase", "warmed"),
            ("input_ns", 0), ("render_ns", -1), ("total_ns", 1), ("peak_rss_bytes", 0),
            ("trial", False), ("cycle", "0"), ("input_ns", 800_000.0),
        ):
            mutations.append([dict(good[0], **{field: value}), *good[1:]])
        missing_field = dict(good[0])
        del missing_field["phase"]
        mutations.extend([[missing_field, *good[1:]], [dict(good[0], extra=1), *good[1:]]])
        cold_index = next(index for index, row in enumerate(good) if row["phase"] == "first-render")
        warm_index = cold_index + 1
        for index, phase in ((cold_index, "warmed"), (warm_index, "first-render")):
            rows = [dict(row) for row in good]
            rows[index]["phase"] = phase
            mutations.append(rows)
        for index, invalid in enumerate(mutations):
            with self.subTest(index=index), self.assertRaises(ValueError):
                comparison.load_evidence(self.write("bad.jsonl", invalid))

    def test_malformed_nonobject_and_duplicate_json_fields_are_rejected(self):
        for text in ("", "\n", "{broken}\n", "[]\n", "null\n",
                     json.dumps(dataset()[0]).replace('"trial": 0', '"trial": 0, "trial": 1') + "\n"):
            with self.subTest(text=text[:30]), self.assertRaises(ValueError):
                path = self.root / "bad.jsonl"
                path.write_text(text, encoding="utf-8")
                comparison.load_evidence(path)

    def test_cli_preserves_complete_output_and_raw_inputs_on_regression(self):
        baseline, candidate = dataset(), dataset()
        for row in candidate:
            if row["case"] == "source-type-mid":
                row["total_ns"] *= 2
        before = self.write("baseline.jsonl", baseline)
        after = self.write("candidate.jsonl", candidate)
        original = (before.read_bytes(), after.read_bytes())
        output = self.root / "nested" / "comparison.tsv"
        result = self.cli(before, after, output)
        self.assertEqual(result.returncode, 1, result.stderr)
        with output.open(newline="") as source:
            rows = list(csv.DictReader(source, delimiter="\t"))
        self.assertEqual(len(rows), 96)
        self.assertEqual(sum(row["status"] == "FAIL" for row in rows), 2)
        self.assertEqual((before.read_bytes(), after.read_bytes()), original)
        result = self.cli(before, self.write("candidate.jsonl", baseline), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("FAIL", output.read_text())

    def test_cli_rejects_invalid_evidence_and_output_aliasing(self):
        baseline = self.write("baseline.jsonl", dataset())
        candidate = self.write("candidate.jsonl", dataset()[:-1])
        output = self.root / "out.tsv"
        self.assertEqual(self.cli(baseline, candidate, output).returncode, 2)
        self.assertFalse(output.exists())
        self.assertEqual(self.cli(baseline, candidate, baseline).returncode, 2)
        self.assertEqual(len(baseline.read_text().splitlines()), 2310)
        output.hardlink_to(baseline)
        self.assertEqual(self.cli(baseline, candidate, output).returncode, 2)
        self.assertEqual(len(baseline.read_text().splitlines()), 2310)

    def test_warmed_regression_does_not_receive_cold_allowance(self):
        baseline, candidate = dataset(), dataset()
        for row in candidate:
            if row["case"] == "exit-insert-mid" and row["phase"] == "warmed":
                row["total_ns"] *= 2
        failed = [row for row in self.compare(baseline, candidate) if row["status"] == "FAIL"]
        self.assertEqual([(row["case"], row["phase"], row["metric"]) for row in failed],
                         [("exit-insert-mid", "warmed", "total_ns")])


if __name__ == "__main__":
    unittest.main()
