#!/usr/bin/env python3
"""Compare exact-note pane and PTY raw records from candidate and clean main."""

from __future__ import annotations

import argparse
import gzip
import json
import math
from collections import defaultdict
from pathlib import Path

from acceptance_1mb import FIXTURE_SHA256, MAIN_REVISION
from acceptance_1mb_pty import REGIONS


def load(path: Path, role: str) -> list[dict]:
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rt", encoding="utf-8") as source:
        rows = [json.loads(line) for line in source]
    if not rows:
        raise ValueError(f"{path} has no samples")
    if any(row["role"] != role or row["fixture_sha256"] != FIXTURE_SHA256
           or len(row["binary_sha256"]) != 64 for row in rows):
        raise ValueError(f"{path} mixes roles, fixtures or binary identities")
    revisions = {row["revision"] for row in rows}
    binaries = {row["binary_sha256"] for row in rows}
    if len(revisions) != 1 or len(binaries) != 1:
        raise ValueError(f"{path} mixes source revisions or binaries")
    if role == "main" and revisions != {MAIN_REVISION}:
        raise ValueError("baseline is not frozen clean-main revision")
    return rows


def percentile(values: list[int], percentile_value: float) -> int:
    if not values:
        raise ValueError("empty timing group")
    ordered = sorted(values)
    return ordered[math.ceil(len(ordered) * percentile_value) - 1]


def pane_groups(rows: list[dict]) -> dict[tuple[str, str], dict]:
    groups: dict[tuple[str, str], dict] = defaultdict(
        lambda: {"keys": [], "edits": [], "trials": set(), "rss": 0}
    )
    for row in rows:
        group = groups[(row["scenario"], row["state"])]
        group["trials"].add(row["trial"])
        group["rss"] = max(group["rss"], row["peak_rss_bytes"])
        if row["type"] == "key":
            group["keys"].append(row)
        elif row["type"] == "edit":
            group["edits"].append(row)
        else:
            raise ValueError("unknown pane record type")
    for (scenario, _), group in groups.items():
        expected = 5000 if not scenario.startswith("select-") else 15
        if len(group["keys"]) != expected or group["trials"] != set(
            range(5 if expected == 5000 else 1)
        ):
            raise ValueError(f"{scenario} has incomplete trials or keys")
        if len(group["edits"]) != (0 if expected == 5000 else 1):
            raise ValueError(f"{scenario} has incomplete edit results")
    return groups


def pty_groups(rows: list[dict]) -> dict[str, dict]:
    groups: dict[str, dict] = defaultdict(
        lambda: {"serial": [], "burst": [], "burst_keys": [], "trials": set()}
    )
    for row in rows:
        group = groups[row["region"]]
        group["trials"].add(row["trial"])
        if row["type"] == "serial":
            group["serial"].append(row)
        elif row["type"] == "burst-frame":
            group["burst"].append(row)
        elif row["type"] == "burst-key":
            group["burst_keys"].append(row)
        else:
            raise ValueError("unknown PTY record type")
    if set(groups) != {region for region, _, _ in REGIONS}:
        raise ValueError("PTY trace omitted a direction or region")
    for region, group in groups.items():
        if (len(group["serial"]) != 5000 or len(group["burst_keys"]) != 5000
                or group["trials"] != set(range(5))):
            raise ValueError(f"{region} has incomplete PTY trials or keys")
        if not group["burst"]:
            raise ValueError(f"{region} has no terminal output frames")
    return groups


def compare_pane(main: list[dict], candidate: list[dict]) -> None:
    baseline = pane_groups(main)
    current = pane_groups(candidate)
    if baseline.keys() != current.keys():
        raise ValueError("pane scenario inventories differ")
    for scenario, state in sorted(current):
        left = baseline[(scenario, state)]
        right = current[(scenario, state)]
        def timing(group: dict, field: str) -> list[int]:
            return [int(row[field]) for row in group["keys"]]
        main_input = percentile(timing(left, "input_ns"), 0.99) / 1e6
        candidate_input = percentile(timing(right, "input_ns"), 0.99) / 1e6
        main_frame = percentile(timing(left, "render_ns"), 0.99) / 1e6
        candidate_frame = percentile(timing(right, "render_ns"), 0.99) / 1e6
        print(f"pane {scenario}/{state}: input p99 main={main_input:.2f} "
              f"candidate={candidate_input:.2f} ms; frame p99 "
              f"main={main_frame:.2f} candidate={candidate_frame:.2f} ms; "
              f"peak RSS main={left['rss'] / 1048576:.1f} "
              f"candidate={right['rss'] / 1048576:.1f} MiB")
        if right["edits"]:
            before, after = left["edits"][0], right["edits"][0]
            print(f"  edit input+frame main={(before['input_ns'] + before['render_ns']) / 1e6:.1f} "
                  f"candidate={(after['input_ns'] + after['render_ns']) / 1e6:.1f} ms; "
                  f"removed lines main={before['removed_lines']} "
                  f"candidate={after['removed_lines']}")


def compare_pty(main: list[dict], candidate: list[dict]) -> None:
    baseline = pty_groups(main)
    current = pty_groups(candidate)
    if baseline.keys() != current.keys():
        raise ValueError("PTY region inventories differ")
    for region in sorted(current):
        left = baseline[region]
        right = current[region]
        main_p99 = percentile([row["latency_ns"] for row in left["serial"]], 0.99)
        candidate_p99 = percentile([row["latency_ns"] for row in right["serial"]], 0.99)
        def active_gaps(group: dict) -> list[int]:
            return [row["gap_ns"] for row in group["burst"]
                    if row["keys_during_gap"] > 0 and not row["scheduled_pause"]]
        main_gap = max(active_gaps(left)) / 1e6
        candidate_gap = max(active_gaps(right)) / 1e6
        main_bytes = sum(row["output_bytes"] for row in left["serial"])
        candidate_bytes = sum(row["output_bytes"] for row in right["serial"])
        print(f"PTY {region}: serial flush p99 main={main_p99 / 1e6:.2f} "
              f"candidate={candidate_p99 / 1e6:.2f} ms; active burst gap worst "
              f"main={main_gap:.2f} candidate={candidate_gap:.2f} ms; "
              f"serial output main={main_bytes} candidate={candidate_bytes} bytes")


def compare_core(main: list[dict], candidate: list[dict]) -> None:
    for line in (600, 3000, 20000):
        for phase in ("source-init", "full-layout", "select-entry", "motion"):
            def phase_values(rows: list[dict], field: str) -> list[int]:
                return [row[field] for row in rows
                        if row["line"] == line and row["phase"] == phase]
            fields = ("handler_ns", "selection_ns") if phase == "motion" else ("duration_ns",)
            for field in fields:
                before = phase_values(main, field)
                after = phase_values(candidate, field)
                expected = 75 if phase == "motion" else 5
                if len(before) != expected or len(after) != expected:
                    raise ValueError(f"core {line}/{phase}/{field} has incomplete samples")
                print(f"core line={line} {phase}/{field}: p99 "
                      f"main={percentile(before, 0.99) / 1e6:.2f} "
                      f"candidate={percentile(after, 0.99) / 1e6:.2f} ms")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("pane-main", "pane-candidate", "pty-main", "pty-candidate",
                 "core-main", "core-candidate"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    compare_pane(load(args.pane_main, "main"), load(args.pane_candidate, "candidate"))
    compare_pty(load(args.pty_main, "main"), load(args.pty_candidate, "candidate"))
    compare_core(load(args.core_main, "main"), load(args.core_candidate, "candidate"))


if __name__ == "__main__":
    main()
