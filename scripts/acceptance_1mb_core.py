#!/usr/bin/env python3
"""Record core-only construction and Select-projection phases on the exact note."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import subprocess
from pathlib import Path

from acceptance_1mb import FIXTURE_SHA256, fixture_identity, revision_identity, summary

LINES = (600, 3000, 20000)
PHASES = ("source-init", "full-layout", "select-entry")


def parse_output(output: str, line: int) -> list[dict]:
    rows = [row.split("\t") for row in output.splitlines()]
    if len(rows) != 18:
        raise ValueError("core probe omitted phase or motion records")
    result = []
    for index, row in enumerate(rows):
        if row[:2] != ["CORE", str(line)]:
            raise ValueError("core probe used the wrong fixture location")
        if index < 3:
            if len(row) != 4 or row[2] != PHASES[index] or int(row[3]) <= 0:
                raise ValueError("core probe phase is missing or unordered")
            result.append({"phase": row[2], "duration_ns": int(row[3])})
        else:
            if (len(row) != 6 or row[2] != "motion" or int(row[3]) != index - 3
                    or min(int(row[4]), int(row[5])) <= 0):
                raise ValueError("core probe motion is missing or unordered")
            result.append({"phase": "motion", "index": index - 3,
                           "handler_ns": int(row[4]), "selection_ns": int(row[5])})
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--role", choices=("candidate", "main"), required=True)
    parser.add_argument("--trials", type=int, default=5)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.trials < 1:
        parser.error("trials must be positive")
    fixture_hash = fixture_identity(args.fixture)
    revision = revision_identity(args.source_root, args.role)
    binary_sha = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    all_rows = []
    for line in LINES:
        motions = []
        for trial in range(args.trials):
            result = subprocess.run(
                [str(args.binary), str(args.fixture), str(line)],
                text=True, capture_output=True, timeout=180, check=False,
            )
            if result.returncode:
                raise RuntimeError(f"core probe failed: {result.stdout}{result.stderr}")
            rows = parse_output(result.stdout, line)
            for row in rows:
                all_rows.append({"role": args.role, "revision": revision,
                                 "binary_sha256": binary_sha,
                                 "fixture_sha256": FIXTURE_SHA256,
                                 "fixture_hash": fixture_hash,
                                 "line": line, "trial": trial, **row})
            motions.extend(row for row in rows if row["phase"] == "motion")
        print(f"{args.role} line={line}: handler {summary([row['handler_ns'] for row in motions])}; "
              f"selection {summary([row['selection_ns'] for row in motions])}", flush=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        opener = gzip.open if args.output.suffix == ".gz" else open
        with opener(args.output, "wt", encoding="utf-8") as destination:
            for row in all_rows:
                destination.write(json.dumps(row, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
