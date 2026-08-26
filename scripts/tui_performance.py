#!/usr/bin/env python3
"""Make-owned TUI performance evidence recorder and comparator."""

from __future__ import annotations

import argparse
import csv
import json
import os
import platform
import re
import resource
import subprocess
import sys
import tempfile
from pathlib import Path

SCHEMA_VERSION = "1"
FIXTURE_VERSION = "oom-edit-tui-v2"
RENDERED_FIRST_FRAME_RSS_LIMIT = 192 * 1024 * 1024
BASELINE_CASES = (
    "source-render-empty",
    "rendered-render-empty",
    "source-first-frame",
    "rendered-first-frame",
    "edit-frame",
    "source-scroll-frame",
    "idle-quiescent",
)
CANDIDATE_ONLY_CASES = (
    "source-gutter-sparse",
    "rendered-gutter-sparse",
    "source-gutter-dense-5000",
    "source-gutter-dense-50000",
    "rendered-gutter-dense-5000",
    "rendered-gutter-dense-50000",
    "idle-gutter-project",
    "idle-gutter-cancel",
)
CASES = BASELINE_CASES + CANDIDATE_ONLY_CASES
MARKER_LINE_COUNTS = {
    "source-gutter-sparse": 4,
    "rendered-gutter-sparse": 4,
    "source-gutter-dense-5000": 5_000,
    "rendered-gutter-dense-5000": 5_000,
    "source-gutter-dense-50000": 50_000,
    "rendered-gutter-dense-50000": 50_000,
}
FIELDS = (
    "schema-version",
    "revision",
    "branch-role",
    "rustc",
    "os",
    "arch",
    "cpu-model",
    "logical-cpus",
    "memory-bytes",
    "fixture-version",
    "case",
    "trial",
    "iterations",
    "measured-ns",
    "wall-average-ns-per-frame",
    "wall-worst-ns-per-frame",
    "cpu-ns-per-frame",
    "peak-rss-bytes",
    "snapshot-heap-bytes",
    "status",
)
COMPARISON_FIELDS = (
    "schema-version",
    "case",
    "metric",
    "baseline-median",
    "baseline-worst",
    "candidate-median",
    "candidate-worst",
    "delta",
    "relative-basis-points",
    "threshold",
    "status",
)
COMPARABLE_METADATA = (
    "schema-version",
    "rustc",
    "os",
    "arch",
    "cpu-model",
    "logical-cpus",
    "memory-bytes",
    "fixture-version",
)
NONNEGATIVE_INTEGER_FIELDS = (
    "trial",
    "iterations",
    "measured-ns",
    "wall-average-ns-per-frame",
    "wall-worst-ns-per-frame",
    "cpu-ns-per-frame",
    "peak-rss-bytes",
    "snapshot-heap-bytes",
)


class EvidenceError(ValueError):
    pass


def _checked(command: list[str], cwd: Path) -> str:
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)
    if result.returncode != 0:
        raise EvidenceError(
            f"command failed ({' '.join(command)}):\n{result.stdout}{result.stderr}"
        )
    return result.stdout.strip()


def _machine_memory_bytes() -> int:
    if sys.platform == "darwin":
        result = subprocess.run(
            ["sysctl", "-n", "hw.memsize"], text=True, capture_output=True, check=False
        )
        if result.returncode == 0 and result.stdout.strip().isdigit():
            return int(result.stdout)
    pages = os.sysconf("SC_PHYS_PAGES")
    page_size = os.sysconf("SC_PAGE_SIZE")
    return int(pages * page_size)


