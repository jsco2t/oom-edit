#!/usr/bin/env python3
"""Gate sustained complete-frame fence edits through the public editor pane."""

import argparse
import gzip
import hashlib
import json
import math
from pathlib import Path
import statistics
import subprocess

from acceptance_1mb import FIXTURE_SHA256, fixture_identity, revision_identity


SCENARIOS = (
    ("select-delete-cycles", 3000, "j"),
    ("select-change-cycles", 3000, "j"),
    ("select-delete-cycles", 20000, "down"),
    ("select-change-cycles", 20000, "down"),
)


def parse_cycles(output: str, expected: tuple, count: int, fixture_hash: str,
                 original_lines: int) -> list[dict]:
    case, line, motion = expected
    lines = output.splitlines()
    if len(lines) != count + 1:
        raise ValueError(f"expected {count} cycles plus header, got {len(lines)} lines")
    header = lines[0].split("\t")
    if header != ["RUN-CYCLES", fixture_hash, case, str(line), motion,
                  "flat", str(count), str(line)]:
        raise ValueError(f"cycle run identity or geometry differs: {header!r}")
    parsed = []
    hashes = set()
    for index, raw in enumerate(lines[1:]):
        fields = raw.split("\t")
        if len(fields) != 12 or fields[:5] != ["CYCLE", fixture_hash, case,
                                                str(line), str(index)]:
            raise ValueError(f"bad cycle identity at {index}: {fields!r}")
        input_ns, render_ns = int(fields[5]), int(fields[6])
        changed_hash, changed_lines, mode = fields[7], int(fields[8]), fields[9]
        undo_input_ns, undo_render_ns = int(fields[10]), int(fields[11])
        if (input_ns <= 0 or render_ns <= 0
                or undo_input_ns <= 0 or undo_render_ns <= 0
                or not 10 <= original_lines - changed_lines <= 20
                or mode != ("insert" if "change" in case else "normal")
                or len(changed_hash) != 16 or changed_hash == fixture_hash):
            raise ValueError(f"bad edited result at cycle {index}")
        hashes.add(changed_hash)
        parsed.append({"cycle": index, "input_ns": input_ns,
                       "render_ns": render_ns, "total_ns": input_ns + render_ns,
                       "undo_input_ns": undo_input_ns,
                       "undo_render_ns": undo_render_ns,
                       "undo_total_ns": undo_input_ns + undo_render_ns,
                       "changed_hash": changed_hash, "changed_lines": changed_lines,
                       "mode": mode})
    if len(hashes) != 1:
        raise ValueError("cycles produced different edited documents")
    return parsed


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
        parser.error("gate requires at least 100 cycles per operation and language")
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
            records.append({"type": "edit_cycle", "revision": revision,
                            "binary_sha256": binary_sha,
                            "fixture_sha256": FIXTURE_SHA256,
                            "case": case, "line": line, **cycle})
        ordered = sorted(cycle["total_ns"] for cycle in cycles)
        p99 = ordered[math.ceil(len(ordered) * 0.99) - 1]
        median = statistics.median(ordered)
        print(f"{case} line {line}: n={len(cycles)} median={median / 1e6:.2f} "
              f"p99={p99 / 1e6:.2f} worst={ordered[-1] / 1e6:.2f} ms",
              flush=True)
        if args.gate and p99 >= 50_000_000:
            failures.append(f"{case} line {line}: p99 >= 50 ms")
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
