import tempfile
import unittest
from pathlib import Path

import tui_performance as perf


def evidence(
    role: str,
    *,
    wall: int = 1_000_000,
    cpu: int = 800_000,
    rss: int = 10_000_000,
    snapshot: int = 10_000_000,
):
    rows = []
    for case in perf.CASES:
        for trial in range(1, 6):
            rows.append(
                {
                    "schema-version": "1",
                    "revision": "a" * 40,
                    "branch-role": role,
                    "rustc": "rustc test",
                    "os": "test-os",
                    "arch": "test-arch",
                    "cpu-model": "test-cpu",
                    "logical-cpus": "8",
                    "memory-bytes": "16000000000",
                    "fixture-version": perf.FIXTURE_VERSION,
                    "case": case,
                    "trial": str(trial),
                    "iterations": "100",
                    "measured-ns": str(wall * 100),
                    "wall-average-ns-per-frame": str(wall),
                    "wall-worst-ns-per-frame": str(wall + trial),
                    "cpu-ns-per-frame": str(cpu),
                    "peak-rss-bytes": str(rss),
                    "snapshot-heap-bytes": str(snapshot),
                    "status": "pass",
                }
            )
    return rows


class TuiPerformanceTests(unittest.TestCase):
    def test_tsv_v1_round_trips_fixed_columns_and_rejects_control_characters(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.tsv"
            rows = evidence("baseline")
            perf.write_tsv(path, perf.FIELDS, rows)
            self.assertEqual(tuple(path.read_text().splitlines()[0].split("\t")), perf.FIELDS)
            self.assertEqual(perf.read_tsv(path), rows)
            rows[0]["cpu-model"] = "bad\tmodel"
            with self.assertRaisesRegex(perf.EvidenceError, "tab or newline"):
                perf.write_tsv(path, perf.FIELDS, rows)

            path.write_text("wrong\theader\n", encoding="utf-8")
            with self.assertRaisesRegex(perf.EvidenceError, "incompatible TSV header"):
                perf.read_tsv(path)

    def test_comparator_rejects_incomplete_duplicate_and_incompatible_evidence(self):
        baseline = evidence("baseline")
        candidate = evidence("candidate")
        incomplete = [
            row
            for row in baseline
            if not (row["case"] == perf.BASELINE_CASES[0] and row["trial"] == "5")
        ]
        with self.assertRaisesRegex(perf.EvidenceError, "at least five"):
            perf.compare_records(incomplete, candidate)
        duplicate = candidate + [dict(candidate[0])]
        with self.assertRaisesRegex(perf.EvidenceError, "duplicate trial"):
            perf.compare_records(baseline, duplicate)
        incompatible = evidence("candidate")
        for row in incompatible:
            row["rustc"] = "different"
        with self.assertRaisesRegex(perf.EvidenceError, "mismatch for rustc"):
            perf.compare_records(baseline, incompatible)

        missing_case = [row for row in candidate if row["case"] != perf.CASES[-1]]
        with self.assertRaisesRegex(perf.EvidenceError, "missing cases"):
            perf.compare_records(baseline, missing_case)

        nonconsecutive = evidence("candidate")
        nonconsecutive[4]["trial"] = "6"
        with self.assertRaisesRegex(perf.EvidenceError, "consecutive and one-based"):
            perf.compare_records(baseline, nonconsecutive)

    def test_legacy_baseline_accepts_candidate_only_absolute_and_scaling_cases(self):
        baseline = [
            row for row in evidence("baseline") if row["case"] in perf.BASELINE_CASES
        ]
        candidate = evidence("candidate")
        for row in candidate:
            case = row["case"]
            if case in perf.MARKER_LINE_COUNTS:
                row["snapshot-heap-bytes"] = str(perf.MARKER_LINE_COUNTS[case] * 16)
            if case.startswith("idle-gutter-"):
                row["wall-average-ns-per-frame"] = "100000"
                row["wall-worst-ns-per-frame"] = "100005"

        comparisons = perf.compare_records(baseline, candidate)
        self.assertTrue(all(row["status"] == "pass" for row in comparisons))
        self.assertEqual(
            {
                row["case"]
                for row in comparisons
                if row["case"].endswith("gutter-offscreen-scaling")
            },
            {"source-gutter-offscreen-scaling", "rendered-gutter-offscreen-scaling"},
        )
        self.assertTrue(
            any(
                row["case"] == "source-gutter-dense-50000"
                and row["metric"] == "snapshot-heap-bytes"
                and row["threshold"] == "at-most-1204096bytes"
                for row in comparisons
            )
        )

        oversized = [dict(row) for row in candidate]
        target = next(
            row for row in oversized if row["case"] == "source-gutter-dense-50000"
        )
        target["snapshot-heap-bytes"] = "1204097"
        oversized_comparison = perf.compare_records(baseline, oversized)
        self.assertTrue(
            any(
                row["status"] == "fail-snapshot-memory-limit"
                for row in oversized_comparison
            )
        )

        slow_scaling = [dict(row) for row in candidate]
        for row in slow_scaling:
            if row["case"] == "source-gutter-dense-50000":
                row["wall-average-ns-per-frame"] = "1300000"
                row["wall-worst-ns-per-frame"] = "1300005"
                row["cpu-ns-per-frame"] = "1300000"
        scaling_comparison = perf.compare_records(baseline, slow_scaling)
        self.assertEqual(
            {
                row["metric"]
                for row in scaling_comparison
                if row["case"] == "source-gutter-offscreen-scaling"
                and row["status"] == "fail-offscreen-scaling"
            },
            {"wall-average-ns-per-frame", "cpu-ns-per-frame"},
        )

    def test_comparator_rejects_invalid_numeric_values_and_units(self):
        baseline = evidence("baseline")
        candidate = evidence("candidate")
        candidate[0]["iterations"] = "frames"
        with self.assertRaisesRegex(perf.EvidenceError, "iterations must be an integer"):
            perf.compare_records(baseline, candidate)

        candidate = evidence("candidate")
        candidate[0]["peak-rss-bytes"] = "-1"
        with self.assertRaisesRegex(perf.EvidenceError, "must not be negative"):
            perf.compare_records(baseline, candidate)

        candidate = evidence("candidate")
        candidate[0]["wall-worst-ns-per-frame"] = "999999"
        with self.assertRaisesRegex(perf.EvidenceError, "worst must not be below"):
            perf.compare_records(baseline, candidate)

    def test_comparator_applies_dual_thresholds_at_boundaries(self):
        baseline = evidence("baseline", wall=1_000_000, cpu=1_000_000, rss=10_000_000)
        within_absolute = evidence(
            "candidate",
            wall=1_099_999,
            cpu=1_099_999,
            rss=11_048_576,
            snapshot=11_048_576,
        )
        self.assertTrue(
            all(row["status"] == "pass" for row in perf.compare_records(baseline, within_absolute))
        )
        over_both = evidence(
            "candidate",
            wall=1_100_001,
            cpu=1_100_001,
            rss=11_048_577,
            snapshot=11_048_577,
        )
        comparisons = perf.compare_records(baseline, over_both)
        self.assertTrue(all(row["status"] == "fail-regression" for row in comparisons))

    def test_comparator_emits_exact_median_worst_and_delta_aggregates(self):
        baseline = evidence("baseline")
        candidate = evidence("candidate")
        for index, row in enumerate(
            row for row in baseline if row["case"] == perf.CASES[0]
        ):
            row["wall-average-ns-per-frame"] = str(100 + index)
            row["wall-worst-ns-per-frame"] = str(200 + index)
        for index, row in enumerate(
            row for row in candidate if row["case"] == perf.CASES[0]
        ):
            row["wall-average-ns-per-frame"] = str(110 + index)
            row["wall-worst-ns-per-frame"] = str(220 + index)
        comparison = next(
            row
            for row in perf.compare_records(baseline, candidate)
            if row["case"] == perf.CASES[0]
            and row["metric"] == "wall-average-ns-per-frame"
        )
        self.assertEqual(comparison["baseline-median"], "102")
        self.assertEqual(comparison["baseline-worst"], "104")
        self.assertEqual(comparison["candidate-median"], "112")
        self.assertEqual(comparison["candidate-worst"], "114")
        self.assertEqual(comparison["delta"], "10")
        self.assertEqual(comparison["status"], "pass")

    def test_rendered_first_frame_rss_limit_is_exact(self):
        limit = perf.RENDERED_FIRST_FRAME_RSS_LIMIT
        self.assertEqual(
            perf.apply_process_limits("rendered-first-frame", "pass", limit), "pass"
        )
        self.assertEqual(
            perf.apply_process_limits("rendered-first-frame", "pass", limit + 1),
            "fail-peak-rss-limit",
        )
        self.assertEqual(
            perf.apply_process_limits("source-first-frame", "pass", limit + 1), "pass"
        )


if __name__ == "__main__":
    unittest.main()
