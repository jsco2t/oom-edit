#!/usr/bin/env python3
"""Cold-process public-pane measurements for realistic Markdown shapes."""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
from pathlib import Path

VERSION = "oom-edit-realistic-v1"
KIB = 1024
FIRST_FRAME_LIMIT_NS = 350_000_000
MIXED_FIRST_FRAME_LIMIT_NS = 500_000_000
TABLE_FIRST_FRAME_LIMIT_NS = 391_050_000  # Fixed pre-incremental worst plus 10%.
GATE_CASES = (
    ("prose", 1024 * KIB),
    ("mixed", 1024 * KIB),
    ("mixed", 144 * KIB),
    ("rust-fence", 128 * KIB),
    ("many-fences", 128 * KIB),
    ("lists", 512 * KIB),
    ("tables", 448 * KIB),
)
LAYOUT_CLASSES = ("prose", "mixed", "rust-fence", "many-fences", "lists", "tables")
LAYOUT_SIZES = (256 * KIB, 512 * KIB, 1024 * KIB)
VIEWPORTS = ((100, 41), (70, 28))
TRIGGERS = ("edit", "save", "width", "reload")

# Fixed fixture identities and same-machine pre-incremental memory maxima. These are
# acceptance baselines, not values learned from a candidate run.
FIXTURE_HASHES = {
    ("prose", 256 * KIB): "28068f0b91ea2c95",
    ("prose", 512 * KIB): "4b634baecb60c8d9",
    ("prose", 1024 * KIB): "d894d5030de45b3d",
    ("mixed", 144 * KIB): "bde118422ca5e1ad",
    ("mixed", 256 * KIB): "f167eab14e1c82d8",
    ("mixed", 512 * KIB): "7e01af49dbe046e7",
    ("mixed", 1024 * KIB): "a2669d94455fc82d",
    ("rust-fence", 128 * KIB): "d5ef4f045322fba0",
    ("rust-fence", 256 * KIB): "9e0fd6c0c23cc0a8",
    ("rust-fence", 512 * KIB): "9524f29ccca9eb78",
    ("rust-fence", 1024 * KIB): "c353ecdf47bd6843",
    ("many-fences", 128 * KIB): "a4215da162cb3947",
    ("many-fences", 256 * KIB): "b9a311802a129ac3",
    ("many-fences", 512 * KIB): "9af5d95514460e7f",
    ("many-fences", 1024 * KIB): "ad5676c1d0b1ba95",
    ("lists", 256 * KIB): "6424ce8f8a580d0e",
    ("lists", 512 * KIB): "33b1994e35a6496c",
    ("lists", 1024 * KIB): "4a3558adfb19ed31",
    ("tables", 256 * KIB): "c1d1f12c448fc3fb",
    ("tables", 448 * KIB): "c5cdec08a4cb33ab",
    ("tables", 512 * KIB): "f8e1ab4c80d1f03b",
    ("tables", 1024 * KIB): "277de37e2b8a30bb",
}
COLD_RSS_BASELINE = {
    ("prose", 1024 * KIB): (152_227_840, 147_705_856),
    ("mixed", 1024 * KIB): (191_496_192, 191_397_888),
    ("mixed", 144 * KIB): (39_882_752, 39_329_792),
    ("rust-fence", 128 * KIB): (33_419_264, 33_280_000),
    ("many-fences", 128 * KIB): (34_357_248, 33_718_272),
    ("lists", 512 * KIB): (125_939_712, 125_566_976),
    ("tables", 448 * KIB): (119_779_328, 119_361_536),
}
LAYOUT_MEMORY_BASELINE = {
    ("prose", 256 * KIB): (11_717_520, 42_135_552),
    ("prose", 512 * KIB): (23_434_792, 77_504_512),
    ("prose", 1024 * KIB): (46_869_472, 147_406_848),
    ("mixed", 256 * KIB): (13_214_392, 56_127_488),
    ("mixed", 512 * KIB): (26_658_407, 99_733_504),
    ("mixed", 1024 * KIB): (53_393_174, 188_264_448),
    ("rust-fence", 256 * KIB): (14_344_736, 51_515_392),
    ("rust-fence", 512 * KIB): (28_689_344, 94_531_584),
    ("rust-fence", 1024 * KIB): (57_378_968, 180_727_808),
    ("many-fences", 256 * KIB): (17_270_248, 50_335_744),
    ("many-fences", 512 * KIB): (34_545_984, 89_452_544),
    ("many-fences", 1024 * KIB): (69_105_992, 165_031_936),
    ("lists", 256 * KIB): (15_679_527, 64_393_216),
    ("lists", 512 * KIB): (31_451_869, 122_380_288),
    ("lists", 1024 * KIB): (62_991_346, 238_456_832),
    ("tables", 256 * KIB): (27_223_680, 69_197_824),
    ("tables", 512 * KIB): (54_455_616, 131_837_952),
    ("tables", 1024 * KIB): (108_919_488, 256_851_968),
}


