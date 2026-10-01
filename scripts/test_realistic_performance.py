"""Contract tests for the realistic performance recorder."""

import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import realistic_performance as perf


class RealisticPerformanceTests(unittest.TestCase):
    def test_measurement_requires_matching_version_size_and_nonzero_work(self):
        identity = "oom-edit-realistic-v1:mixed:147456:0123456789abcdef"
        row = (
            f"MEASURE\t{identity}\t100\t41\tnormal\tstart\t10\t20\t30\t40\t110\t1000"
        )
        sample = perf.parse_sample(row, "MEASURE", 147456)
        self.assertEqual(sample["total_ns"], 110)
        self.assertEqual(sample["peak_rss_bytes"], 1000)
        with self.assertRaises(ValueError):
            perf.parse_sample(row, "MEASURE", 147457)
        with self.assertRaises(ValueError):
            perf.parse_sample(row.replace("\t30\t", "\t0\t"), "MEASURE", 147456)

    def test_layout_and_trigger_rows_are_checked(self):
        identity = "oom-edit-realistic-v1:lists:262144:0123456789abcdef"
        layout = perf.parse_sample(
            f"LAYOUT\t{identity}\t96\t100\t200\t300\t400", "LAYOUT", 262144
        )
        self.assertEqual(layout["layout_heap_bytes"], 300)
        frame = perf.parse_sample(
            f"MEASURE\t{identity}\t100\t41\tnormal\tstart\t1\t2\t3\t4\t11\t12\n"
            f"TRIGGER\t{identity}\tsave\t13",
            "MEASURE",
            262144,
        )
        self.assertEqual(frame["trigger_ns"], 13)
        with self.assertRaises(ValueError):
            perf.parse_sample("LAYOUT\twrong\t96\t100\t200\t300\t400", "LAYOUT", 262144)
        with self.assertRaises(ValueError):
            perf.parse_sample(
                f"MEASURE\t{identity}\t100\t41\tnormal\tstart\t1\t2\t3\t4\t11\t12\n"
                f"TRIGGER\t{identity}\tsave\t13\nTRIGGER\t{identity}\tsave\t13",
                "MEASURE", 262144,
            )

    def test_sample_rejects_wrong_fixture_mode_dimensions_and_trigger(self):
        identity = f"{perf.VERSION}:mixed:147456:{perf.FIXTURE_HASHES[('mixed', 147456)]}"
        row = f"MEASURE\t{identity}\t100\t41\tnormal\tstart\t1\t2\t3\t4\t11\t12"
        valid = SimpleNamespace(returncode=0, stdout=row, stderr="")
        with patch.object(perf.subprocess, "run", return_value=valid):
            self.assertEqual(perf.sample(Path("probe"), "mixed", 147456, 100, 41,
                                         "normal")["fixture"], identity)
        for invalid in (
            row.replace(identity[-16:], "0123456789abcdef"),
            row.replace("\t100\t41\t", "\t99\t41\t"),
            row.replace("\tnormal\t", "\tsource\t"),
            row.replace("\tstart\t", "\tlast\t"),
            row + f"\nTRIGGER\t{identity}\tsave\t13",
        ):
            result = SimpleNamespace(returncode=0, stdout=invalid, stderr="")
            with patch.object(perf.subprocess, "run", return_value=result):
                with self.assertRaises(ValueError):
                    perf.sample(Path("probe"), "mixed", 147456, 100, 41, "normal")

    def test_revised_cold_and_memory_contracts_are_fixed(self):
        self.assertEqual(perf.cold_limit_ns("mixed", 1024 * perf.KIB), 500_000_000)
        self.assertEqual(perf.cold_limit_ns("tables", 448 * perf.KIB), 391_050_000)
        self.assertEqual(perf.cold_limit_ns("lists", 512 * perf.KIB), 350_000_000)
        self.assertTrue(perf.within_memory_budget(110, 100))
        self.assertFalse(perf.within_memory_budget(111, 100))
        self.assertTrue(perf.within_memory_budget(115, 100, "rust-fence", "rss"))
        self.assertFalse(perf.within_memory_budget(116, 100, "rust-fence", "rss"))
        self.assertFalse(perf.within_memory_budget(111, 100, "rust-fence", "heap"))
        self.assertFalse(perf.within_memory_budget(111, 100, "mixed", "rss"))
        self.assertEqual(perf.FIXTURE_HASHES[("mixed", 1024 * perf.KIB)],
                         "a2669d94455fc82d")
        self.assertEqual(set(perf.COLD_RSS_BASELINE), set(perf.GATE_CASES))
        self.assertEqual(set(perf.LAYOUT_MEMORY_BASELINE),
                         {(name, size) for name in perf.LAYOUT_CLASSES
                          for size in perf.LAYOUT_SIZES})

    def test_scaling_rejects_exactly_above_two_and_a_quarter(self):
        self.assertTrue(perf.growth_ok(100, 225))
        self.assertFalse(perf.growth_ok(100, 226))


if __name__ == "__main__":
    unittest.main()