def _cpu_model() -> str:
    if sys.platform == "darwin":
        result = subprocess.run(
            ["sysctl", "-n", "machdep.cpu.brand_string"],
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.exists():
        for line in cpuinfo.read_text(encoding="utf-8").splitlines():
            if line.lower().startswith("model name"):
                return line.split(":", 1)[1].strip()
    return platform.processor() or "unknown"


def machine_metadata(root: Path, role: str) -> dict[str, str]:
    return {
        "schema-version": SCHEMA_VERSION,
        "revision": _checked(["git", "rev-parse", "HEAD"], root),
        "branch-role": role,
        "rustc": _checked(["rustc", "--version"], root),
        "os": platform.system().lower(),
        "arch": platform.machine().lower(),
        "cpu-model": _cpu_model(),
        "logical-cpus": str(os.cpu_count() or 1),
        "memory-bytes": str(_machine_memory_bytes()),
        "fixture-version": FIXTURE_VERSION,
    }


def validate_value(value: object, field: str) -> str:
    text = str(value)
    if "\t" in text or "\n" in text or "\r" in text:
        raise EvidenceError(f"{field} contains a tab or newline")
    if not text:
        raise EvidenceError(f"{field} is empty")
    return text


def write_tsv(path: Path, fields: tuple[str, ...], rows: list[dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    normalized = [
        {field: validate_value(row[field], field) for field in fields} for row in rows
    ]
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", newline="", dir=path.parent, delete=False
    ) as temporary:
        writer = csv.DictWriter(temporary, fieldnames=fields, delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(normalized)
        temporary.flush()
        os.fsync(temporary.fileno())
        temporary_path = Path(temporary.name)
    os.replace(temporary_path, path)


def read_tsv(path: Path, fields: tuple[str, ...] = FIELDS) -> list[dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as source:
        reader = csv.DictReader(source, delimiter="\t")
        if tuple(reader.fieldnames or ()) != fields:
            raise EvidenceError(f"{path} has an incompatible TSV header")
        rows = list(reader)
    if not rows:
        raise EvidenceError(f"{path} has no evidence rows")
    for row in rows:
        for field in fields:
            validate_value(row[field], field)
    return rows


def _find_test_executable(root: Path) -> Path:
    command = [
        "cargo",
        "test",
        "--release",
        "-p",
        "oom-edit",
        "--lib",
        "perf_tests::tui_release_performance_case",
        "--no-run",
        "--offline",
        "--locked",
        "--message-format=json",
    ]
    result = subprocess.run(command, cwd=root, text=True, capture_output=True, check=False)
    if result.returncode != 0:
        raise EvidenceError(f"release test build failed:\n{result.stdout}{result.stderr}")
    executables: list[Path] = []
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        executable = message.get("executable")
        target = message.get("target", {})
        if executable and target.get("name") == "oom_edit":
            executables.append(Path(executable))
    if not executables:
        raise EvidenceError("cargo did not report the oom-edit test executable")
    return executables[-1]


def _parse_time(stderr: str) -> tuple[int, int]:
    if sys.platform == "darwin":
        timing = re.search(
            r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", stderr
        )
        rss = re.search(r"^\s*(\d+)\s+maximum resident set size", stderr, re.MULTILINE)
        if not timing or not rss:
            raise EvidenceError(f"could not parse macOS /usr/bin/time output:\n{stderr}")
        cpu_ns = int((float(timing.group(2)) + float(timing.group(3))) * 1_000_000_000)
        return cpu_ns, int(rss.group(1))
    user = re.search(r"User time \(seconds\):\s*([0-9.]+)", stderr)
    system = re.search(r"System time \(seconds\):\s*([0-9.]+)", stderr)
    rss = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", stderr)
    if not user or not system or not rss:
        raise EvidenceError(f"could not parse Linux /usr/bin/time output:\n{stderr}")
    cpu_ns = int((float(user.group(1)) + float(system.group(1))) * 1_000_000_000)
    return cpu_ns, int(rss.group(1)) * 1024


def _timed_case_process(executable: Path, case: str) -> dict[str, object]:
    time_args = ["-l"] if sys.platform == "darwin" else ["-v"]
    command = [
        "/usr/bin/time",
        *time_args,
        str(executable),
        "--exact",
        "perf_tests::tui_release_performance_case",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]
    env = os.environ.copy()
    env["OOM_TUI_PERF_CASE"] = case
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    result = subprocess.run(command, text=True, capture_output=True, env=env, check=False)
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    try:
        cpu_ns, peak_rss = _parse_time(result.stderr)
    except EvidenceError:
        cpu_seconds = (usage_after.ru_utime - usage_before.ru_utime) + (
            usage_after.ru_stime - usage_before.ru_stime
        )
        cpu_ns = int(cpu_seconds * 1_000_000_000)
        peak_rss = int(usage_after.ru_maxrss)
        if sys.platform != "darwin":
            peak_rss *= 1024
    return {
        "returncode": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "cpu_ns": cpu_ns,
        "peak_rss": peak_rss,
    }


def _run_case(executable: Path, case: str) -> dict[str, str]:
    helper = subprocess.run(
        [
            sys.executable,
            str(Path(__file__).resolve()),
            "_case-helper",
            "--executable",
            str(executable),
            "--case",
            case,
        ],
        text=True,
        capture_output=True,
        check=False,
    )
    if helper.returncode != 0:
        raise EvidenceError(f"{case} timing helper failed:\n{helper.stdout}{helper.stderr}")
    try:
        timed = json.loads(helper.stdout)
    except json.JSONDecodeError as error:
        raise EvidenceError(f"{case} timing helper emitted invalid JSON") from error
    stdout = str(timed["stdout"])
    stderr = str(timed["stderr"])
    returncode = int(timed["returncode"])
    cpu_ns = int(timed["cpu_ns"])
    peak_rss = int(timed["peak_rss"])
    if returncode != 0 and "test result: ok." not in stdout:
        raise EvidenceError(f"{case} failed:\n{stdout}{stderr}")
    metric_line = next(
        (
            line[line.index("OOM_TUI_METRIC\t") :]
            for line in stdout.splitlines()
            if "OOM_TUI_METRIC\t" in line
        ),
        None,
    )
    if metric_line is None:
        raise EvidenceError(f"{case} emitted no metric row:\n{stdout}")
    parts = metric_line.split("\t")
    if len(parts) != 9 or parts[1] != FIXTURE_VERSION or parts[2] != case:
        raise EvidenceError(f"{case} emitted an invalid metric row: {metric_line}")
    iterations = int(parts[3])
    if iterations <= 0:
        raise EvidenceError(f"{case} reported no iterations")
    status = apply_process_limits(case, parts[8], peak_rss)
    return {
        "case": case,
        "iterations": str(iterations),
        "measured-ns": parts[4],
        "wall-average-ns-per-frame": parts[5],
        "wall-worst-ns-per-frame": parts[6],
        "cpu-ns-per-frame": str(cpu_ns // iterations),
        "peak-rss-bytes": str(peak_rss),
        "snapshot-heap-bytes": parts[7],
        "status": status,
    }


def apply_process_limits(case: str, status: str, peak_rss: int) -> str:
    if (
        status == "pass"
        and case == "rendered-first-frame"
        and peak_rss > RENDERED_FIRST_FRAME_RSS_LIMIT
    ):
        return "fail-peak-rss-limit"
    return status


def record(root: Path, role: str, trials: int) -> list[dict[str, str]]:
    if role not in {"baseline", "candidate"}:
        raise EvidenceError("branch role must be baseline or candidate")
    if trials < 1:
        raise EvidenceError("trials must be positive")
    executable = _find_test_executable(root)
    metadata = machine_metadata(root, role)
    rows: list[dict[str, str]] = []
    for case in CASES:
        for trial in range(1, trials + 1):
            measured = _run_case(executable, case)
            row = dict(metadata)
            row.update(measured)
            row["trial"] = str(trial)
            rows.append(row)
            print(f"{role} {case} trial {trial}/{trials}: {measured['status']}", flush=True)
    return rows


def _median(values: list[int]) -> int:
    ordered = sorted(values)
    return ordered[len(ordered) // 2]


def validate_evidence(rows: list[dict[str, str]], role: str) -> None:
    metadata = {field: rows[0][field] for field in COMPARABLE_METADATA}
    seen: set[tuple[str, int]] = set()
    counts: dict[str, int] = {}
    trials_by_case: dict[str, set[int]] = {}
    for row in rows:
        if row["branch-role"] != role:
            raise EvidenceError(f"expected {role} evidence, found {row['branch-role']}")
        if any(row[field] != value for field, value in metadata.items()):
            raise EvidenceError(f"{role} evidence contains mixed machine metadata")
        if row["case"] not in CASES:
            raise EvidenceError(f"unknown performance case: {row['case']}")
        parsed: dict[str, int] = {}
        for field in NONNEGATIVE_INTEGER_FIELDS:
            try:
                parsed[field] = int(row[field])
            except ValueError as error:
                raise EvidenceError(f"{field} must be an integer") from error
            if parsed[field] < 0:
                raise EvidenceError(f"{field} must not be negative")
        trial = parsed["trial"]
        if trial == 0:
            raise EvidenceError("trial must be one-based")
        if parsed["iterations"] == 0:
            raise EvidenceError("iterations must be positive")
        if parsed["wall-worst-ns-per-frame"] < parsed["wall-average-ns-per-frame"]:
            raise EvidenceError("wall worst must not be below wall average")
        key = (row["case"], trial)
        if key in seen:
            raise EvidenceError(f"duplicate trial: {row['case']} {trial}")
        seen.add(key)
        counts[row["case"]] = counts.get(row["case"], 0) + 1
        trials_by_case.setdefault(row["case"], set()).add(trial)
        if row["status"] != "pass":
            raise EvidenceError(f"{role} {row['case']} trial {trial} did not pass")
    for case, count in counts.items():
        if count < 5:
            raise EvidenceError(f"{role} {case} has {count} trials; at least five are required")
        expected_trials = set(range(1, count + 1))
        if trials_by_case[case] != expected_trials:
            raise EvidenceError(f"{role} {case} trials must be consecutive and one-based")
    required_cases = BASELINE_CASES if role == "baseline" else CASES
    missing = set(required_cases) - counts.keys()
    if missing:
        raise EvidenceError(f"{role} evidence is missing cases: {sorted(missing)}")


def compare_records(
    baseline: list[dict[str, str]], candidate: list[dict[str, str]]
) -> list[dict[str, str]]:
    validate_evidence(baseline, "baseline")
    validate_evidence(candidate, "candidate")
    for field in COMPARABLE_METADATA:
        if baseline[0][field] != candidate[0][field]:
            raise EvidenceError(f"baseline/candidate mismatch for {field}")

    baseline_cases = {row["case"] for row in baseline}
    candidate_cases = {row["case"] for row in candidate}
    missing = baseline_cases - candidate_cases
    if missing:
        raise EvidenceError(f"candidate is missing baseline cases: {sorted(missing)}")

    rows: list[dict[str, str]] = []
    for case in sorted(baseline_cases):
        for metric, threshold in (
            ("wall-average-ns-per-frame", "10-percent-and-100000ns"),
            ("cpu-ns-per-frame", "10-percent-and-100000ns"),
            ("peak-rss-bytes", "5-percent-and-1048576bytes"),
            ("snapshot-heap-bytes", "5-percent-and-1048576bytes"),
        ):
            base_values = [int(row[metric]) for row in baseline if row["case"] == case]
            candidate_values = [
                int(row[metric]) for row in candidate if row["case"] == case
            ]
            base_median = _median(base_values)
            candidate_median = _median(candidate_values)
            delta = candidate_median - base_median
            basis_points = 0 if base_median == 0 else delta * 10_000 // base_median
            if metric in {"peak-rss-bytes", "snapshot-heap-bytes"}:
                failed = delta > 1_048_576 and candidate_median * 100 > base_median * 105
            else:
                failed = delta > 100_000 and candidate_median * 100 > base_median * 110
            rows.append(
                {
                    "schema-version": SCHEMA_VERSION,
                    "case": case,
                    "metric": metric,
                    "baseline-median": str(base_median),
                    "baseline-worst": str(max(base_values)),
                    "candidate-median": str(candidate_median),
                    "candidate-worst": str(max(candidate_values)),
                    "delta": str(delta),
                    "relative-basis-points": str(basis_points),
                    "threshold": threshold,
                    "status": "fail-regression" if failed else "pass",
                }
            )

    candidate_only = candidate_cases - baseline_cases
    for case in sorted(candidate_only):
        candidate_rows = [row for row in candidate if row["case"] == case]
        wall_values = [int(row["wall-worst-ns-per-frame"]) for row in candidate_rows]
        wall_limit = 1_000_000 if case.startswith("idle-") else 50_000_000
        rows.append(
            {
                "schema-version": SCHEMA_VERSION,
                "case": case,
                "metric": "wall-worst-ns-per-frame",
                "baseline-median": "0",
                "baseline-worst": "0",
                "candidate-median": str(_median(wall_values)),
                "candidate-worst": str(max(wall_values)),
                "delta": str(_median(wall_values)),
                "relative-basis-points": "0",
                "threshold": f"less-than-{wall_limit}ns",
                "status": "fail-absolute-limit"
                if max(wall_values) >= wall_limit
                else "pass",
            }
        )
        if case in MARKER_LINE_COUNTS:
            heap_values = [int(row["snapshot-heap-bytes"]) for row in candidate_rows]
            heap_limit = MARKER_LINE_COUNTS[case] * 24 + 4 * 1024
            rows.append(
                {
                    "schema-version": SCHEMA_VERSION,
                    "case": case,
                    "metric": "snapshot-heap-bytes",
                    "baseline-median": "0",
                    "baseline-worst": "0",
                    "candidate-median": str(_median(heap_values)),
                    "candidate-worst": str(max(heap_values)),
                    "delta": str(_median(heap_values)),
                    "relative-basis-points": "0",
                    "threshold": f"at-most-{heap_limit}bytes",
                    "status": "fail-snapshot-memory-limit"
                    if max(heap_values) > heap_limit
                    else "pass",
                }
            )

    for surface in ("source", "rendered"):
        smaller_case = f"{surface}-gutter-dense-5000"
        larger_case = f"{surface}-gutter-dense-50000"
        if not {smaller_case, larger_case}.issubset(candidate_only):
            continue
        for metric in ("wall-average-ns-per-frame", "cpu-ns-per-frame"):
            smaller_values = [
                int(row[metric]) for row in candidate if row["case"] == smaller_case
            ]
            larger_values = [
                int(row[metric]) for row in candidate if row["case"] == larger_case
            ]
            smaller_median = _median(smaller_values)
            larger_median = _median(larger_values)
            delta = larger_median - smaller_median
            basis_points = (
                0 if smaller_median == 0 else delta * 10_000 // smaller_median
            )
            rows.append(
                {
                    "schema-version": SCHEMA_VERSION,
                    "case": f"{surface}-gutter-offscreen-scaling",
                    "metric": metric,
                    "baseline-median": str(smaller_median),
                    "baseline-worst": str(max(smaller_values)),
                    "candidate-median": str(larger_median),
                    "candidate-worst": str(max(larger_values)),
                    "delta": str(delta),
                    "relative-basis-points": str(basis_points),
                    "threshold": "at-most-25-percent-median-growth",
                    "status": "fail-offscreen-scaling"
                    if larger_median * 100 > smaller_median * 125
                    else "pass",
                }
            )
    return rows


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    subparsers = parser.add_subparsers(dest="command", required=True)
    record_parser = subparsers.add_parser("record")
    record_parser.add_argument("--role", required=True)
    record_parser.add_argument("--output", type=Path, required=True)
    record_parser.add_argument("--trials", type=int, default=5)
    subparsers.add_parser("gate")
    helper_parser = subparsers.add_parser("_case-helper")
    helper_parser.add_argument("--executable", type=Path, required=True)
    helper_parser.add_argument("--case", choices=CASES, required=True)
    compare_parser = subparsers.add_parser("compare")
    compare_parser.add_argument("--baseline", type=Path, required=True)
    compare_parser.add_argument("--candidate", type=Path, required=True)
    compare_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "_case-helper":
            print(json.dumps(_timed_case_process(args.executable, args.case)))
        elif args.command == "record":
            rows = record(args.root, args.role, args.trials)
            write_tsv(args.output, FIELDS, rows)
            failures = [row for row in rows if row["status"] != "pass"]
            if failures:
                raise EvidenceError(
                    f"recorded {len(failures)} failing trial rows; evidence was retained"
                )
        elif args.command == "gate":
            rows = record(args.root, "candidate", 1)
            if any(row["status"] != "pass" for row in rows):
                raise EvidenceError("one or more release TUI cases failed")
        else:
            baseline = read_tsv(args.baseline)
            candidate = read_tsv(args.candidate)
            comparisons = compare_records(baseline, candidate)
            write_tsv(args.output, COMPARISON_FIELDS, comparisons)
            failures = [row for row in comparisons if row["status"] != "pass"]
            if failures:
                raise EvidenceError(f"{len(failures)} performance comparisons failed")
    except EvidenceError as error:
        print(f"tui-performance: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
