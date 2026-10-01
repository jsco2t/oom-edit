#!/usr/bin/env python3
"""Record and gate input-to-owned-frame timings through the public pane."""

from __future__ import annotations

import argparse
import json
import math
import statistics
import subprocess
from pathlib import Path

VERSION = "oom-edit-realistic-v1"
KIB = 1024
CASES = {
    "source-type": ("insert", "delete"),
    "source-type-mid": ("insert", "delete"),
    "source-structural": ("insert", "delete"),
    "source-structural-mid": ("insert", "delete"),
    "source-line": ("insert", "delete"),
    "source-line-mid": ("insert", "delete"),
    "source-frontmatter": ("insert", "delete"),
    "source-reference": ("insert", "delete"),
    "fence-language": ("plain", "highlighted"),
    "fence-language-mid": ("plain", "highlighted"),
    "fence-unknown": ("unknown", "highlighted"),
    "fence-unknown-mid": ("unknown", "highlighted"),
    "source-scroll": ("down", "up"),
    "source-scroll-mid": ("down", "up"),
    "rendered-scroll": ("down", "up"),
    "rendered-scroll-mid": ("down", "up"),
    "rendered-edit": ("delete", "undo"),
    "rendered-edit-mid": ("delete", "undo"),
    "exit-insert": ("escape",),
    "exit-insert-mid": ("escape",),
    "resize": ("narrow", "wide"),
    "reload": ("reload",),
    "save": ("save",),
}
GATE_CASES = (
    "source-type", "source-type-mid", "source-line", "source-line-mid",
    "rendered-edit", "rendered-edit-mid", "exit-insert", "exit-insert-mid",
    "source-scroll", "source-scroll-mid", "rendered-scroll", "rendered-scroll-mid",
)
LOCAL_LIMIT_NS = 50_000_000
SCROLL_LIMIT_NS = 25_000_000
FIRST_RENDER_LIMIT_NS = 500_000_000
SIZES = (144 * KIB, 1024 * KIB)
CASE_CLASSES = {"source-reference": "mixed-reference"}
FIXTURE_IDENTITIES = {
    ("mixed", 144 * KIB): "oom-edit-realistic-v1:mixed:147456:bde118422ca5e1ad",
    ("mixed", 1024 * KIB): "oom-edit-realistic-v1:mixed:1048576:a2669d94455fc82d",
    ("mixed-reference", 144 * KIB): "oom-edit-realistic-v1:mixed-reference:147456:46803c7e01139b23",
    ("mixed-reference", 1024 * KIB): "oom-edit-realistic-v1:mixed-reference:1048576:9e6d4c35623679b3",
}


def parse_sample(row: str, class_name: str, size: int, case: str) -> dict[str, object]:
    fields = row.split("\t")
    if len(fields) != 11 or fields[0] != "INTERACTION":
        raise ValueError(f"malformed interaction measurement: {row!r}")
    identity = fields[1].split(":")
    if (
        len(identity) != 4
        or identity[0] != VERSION
        or identity[1] != class_name
        or int(identity[2]) != size
        or len(identity[3]) != 16
        or fields[1] != FIXTURE_IDENTITIES.get((class_name, size))
    ):
        raise ValueError(f"wrong fixture identity: {fields[1]!r}")
    if fields[2] != case or fields[3] not in CASES[case]:
        raise ValueError(f"wrong interaction case or step: {row!r}")
    width, height = map(int, fields[4:6])
    expected_width = 99 if case == "resize" and fields[3] == "narrow" else 100
    if (width, height) != (expected_width, 41):
        raise ValueError(f"wrong interaction dimensions: {row!r}")
    expected_mode = "insert" if case.startswith(("source-", "fence-")) else "normal"
    if fields[6] != expected_mode:
        raise ValueError(f"wrong interaction mode: {row!r}")
    input_ns, render_ns, total_ns, peak_rss_bytes = map(int, fields[7:11])
    if (min(input_ns, render_ns, total_ns) <= 0
            or peak_rss_bytes <= 0
            or total_ns < input_ns + render_ns):
        raise ValueError(f"invalid interaction timing: {row!r}")
    return {
        "fixture": fields[1],
        "case": case,
        "step": fields[3],
        "width": width,
        "height": height,
        "mode": fields[6],
        "input_ns": input_ns,
        "render_ns": render_ns,
        "total_ns": total_ns,
        "peak_rss_bytes": peak_rss_bytes,
    }


def summarize(values: list[int]) -> dict[str, float | int]:
    if not values:
        raise ValueError("cannot summarize empty timings")
    ordered = sorted(values)
    return {
        "median": statistics.median(ordered),
        "p95": ordered[math.ceil(len(ordered) * 0.95) - 1],
        "p99": ordered[math.ceil(len(ordered) * 0.99) - 1],
        "worst": ordered[-1],
    }


def sample(
    binary: Path, class_name: str, size: int, case: str, iterations: int
) -> list[dict[str, object]]:
    command = [
        str(binary), "interaction", class_name, str(size), "100", "41", case,
        str(iterations),
    ]
    completed = subprocess.run(command, text=True, capture_output=True, timeout=180, check=False)
    if completed.returncode:
        raise RuntimeError(f"probe failed: {command!r}\n{completed.stdout}{completed.stderr}")
    rows = [parse_sample(line, class_name, size, case) for line in completed.stdout.splitlines()]
    if len(rows) != iterations * len(CASES[case]):
        raise ValueError(f"incomplete interaction samples: {command!r}")
    expected_steps = list(CASES[case]) * iterations
    if [row["step"] for row in rows] != expected_steps:
        raise ValueError(f"out-of-order or incomplete interaction steps: {command!r}")
    return rows