def cold_limit_ns(class_name: str, size: int) -> int:
    if (class_name, size) == ("mixed", 1024 * KIB):
        return MIXED_FIRST_FRAME_LIMIT_NS
    if (class_name, size) == ("tables", 448 * KIB):
        return TABLE_FIRST_FRAME_LIMIT_NS
    return FIRST_FRAME_LIMIT_NS


def within_memory_budget(observed: int, baseline: int, class_name: str = "",
                         metric: str = "rss") -> bool:
    if class_name == "rust-fence" and metric == "rss":
        return observed >= 0 and observed * 20 <= baseline * 23
    return observed >= 0 and observed * 10 <= baseline * 11


def parse_sample(output: str, expected_kind: str, expected_bytes: int) -> dict[str, object]:
    lines = [line.split("\t") for line in output.strip().splitlines()]
    if not lines or lines[0][0] != expected_kind or len(lines) > (2 if expected_kind == "MEASURE" else 1):
        raise ValueError(f"missing {expected_kind} measurement: {output!r}")
    fields = lines[0]
    if len(fields) < 2:
        raise ValueError("missing fixture identity")
    identity = fields[1].split(":")
    if (len(identity) != 4 or identity[0] != VERSION
            or int(identity[2]) != expected_bytes or len(identity[3]) != 16
            or any(character not in "0123456789abcdef" for character in identity[3])):
        raise ValueError(f"wrong fixture identity: {fields[1]!r}")
    if expected_kind == "MEASURE":
        if len(fields) != 12:
            raise ValueError(f"wrong MEASURE field count: {fields!r}")
        dimensions = (int(fields[2]), int(fields[3]))
        construct, opened, rendered, copied, total, rss = map(int, fields[6:12])
        if min(construct, opened, rendered, copied, total, rss) <= 0:
            raise ValueError("measurement omitted work")
        if total < construct + opened + rendered + copied:
            raise ValueError("total is smaller than timed phases")
        result: dict[str, object] = {
            "kind": "first-frame",
            "fixture": fields[1],
            "width": dimensions[0],
            "height": dimensions[1],
            "mode": fields[4],
            "cursor": fields[5],
            "construct_ns": construct,
            "open_ns": opened,
            "render_ns": rendered,
            "copy_ns": copied,
            "total_ns": total,
            "peak_rss_bytes": rss,
        }
        if len(lines) > 1:
            trigger = lines[1]
            if len(trigger) != 4 or trigger[0] != "TRIGGER" or trigger[1] != fields[1]:
                raise ValueError(f"malformed trigger measurement: {trigger!r}")
            if trigger[2] not in TRIGGERS or int(trigger[3]) <= 0:
                raise ValueError(f"invalid trigger measurement: {trigger!r}")
            result["trigger"] = trigger[2]
            result["trigger_ns"] = int(trigger[3])
        return result
    if len(fields) != 7:
        raise ValueError(f"wrong LAYOUT field count: {fields!r}")
    elapsed, rows, heap, rss = map(int, fields[3:7])
    if min(elapsed, rows, heap, rss) <= 0:
        raise ValueError("layout measurement omitted work")
    return {
        "kind": "layout",
        "fixture": fields[1],
        "width": int(fields[2]),
        "layout_ns": elapsed,
        "rows": rows,
        "layout_heap_bytes": heap,
        "peak_rss_bytes": rss,
    }


def sample(binary: Path, class_name: str, size: int, width: int, height: int,
           mode: str, cursor: str = "start", trigger: str | None = None) -> dict[str, object]:
    command = [str(binary), class_name, str(size), str(width), str(height), mode, cursor]
    if trigger:
        command.append(trigger)
    completed = subprocess.run(command, text=True, capture_output=True, timeout=180, check=False)
    if completed.returncode:
        raise RuntimeError(f"probe failed: {command!r}\n{completed.stdout}{completed.stderr}")
    result = parse_sample(completed.stdout, "LAYOUT" if mode == "layout" else "MEASURE", size)
    expected_identity = f"{VERSION}:{class_name}:{size}:{FIXTURE_HASHES[(class_name, size)]}"
    if result["fixture"] != expected_identity:
        raise ValueError("probe returned a different fixture")
    if result["width"] != width:
        raise ValueError("probe returned the wrong width")
    if mode == "layout":
        if "trigger" in result:
            raise ValueError("layout sample included a trigger")
    elif (result["height"], result["mode"], result["cursor"]) != (height, mode, cursor):
        raise ValueError("probe returned the wrong pane or cursor state")
    if result.get("trigger") != trigger:
        raise ValueError("probe omitted or changed the requested trigger")
    return result


