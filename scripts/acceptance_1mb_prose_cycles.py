#!/usr/bin/env python3
"""Gate exact-note multi-range Markdown selection edits through EditorPane."""

import argparse
import gzip
import hashlib
import json
import math
from pathlib import Path
import statistics
import subprocess

from acceptance_1mb import FIXTURE_SHA256, fixture_identity, revision_identity
from acceptance_1mb_edit_cycles import parse_cycles


SCENARIOS = (
    ("select-delete-cycles", 600, "j"),
    ("select-change-cycles", 600, "j"),
    ("select-delete-cycles", 130, "j"),
    ("select-change-cycles", 130, "j"),
)
P99_LIMIT_NS = 50_000_000
WORST_LIMIT_NS = 100_000_000


def gate_failures(cycles: list[dict], count: int) -> list[str]:
    if count < 100 or len(cycles) != count:
        return ["fewer than 100 verified cycles"]
    if any(cycle["cycle"] != index for index, cycle in enumerate(cycles)):
        return ["missing or reordered cycle"]
    failures = []
    for label, field in (("edit", "total_ns"), ("undo", "undo_total_ns")):
        totals = sorted(cycle[field] for cycle in cycles)
        if any(total <= 0 for total in totals):
            failures.append(f"{label} has nonpositive timing")
        if totals[math.ceil(count * 0.99) - 1] >= P99_LIMIT_NS:
            failures.append(f"{label} p99 is at or above 50 ms")
        if totals[-1] >= WORST_LIMIT_NS:
            failures.append(f"{label} worst is at or above 100 ms")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--count", type=int, default=100)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--gate", action="store_true")
    args = parser.parse_args()
    if args.count <= 0 or (args.gate and args.count < 100):
        parser.error("gate requires at least 100 cycles per operation and location")

    fixture_hash = fixture_identity(args.fixture)
    original_lines = len(args.fixture.read_text(encoding="utf-8").splitlines())
    revision = revision_identity(args.source_root, "candidate")
    binary_sha = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    records = []
    failures = []
    for scenario in SCENARIOS:
        case, line, motion = scenario
        command = [str(args.binary), str(args.fixture), case, str(line), motion,
                   "flat", str(args.count)]
        result = subprocess.run(command, capture_output=True, text=True,
                                timeout=max(180, args.count * 5), check=False)
        if result.returncode:
            raise RuntimeError(f"probe failed: {command!r}\n{result.stdout}{result.stderr}")
        cycles = parse_cycles(result.stdout, scenario, args.count,
                              fixture_hash, original_lines)
        for cycle in cycles:
            records.append({"type": "prose_edit_cycle", "revision": revision,
                            "binary_sha256": binary_sha,
                            "fixture_sha256": FIXTURE_SHA256,
                            "case": case, "line": line, **cycle})
        for label, field in (("edit", "total_ns"), ("undo", "undo_total_ns")):
            totals = sorted(cycle[field] for cycle in cycles)
            p99 = totals[math.ceil(len(totals) * 0.99) - 1]
            median = statistics.median(totals)
            print(f"{case} line {line} {label}: n={len(cycles)} "
                  f"median={median / 1e6:.2f} p99={p99 / 1e6:.2f} "
                  f"worst={totals[-1] / 1e6:.2f} ms", flush=True)
        if args.gate:
            failures.extend(f"{case} line {line}: {reason}"
                            for reason in gate_failures(cycles, args.count))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        opener = gzip.open if args.output.suffix == ".gz" else open
        with opener(args.output, "wt", encoding="utf-8") as destination:
            for record in records:
                destination.write(json.dumps(record, sort_keys=True) + "\n")
    if failures:
        print("FAIL: " + ", ".join(failures), flush=True)
        return 1
    print(f"PASS: {len(records)} exact edit-and-undo cycles", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