def verify_cycles(rows: list[dict[str, object]], case: str,
                  trials: int, iterations: int) -> None:
    if len(rows) != trials * iterations * len(CASES[case]):
        raise ValueError(f"wrong sample count for {case}")
    expected = [(trial, cycle, step)
                for trial in range(trials)
                for cycle in range(iterations)
                for step in CASES[case]]
    actual = [(row.get("trial"), row.get("cycle"), row.get("step")) for row in rows]
    if actual != expected or any(row.get("case") != case for row in rows):
        raise ValueError(f"wrong trial/cycle classification for {case}")


def gate_failures(rows: list[dict[str, object]], case: str,
                  trials: int, iterations: int) -> list[str]:
    verify_cycles(rows, case, trials, iterations)
    if trials < 5 or iterations < 21:
        raise ValueError("interaction gate requires five trials and 21 cycles")
    failures = []
    for step in CASES[case]:
        step_rows = [row for row in rows if row["step"] == step]
        if case.startswith("exit-insert"):
            cold = [row for row in step_rows if row["cycle"] == 0]
            warm = [row for row in step_rows if row["cycle"] > 0]
            if len(cold) != trials or len(warm) < 100:
                raise ValueError("missing first-ever or warmed return samples")
            if max(int(row["total_ns"]) for row in cold) >= FIRST_RENDER_LIMIT_NS:
                failures.append(f"{case} first rendered projection")
            warm_times = [int(row["total_ns"]) for row in warm]
            if summarize(warm_times)["p99"] >= LOCAL_LIMIT_NS or max(warm_times) >= LOCAL_LIMIT_NS:
                failures.append(f"{case} warmed return")
        else:
            times = [int(row["total_ns"]) for row in step_rows]
            limit = SCROLL_LIMIT_NS if case.endswith("scroll") or "scroll-mid" in case else LOCAL_LIMIT_NS
            if summarize(times)["p99"] >= limit:
                failures.append(f"{case}/{step}")
    return failures


def format_phases(rows: list[dict[str, object]]) -> str:
    parts = []
    for name in ("input_ns", "render_ns", "total_ns"):
        summary = summarize([int(row[name]) for row in rows])
        parts.append(
            f"{name.removesuffix('_ns')} median/p95/p99/worst "
            + "/".join(f"{float(summary[key]) / 1e6:.1f}"
                       for key in ("median", "p95", "p99", "worst"))
            + " ms"
        )
    parts.append(
        f"peak RSS {max(int(row['peak_rss_bytes']) for row in rows) / (1024 * 1024):.1f} MiB"
    )
    return "; ".join(parts)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--gate", action="store_true")
    parser.add_argument("--trials", type=int, default=5)
    parser.add_argument("--iterations", type=int, default=21)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--case", choices=CASES.keys(), action="append")
    parser.add_argument("--size", type=int, action="append")
    args = parser.parse_args()
    if args.trials < 1 or args.iterations < 1:
        parser.error("trials and iterations must be positive")
    if args.gate and (args.trials < 5 or args.iterations < 21 or args.case or args.size):
        parser.error("gate requires five trials, 21 cycles and the fixed case/size inventory")
    chosen_cases = list(GATE_CASES) if args.gate else args.case or list(CASES)
    chosen_sizes = (1024 * KIB,) if args.gate else args.size or list(SIZES)
    observations: list[dict[str, object]] = []
    failures: list[str] = []
    for size in chosen_sizes:
        for case in chosen_cases:
            class_name = CASE_CLASSES.get(case, "mixed")
            rows = []
            for trial in range(args.trials):
                run = sample(args.binary, class_name, size, case, args.iterations)
                for index, row in enumerate(run):
                    row["trial"] = trial
                    row["cycle"] = index // len(CASES[case])
                    row["phase"] = (
                        "first-render" if row["cycle"] == 0 else "warmed"
                    ) if case.startswith("exit-insert") else "ordinary"
                rows.extend(run)
            verify_cycles(rows, case, args.trials, args.iterations)
            observations.extend(rows)
            for step in CASES[case]:
                step_rows = [row for row in rows if row["step"] == step]
                groups = (
                    (("first-render", [row for row in step_rows if row["cycle"] == 0]),
                     ("warmed", [row for row in step_rows if row["cycle"] > 0]))
                    if case.startswith("exit-insert") else (("ordinary", step_rows),)
                )
                for phase, phase_rows in groups:
                    print(f"{class_name} {size // KIB} KiB {case}/{step}/{phase} "
                          f"n={len(phase_rows)}: {format_phases(phase_rows)}", flush=True)
            if args.gate:
                failures.extend(gate_failures(rows, case, args.trials, args.iterations))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text("\n".join(json.dumps(row, sort_keys=True)
                                         for row in observations) + "\n")
    if failures:
        print("FAIL: " + ", ".join(failures), flush=True)
        return 1
    if args.gate:
        print(f"PASS: {len(observations)} verified interaction samples", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
