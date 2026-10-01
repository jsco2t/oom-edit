#!/usr/bin/env python3
"""Assert that repeated public-pane use has a bounded current-RSS working set."""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess

from acceptance_1mb import FIXTURE_SHA256, fixture_identity, revision_identity


CHECKPOINTS = (100, 200, 300, 400)
MAX_POST_WARMUP_GROWTH_BYTES = 2 * 1024 * 1024
FIRST_EDIT_LIMIT_NS = 50_000_000
SCENARIOS = (
    ("rust-delete", 3000),
    ("rust-change", 3000),
    ("go-delete", 20000),
    ("go-change", 20000),
    ("rust-mode", 3000),
    ("go-mode", 20000),
    ("rust-reload", 3000),
    ("go-reload", 20000),
)


def parse_sample(output: str, fixture_hash: str, scenario: str, count: int,
                 line: int) -> dict[str, object]:
    rows = [row.split("\t") for row in output.splitlines()]
    is_edit = scenario.endswith(("-delete", "-change"))
    expected_cycles = tuple(cycle for cycle in CHECKPOINTS if cycle <= count)
    if len(rows) != 1 + int(is_edit) + len(expected_cycles):
        raise ValueError("missing or extra RSS-stability rows")
    if rows[0] != ["RSS-RUN", fixture_hash, scenario, str(count), str(line), "4100"]:
        raise ValueError("RSS-stability run identity, count or geometry differs")
    offset = 1
    first_edit_ns = None
    if is_edit:
        if rows[offset][:3] != ["FIRST-EDIT", fixture_hash, scenario] \
                or len(rows[offset]) != 4:
            raise ValueError("missing first-edit timing")
        first_edit_ns = int(rows[offset][3])
        if first_edit_ns <= 0:
            raise ValueError("invalid first-edit timing")
        offset += 1
    checkpoints = []
    for cycle, fields in zip(expected_cycles, rows[offset:]):
        if len(fields) != 8 or fields[:4] != ["RSS", fixture_hash, scenario,
                                              str(cycle)]:
            raise ValueError(f"wrong RSS checkpoint identity at cycle {cycle}")
        rss = int(fields[4])
        if rss <= 0 or fields[5:] != [fixture_hash, "normal", "4100"]:
            raise ValueError(f"wrong document, mode, frame or RSS at cycle {cycle}")
        checkpoints.append({"cycle": cycle, "current_rss_bytes": rss})
    return {"scenario": scenario, "first_edit_ns": first_edit_ns,
            "checkpoints": checkpoints}


def failures(sample: dict[str, object]) -> list[str]:
    errors = []
    checkpoints = sample["checkpoints"]
    if checkpoints:
        warmed = checkpoints[0]["current_rss_bytes"]
        for checkpoint in checkpoints[1:]:
            if checkpoint["current_rss_bytes"] > warmed + MAX_POST_WARMUP_GROWTH_BYTES:
                errors.append(f"cycle {checkpoint['cycle']} grew more than 2 MiB")
                break
    first_edit_ns = sample["first_edit_ns"]
    if first_edit_ns is not None and first_edit_ns >= FIRST_EDIT_LIMIT_NS:
        errors.append("first post-open edit reached 50 ms")
    return errors


def run_probe(binary: Path, fixture: Path, scenario: str, line: int,
              count: int) -> str:
    command = [str(binary), str(fixture), scenario, str(line), str(count)]
    completed = subprocess.run(command, capture_output=True, text=True,
                               timeout=max(180, count * 5), check=False)
    if completed.returncode:
        raise RuntimeError(
            f"probe failed: {command!r}\n{completed.stdout}{completed.stderr}"
        )
    return completed.stdout


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--count", type=int, default=400)
    parser.add_argument("--scenario", choices=[name for name, _ in SCENARIOS])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--gate", action="store_true")
    args = parser.parse_args()
    if args.count not in CHECKPOINTS or args.trials < (3 if args.gate else 1):
        parser.error("count must be 100/200/300/400; gate needs three trials")
    if args.gate and args.count != 400:
        parser.error("gate requires 400 cycles")
    if args.gate and args.scenario:
        parser.error("gate must cover every scenario")
    fixture_hash = fixture_identity(args.fixture)
    revision = revision_identity(args.source_root, "candidate")
    binary_sha = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    observations = []
    errors = []
    for scenario, line in SCENARIOS:
        if args.scenario and scenario != args.scenario:
            continue
        # Reload scenarios have no timing assertion; edit trials stay serial.
        if scenario.endswith("-reload") and args.trials > 1:
            print(f"running {scenario} trials concurrently: {args.trials}", flush=True)
            with ThreadPoolExecutor(max_workers=min(args.trials, 3)) as pool:
                outputs = list(pool.map(
                    lambda _: run_probe(args.binary, args.fixture, scenario, line,
                                        args.count),
                    range(args.trials),
                ))
        else:
            outputs = []
            for trial in range(args.trials):
                print(f"running {scenario} trial {trial + 1}/{args.trials}", flush=True)
                outputs.append(run_probe(args.binary, args.fixture, scenario, line,
                                         args.count))
        for trial, output in enumerate(outputs):
            sample = parse_sample(output, fixture_hash, scenario, args.count, line)
            record = {"type": "rss_stability", "revision": revision,
                      "binary_sha256": binary_sha, "fixture_sha256": FIXTURE_SHA256,
                      "trial": trial + 1, **sample}
            observations.append(record)
            observed = [row["current_rss_bytes"] for row in sample["checkpoints"]]
            print(f"{scenario} trial {trial + 1}: "
                  f"current RSS {[round(value / 1048576, 2) for value in observed]} MiB; "
                  f"first edit {sample['first_edit_ns']}", flush=True)
            if args.gate:
                errors.extend(f"{scenario} trial {trial + 1}: {error}"
                              for error in failures(sample))
                if errors:
                    break
        if errors:
            break
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text("\n".join(json.dumps(row, sort_keys=True)
                                         for row in observations) + "\n")
    if errors:
        print("FAIL: " + "; ".join(errors), flush=True)
        return 1
    print(f"PASS: {len(observations)} fresh-process RSS-stability runs", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
