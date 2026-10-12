#!/usr/bin/env python3
"""Compare complete public-pane gate evidence without relaxing absolute budgets."""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path

import interaction_performance as perf

METRICS = ("input_ns", "render_ns", "total_ns", "peak_rss_bytes")
INTEGER_FIELDS = (*METRICS, "width", "height", "trial", "cycle")
SAMPLE_FIELDS = {*INTEGER_FIELDS, "fixture", "case", "step", "mode", "phase"}
OUTPUT_FIELDS = (
    "case", "step", "phase", "samples", "metric",
    "baseline_median", "baseline_p99", "baseline_worst",
    "candidate_median", "candidate_p99", "candidate_worst",
    "delta", "relative_percent", "threshold_percent", "threshold_absolute",
    "status", "failures",
)


def unique_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON field: {key}")
        result[key] = value
    return result


def load_evidence(path: Path) -> dict[str, list[dict[str, object]]]:
    """Require exactly the native gate inventory and validate every raw sample."""
    cases: dict[str, list[dict[str, object]]] = {case: [] for case in perf.GATE_CASES}
    with path.open(encoding="utf-8") as source:
        for line_number, line in enumerate(source, 1):
            try:
                row = json.loads(line, object_pairs_hook=unique_object)
                if not isinstance(row, dict) or set(row) != SAMPLE_FIELDS:
                    raise ValueError("incompatible interaction fields")
                if any(type(row[field]) is not int for field in INTEGER_FIELDS):
                    raise ValueError("interaction coordinates and measurements must be integers")
                case = row["case"]
                if not isinstance(case, str) or case not in cases:
                    raise ValueError("unknown or nongate interaction case")
                # Reuse the recorder's exact fixture, geometry, mode and timing checks.
                raw = "\t".join(str(row[field]) for field in (
                    "fixture", "case", "step", "width", "height", "mode", *METRICS,
                ))
                perf.parse_sample("INTERACTION\t" + raw, perf.CASE_CLASSES.get(case, "mixed"),
                                  1024 * perf.KIB, case)
                if row["phase"] != perf.sample_phase(case, row["cycle"]):
                    raise ValueError("wrong interaction phase classification")
                cases[case].append(row)
            except ValueError as error:
                raise ValueError(f"{path}:{line_number}: {error}") from error
    for case, rows in cases.items():
        perf.verify_cycles(rows, case, perf.GATE_TRIALS, perf.GATE_ITERATIONS)
    return cases


def median_regresses(baseline: float | int, candidate: float | int, metric: str) -> bool:
    divisor, absolute = (20, 1_048_576) if metric == "peak_rss_bytes" else (10, 100_000)
    delta = candidate - baseline
    # Integer ratios keep an exactly-equal percentage on the passing boundary.
    return delta > absolute and delta * divisor > baseline


def compare(baseline: dict[str, list[dict[str, object]]],
            candidate: dict[str, list[dict[str, object]]]) -> list[dict[str, object]]:
    comparisons = []
    for case in perf.GATE_CASES:
        absolute_failures = []
        for label, rows in (("baseline", baseline[case]), ("candidate", candidate[case])):
            absolute_failures.extend(
                f"{label}: {failure}"
                for failure in perf.gate_failures(rows, case, perf.GATE_TRIALS, perf.GATE_ITERATIONS)
            )
        for step in perf.CASES[case]:
            phases = dict.fromkeys(row["phase"] for row in baseline[case] if row["step"] == step)
            for phase in phases:
                before = [row for row in baseline[case]
                          if row["step"] == step and row["phase"] == phase]
                after = [row for row in candidate[case]
                         if row["step"] == step and row["phase"] == phase]
                for metric in METRICS:
                    b = perf.summarize([row[metric] for row in before])
                    c = perf.summarize([row[metric] for row in after])
                    failures = list(absolute_failures)
                    if median_regresses(b["median"], c["median"], metric):
                        failures.append("median regression")
                    comparisons.append({
                        "case": case, "step": step, "phase": phase, "samples": len(before),
                        "metric": metric,
                        "baseline_median": b["median"], "baseline_p99": b["p99"],
                        "baseline_worst": b["worst"],
                        "candidate_median": c["median"], "candidate_p99": c["p99"],
                        "candidate_worst": c["worst"],
                        "delta": c["median"] - b["median"],
                        "relative_percent": (c["median"] - b["median"]) * 100 / b["median"],
                        "threshold_percent": 5 if metric == "peak_rss_bytes" else 10,
                        "threshold_absolute": 1_048_576 if metric == "peak_rss_bytes" else 100_000,
                        "status": "FAIL" if failures else "PASS", "failures": "; ".join(failures),
                    })
    return comparisons


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if len({args.baseline.resolve(), args.candidate.resolve(), args.output.resolve()}) != 3:
            raise ValueError("baseline, candidate and output must be distinct paths")
        if args.output.exists() and any(args.output.samefile(path)
                                       for path in (args.baseline, args.candidate)):
            raise ValueError("output must not alias raw evidence")
        comparisons = compare(load_evidence(args.baseline), load_evidence(args.candidate))
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("w", encoding="utf-8", newline="") as output:
            writer = csv.DictWriter(output, fieldnames=OUTPUT_FIELDS, delimiter="\t",
                                    lineterminator="\n")
            writer.writeheader()
            writer.writerows(comparisons)
    except (OSError, ValueError) as error:
        print(f"Invalid interaction evidence: {error}", file=sys.stderr)
        return 2
    failures = [row for row in comparisons if row["status"] == "FAIL"]
    for row in failures:
        print(f"FAIL {row['case']}/{row['step']}/{row['phase']}/{row['metric']}: "
              f"{row['failures']}")
    print(f"{'FAIL' if failures else 'PASS'}: {len(comparisons)} metric comparisons; "
          f"{len(failures)} failed; full evidence in {args.output}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
