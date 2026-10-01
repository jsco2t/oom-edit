#!/usr/bin/env python3
"""Record exact-fixture public-pane keyboard, selection and edit latency."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import math
import statistics
import subprocess
from pathlib import Path

FIXTURE_SHA256 = "1a7676c0af7b6c82480a6e0b60b56748b0cbd028ffef9808b6422d5dacb302e7"
FIXTURE_BYTES = 1_048_722
MAIN_REVISION = "0e26684733551691a49a2ff7111cec9404fdd2d3"
WIDTH = 100
HEIGHT = 41
NAVIGATION = (
    ("area-down-j", 500, "j"),
    ("area-up-k", 1500, "k"),
    ("rust-down-j", 3000, "j"),
    ("rust-up-k", 4000, "k"),
    ("go-down-arrow", 20000, "down"),
    ("go-up-arrow", 21000, "up"),
)
SELECTION = (
    ("prose", 600, "j"),
    ("rust", 3000, "j"),
    ("go", 20000, "down"),
)


def operation_gate_fails(scenario_name: str, edits: list[dict]) -> bool:
    if any(edit["removed_lines"] < 10 for edit in edits):
        return True
    # The approved edit-latency budget covers the two large code fences.
    return scenario_name.endswith(("-rust", "-go")) and any(
        edit["input_ns"] + edit["render_ns"] >= 50_000_000 for edit in edits
    )


def fnv64(data: bytes) -> str:
    value = 0xCBF29CE484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"


def fixture_identity(path: Path) -> str:
    data = path.read_bytes()
    if len(data) != FIXTURE_BYTES or hashlib.sha256(data).hexdigest() != FIXTURE_SHA256:
        raise ValueError("fixture bytes or SHA-256 differ from examples/kitchen-sink-1mb.md")
    return fnv64(data)


def revision_identity(root: Path, role: str) -> str:
    revision = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    if len(revision) != 40 or any(character not in "0123456789abcdef" for character in revision):
        raise ValueError("source revision is not a full Git SHA")
    if role == "main":
        if revision != MAIN_REVISION:
            raise ValueError("clean-main profile uses the wrong frozen revision")
        if subprocess.run(["git", "-C", str(root), "diff", "--quiet"],
                          check=False).returncode != 0:
            raise ValueError("clean-main baseline has tracked source changes")
    return revision


def parse_run(output: str, *, fixture_hash: str, case: str, line: int,
              motion: str, state: str, count: int) -> tuple[dict, list[dict], dict | None]:
    rows = [row.split("\t") for row in output.splitlines()]
    if not rows or len(rows[0]) != 11 or rows[0][0] != "RUN":
        raise ValueError("probe omitted or malformed RUN row")
    common = [fixture_hash, case, str(line), motion, state, str(count)]
    if rows[0][1:7] != common:
        raise ValueError("probe RUN identity does not match requested scenario")
    first_line, cold_ns, rss, peak_rss = map(int, rows[0][7:11])
    if first_line != line or min(cold_ns, rss, peak_rss) <= 0 or peak_rss < rss:
        raise ValueError("probe RUN metadata is inconsistent")
    run = {
        "fixture_hash": fixture_hash, "case": case, "start_line": line,
        "motion": motion, "state": state, "count": count,
        "width": WIDTH, "height": HEIGHT, "cold_ns": cold_ns,
        "rss_bytes": rss, "peak_rss_bytes": peak_rss,
    }
    keys = []
    previous_line = line
    previous_end = 0
    for index, fields in enumerate(rows[1:count + 1]):
        if len(fields) != 17 or fields[0] != "KEY" or fields[1:6] != common[:5]:
            raise ValueError(f"malformed KEY row {index}")
        if int(fields[6]) != index:
            raise ValueError("missing, duplicate or unordered key index")
        start_ns, input_ns, render_ns, end_ns, pause_ns, source_line, row, column = map(
            int, fields[7:15]
        )
        mode = fields[15]
        changed = fields[16]
        if (min(input_ns, render_ns) <= 0 or start_ns < previous_end
                or end_ns < start_ns + input_ns + render_ns
                or not 0 <= row < HEIGHT or not 0 <= column < WIDTH
                or mode != ("normal" if case == "nav" else "select")
                or changed != "true"):
            raise ValueError(f"invalid key state or timing at index {index}")
        if motion in ("j", "down") and source_line < previous_line:
            raise ValueError("downward key moved up")
        if motion in ("k", "up") and source_line > previous_line:
            raise ValueError("upward key moved down")
        if (index == count // 2 and count >= 100 and pause_ns < 300_000_000
                or index != count // 2 and pause_ns != 0):
            raise ValueError("stop/resume pause is missing or misplaced")
        keys.append({
            "index": index, "start_ns": start_ns, "input_ns": input_ns,
            "render_ns": render_ns, "end_ns": end_ns, "pause_ns": pause_ns,
            "source_line": source_line, "cursor_row": row,
            "cursor_column": column, "mode": mode, "frame_changed": changed == "true",
        })
        previous_line = source_line
        previous_end = end_ns
    if len(keys) != count:
        raise ValueError("probe omitted key samples")
    edit = None
    if case == "nav":
        if len(rows) != count + 1:
            raise ValueError("navigation probe emitted unexpected rows")
    else:
        if len(rows) != count + 2:
            raise ValueError("selection probe omitted edit result")
        fields = rows[-1]
        if len(fields) != 13 or fields[0] != "EDIT" or fields[1:7] != common:
            raise ValueError("malformed EDIT row")
        input_ns, render_ns, original_lines, changed_lines = map(int, fields[7:11])
        changed_hash, mode = fields[11:13]
        if (min(input_ns, render_ns, original_lines, changed_lines) <= 0
                or len(changed_hash) != 16 or changed_hash == fixture_hash
                or mode != ("normal" if case == "select-delete" else "insert")):
            raise ValueError("invalid selection operation result")
        edit = {
            "input_ns": input_ns, "render_ns": render_ns,
            "original_lines": original_lines, "changed_lines": changed_lines,
            "changed_hash": changed_hash, "mode": mode,
            "removed_lines": original_lines - changed_lines,
        }
    return run, keys, edit


def summary(values: list[int]) -> str:
    ordered = sorted(values)
    p99 = ordered[math.ceil(len(ordered) * 0.99) - 1]
    return (f"median={statistics.median(ordered) / 1e6:.2f} "
            f"p99={p99 / 1e6:.2f} worst={ordered[-1] / 1e6:.2f} ms")


def run_probe(binary: Path, fixture: Path, scenario: tuple, state: str,
              count: int, fixture_hash: str) -> tuple[dict, list[dict], dict | None]:
    name, line, motion = scenario
    case = "nav" if name.startswith(("area-", "rust-", "go-")) else name.rsplit("-", 1)[0]
    command = [str(binary), str(fixture), case, str(line), motion, state, str(count)]
    result = subprocess.run(command, text=True, capture_output=True, timeout=180, check=False)
    if result.returncode:
        raise RuntimeError(f"probe failed: {command!r}\n{result.stdout}{result.stderr}")
    run, keys, edit = parse_run(result.stdout, fixture_hash=fixture_hash,
                                case=case, line=line, motion=motion,
                                state=state, count=count)
    run["scenario"] = name
    return run, keys, edit


def scenarios() -> list[tuple[str, tuple, str, int]]:
    result = [("navigation", scenario, state, 1000)
              for state in ("flat", "retained") for scenario in NAVIGATION]
    result.extend(("selection", (f"select-{operation}-{name}", line, motion), "flat", 15)
                  for name, line, motion in SELECTION
                  for operation in ("delete", "change"))
    return result


def navigation_scenarios() -> list[tuple[str, tuple, str, int]]:
    return [scenario for scenario in scenarios() if scenario[0] == "navigation"]


def selection_motion_scenarios() -> list[tuple[str, tuple, str, int]]:
    return [
        ("selection", (f"select-{operation}-{name}", line, motion), state, 15)
        for state in ("flat", "retained")
        for name, line, motion in SELECTION
        for operation in ("delete", "change")
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--role", choices=("candidate", "main"), required=True)
    parser.add_argument("--trials", type=int, default=5)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--gate", action="store_true")
    parser.add_argument("--navigation-only", action="store_true")
    parser.add_argument("--select-motion-gate", action="store_true")
    parser.add_argument("--scenario", action="append")
    args = parser.parse_args()
    if args.trials < 1:
        parser.error("trials must be positive")
    if args.gate and (args.trials < 5 or args.scenario):
        parser.error("gate requires five trials and the fixed scenario inventory")
    if args.select_motion_gate and (
        args.gate or args.navigation_only or args.trials < 7 or args.scenario
    ):
        parser.error("Select motion gate requires seven trials and its fixed inventory")
    fixture_hash = fixture_identity(args.fixture)
    revision = revision_identity(args.source_root, args.role)
    binary_sha = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    chosen = (selection_motion_scenarios() if args.select_motion_gate
              else navigation_scenarios() if args.navigation_only else scenarios())
    if args.scenario:
        names = set(args.scenario)
        chosen = [scenario for scenario in chosen if scenario[1][0] in names]
        if {scenario[1][0] for scenario in chosen} != names:
            parser.error("unknown or duplicate scenario name")
    output_rows = []
    failures = []
    for category, scenario, state, count in chosen:
        trial_count = args.trials if category == "navigation" or args.select_motion_gate else 1
        samples = []
        edits = []
        for trial in range(trial_count):
            run, keys, edit = run_probe(args.binary, args.fixture, scenario,
                                        state, count, fixture_hash)
            for key in keys:
                output_rows.append({"type": "key", "role": args.role,
                                    "revision": revision, "binary_sha256": binary_sha,
                                    "fixture_sha256": FIXTURE_SHA256,
                                    "trial": trial, **run, **key})
            if edit:
                output_rows.append({"type": "edit", "role": args.role,
                                    "revision": revision, "binary_sha256": binary_sha,
                                    "fixture_sha256": FIXTURE_SHA256,
                                    "trial": trial, **run, **edit})
                edits.append(edit)
            samples.extend(key["input_ns"] + key["render_ns"] for key in keys)
        print(f"{args.role} {scenario[0]} {state}: n={len(samples)} {summary(samples)}",
              flush=True)
        for edit in edits:
            print(f"  edit input={edit['input_ns'] / 1e6:.2f} ms "
                  f"frame={edit['render_ns'] / 1e6:.2f} ms "
                  f"removed_lines={edit['removed_lines']}", flush=True)
        if args.gate or args.select_motion_gate:
            ordered = sorted(samples)
            p99 = ordered[math.ceil(len(ordered) * 0.99) - 1]
            limit = 25_000_000 if category == "navigation" else 50_000_000
            if p99 >= limit or (category == "navigation" and ordered[-1] >= 50_000_000):
                failures.append(f"{scenario[0]} {state}: latency")
            if args.gate and category == "selection" and operation_gate_fails(
                scenario[0], edits
            ):
                failures.append(f"{scenario[0]}: operation")
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        opener = gzip.open if args.output.suffix == ".gz" else open
        with opener(args.output, "wt", encoding="utf-8") as destination:
            for row in output_rows:
                destination.write(json.dumps(row, sort_keys=True) + "\n")
    if failures:
        print("FAIL: " + ", ".join(failures), flush=True)
        return 1
    print(f"PASS: {len(output_rows)} validated records", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