def growth_ok(smaller: int, larger: int) -> bool:
    return larger * 4 <= smaller * 9


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("record", "gate"))
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--trials", type=int, default=5)
    parser.add_argument("--skip-layout", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.trials < (5 if args.mode == "gate" else 1):
        parser.error("gate requires at least five trials; record requires at least one")
    if args.mode == "gate" and args.skip_layout:
        parser.error("gate cannot skip layout scaling")
    observations: list[dict[str, object]] = []
    failures: list[str] = []
    for class_name, size in GATE_CASES:
        for width, height in VIEWPORTS:
            runs = [sample(args.binary, class_name, size, width, height, "normal")
                    for _ in range(args.trials)]
            observations.extend(runs)
            worst = max(int(run["total_ns"]) for run in runs)
            limit = cold_limit_ns(class_name, size)
            rss = max(int(run["peak_rss_bytes"]) for run in runs)
            baseline_rss = COLD_RSS_BASELINE[(class_name, size)][0 if width == 100 else 1]
            print(f"{class_name} {size // KIB} KiB {width}x{height}: "
                  f"median {statistics.median(int(run['total_ns']) for run in runs) / 1e6:.1f} ms, "
                  f"worst {worst / 1e6:.1f} ms, limit <{limit / 1e6:.2f} ms; "
                  f"peak RSS {rss / (1024 * 1024):.1f} MiB", flush=True)
            if args.mode == "gate" and worst >= limit:
                failures.append(f"{class_name} {size // KIB} KiB {width}x{height} first frame")
            if args.mode == "gate" and not within_memory_budget(
                    rss, baseline_rss, class_name, "rss"):
                failures.append(f"{class_name} {size // KIB} KiB {width}x{height} peak RSS")
    for mode, cursor in (("source", "start"), ("normal", "last")):
        observations.append(sample(args.binary, "mixed", 144 * KIB, 100, 41, mode, cursor))
    for trigger in TRIGGERS:
        runs = [sample(args.binary, "mixed", 144 * KIB, 100, 41,
                       "normal", trigger=trigger)
                for _ in range(args.trials if args.mode == "gate" else 1)]
        observations.extend(runs)
        times = sorted(int(run["trigger_ns"]) for run in runs)
        print(f"trigger {trigger} 144 KiB: median {statistics.median(times) / 1e6:.1f} ms, "
              f"worst {times[-1] / 1e6:.1f} ms", flush=True)
    if not args.skip_layout:
        for class_name in LAYOUT_CLASSES:
            samples = {}
            for size in LAYOUT_SIZES:
                runs = [sample(args.binary, class_name, size, 96, 41, "layout")
                        for _ in range(args.trials)]
                observations.extend(runs)
                samples[size] = max(int(run["layout_ns"]) for run in runs)
                heap = max(int(run["layout_heap_bytes"]) for run in runs)
                rss = max(int(run["peak_rss_bytes"]) for run in runs)
                baseline_heap, baseline_rss = LAYOUT_MEMORY_BASELINE[(class_name, size)]
                print(f"layout {class_name} {size // KIB} KiB: "
                      f"worst {samples[size] / 1e6:.1f} ms, "
                      f"retained heap {heap / (1024 * 1024):.1f} MiB, "
                      f"peak RSS {rss / (1024 * 1024):.1f} MiB", flush=True)
                if args.mode == "gate" and not within_memory_budget(
                        heap, baseline_heap, class_name, "heap"):
                    failures.append(f"{class_name} {size // KIB} KiB retained heap")
                if args.mode == "gate" and not within_memory_budget(
                        rss, baseline_rss, class_name, "rss"):
                    failures.append(f"{class_name} {size // KIB} KiB layout peak RSS")
            for small, large in zip(LAYOUT_SIZES, LAYOUT_SIZES[1:]):
                if args.mode == "gate" and not growth_ok(samples[small], samples[large]):
                    failures.append(f"{class_name} layout {small // KIB}->{large // KIB} KiB")
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text("\n".join(json.dumps(row, sort_keys=True) for row in observations) + "\n")
    if failures:
        print("FAIL: " + ", ".join(failures))
        return 1
    print(f"PASS: {len(observations)} process measurements")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
