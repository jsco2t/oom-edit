#!/usr/bin/env python3
"""Validate the embeddable editor's requirement-to-case inventory."""

from __future__ import annotations

import argparse
import ast
import csv
import re
import shlex
import subprocess
from pathlib import Path


FR_NUMBERS = (
    1, 2, 3, 4, 5, 6,
    10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
    20, 21, 22, 23, 24, 25, 26, 27, 28,
    30, 31, 32, 33, 34, 35, 36,
    40, 41, 42, 43,
    50, 51, 52, 53, 54, 55, 56, 57,
    60, 61, 62, 63, 64, 65, 66, 67,
    70, 71, 72, 73, 74, 75,
    80, 81, 82, 83, 84, 85,
    90, 91, 92, 93, 94,
    100, 101, 102,
    110, 111, 112, 113, 114, 115, 116, 117, 118,
)
EXPECTED_IDS = {f"FR-{number:03d}" for number in FR_NUMBERS} | {
    f"NFR-{number:03d}" for number in range(1, 11)
}
FIELDS = (
    "requirement",
    "task",
    "case",
    "fixture",
    "expected",
    "location",
    "target",
    "status",
)
EXCLUSIONS = {"FR-066"}
STATUSES = {"planned", "covered", "excluded"}


class CoverageError(ValueError):
    """The inventory is incomplete or contains an invalid case."""


def load_manifest(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as source:
        reader = csv.DictReader(source, delimiter="\t")
        if tuple(reader.fieldnames or ()) != FIELDS:
            raise CoverageError("coverage inventory has an incompatible header")
        return list(reader)


def validate_manifest(rows: list[dict[str, str]], *, final: bool = False) -> None:
    if not rows:
        raise CoverageError("coverage inventory is empty")
    observed = {row["requirement"] for row in rows}
    missing = EXPECTED_IDS - observed
    unknown = observed - EXPECTED_IDS
    if missing:
        raise CoverageError(f"missing requirements: {sorted(missing)}")
    if unknown:
        raise CoverageError(f"unknown requirements: {sorted(unknown)}")

    seen_cases: set[str] = set()
    for row in rows:
        requirement = row["requirement"]
        if any(not row.get(field, "").strip() for field in FIELDS):
            raise CoverageError(f"{requirement} has an empty inventory field")
        if row["status"] not in STATUSES:
            raise CoverageError(f"{requirement} has an invalid status: {row['status']}")
        if row["status"] == "excluded" and requirement not in EXCLUSIONS:
            raise CoverageError(f"{requirement} has an unjustified exclusion")
        if requirement in EXCLUSIONS and row["status"] != "excluded":
            raise CoverageError(f"{requirement} must record its DRD exclusion")
        if final and row["status"] == "planned":
            raise CoverageError(f"{requirement} still has planned cases")
        if row["status"] == "covered" and row["location"].startswith("planned:"):
            raise CoverageError(f"{requirement} covered case has no test location")
        if not row["case"].startswith(f"EDIT-{requirement}-"):
            raise CoverageError(f"{requirement} case name has the wrong prefix")
        if row["case"] in seen_cases:
            raise CoverageError(f"duplicate case: {row['case']}")
        seen_cases.add(row["case"])
        if not row["target"].startswith("make "):
            raise CoverageError(f"{requirement} has no make target")
        if not row["task"].isdigit() or len(row["task"]) != 3:
            raise CoverageError(f"{requirement} has an invalid task ID")


def validate_locations(rows, root, rust_tests=None):
    """Reject nonexistent, stubbed or disabled cases and unowned workflows."""
    root = root.resolve()
    makefile = (root / "Makefile").read_text(encoding="utf-8")
    for row in rows:
        if row["status"] != "covered":
            continue
        location = row["location"]
        parts = location.split("::")
        path = (root / parts[0]).resolve()
        if root not in path.parents or not path.is_file() or len(parts) < 2:
            raise CoverageError(f"{row['case']}: missing or external case location {location}")
        target = shlex.split(row["target"])
        if len(target) < 2 or target[0] != "make" or not re.fullmatch(r"[a-zA-Z0-9_-]+", target[1]):
            raise CoverageError(f"{row['case']}: invalid make target")
        recipe = re.search(rf"(?m)^{re.escape(target[1])}:[^\n]*\n((?:\t[^\n]*\n)*)", makefile)
        if recipe is None:
            raise CoverageError(f"{row['case']}: unknown make target {target[1]}")
        source = path.read_text(encoding="utf-8")
        if path.suffix == ".rs":
            name = parts[-1]
            declaration = re.search(rf"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+{re.escape(name)}\b", source)
            if declaration is None:
                raise CoverageError(f"{row['case']}: missing Rust function {name}")
            body = source[declaration.end():]
            if re.match(r"\s*\([^)]*\)\s*\{\s*(?:return\s*;\s*)?\}", body):
                raise CoverageError(f"{row['case']}: stubbed Rust case {name}")
            attributes = source[:declaration.start()].rsplit("}", 1)[-1]
            if re.search(r"#\[ignore\b", attributes):
                commands = [shlex.split(line.strip()) for line in recipe.group(1).splitlines()]
                if not any("--ignored" in command and "--exact" in command and any(arg == name or arg.endswith("::" + name) for arg in command) for command in commands):
                    raise CoverageError(f"{row['case']}: ignored case is not explicitly executed")
            if rust_tests is not None and "benches" not in path.parts:
                requested = "::".join(parts[1:])
                if not any(test == requested or test.endswith("::" + requested) for test in rust_tests):
                    raise CoverageError(f"{row['case']}: case is not in the compiled test inventory: {requested}")
        elif path.suffix == ".py":
            name = parts[-1].split(".")[-1]
            tree = ast.parse(source)
            definitions = [node for node in ast.walk(tree) if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == name]
            if len(definitions) != 1 or all(
                isinstance(node, (ast.Pass, ast.Return))
                or (isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant))
                for node in definitions[0].body
            ):
                raise CoverageError(f"{row['case']}: missing or stubbed Python case {name}")
            disabled = {"skip", "skipIf", "skipUnless", "expectedFailure"}
            for node in ast.walk(tree):
                if isinstance(node, ast.ImportFrom) and node.module == "unittest":
                    disabled.update(alias.asname or alias.name for alias in node.names if alias.name in disabled)
            for decorator in definitions[0].decorator_list:
                function = decorator.func if isinstance(decorator, ast.Call) else decorator
                label = function.attr if isinstance(function, ast.Attribute) else function.id if isinstance(function, ast.Name) else ""
                if label in disabled:
                    raise CoverageError(f"{row['case']}: disabled Python case {name}")
        else:
            raise CoverageError(f"{row['case']}: unsupported executable case type")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=Path("coverage/edit_drd.tsv"))
    parser.add_argument("--final", action="store_true")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        rows = load_manifest(args.manifest)
        validate_manifest(rows, final=args.final)
        if args.final:
            result = subprocess.run(["make", "--no-print-directory", "coverage-test-list"], cwd=args.root, check=True, text=True, capture_output=True)
            rust_tests = set(re.findall(r"(?m)^([A-Za-z0-9_:]+): test$", result.stdout))
            if not rust_tests:
                raise CoverageError("compiled Rust test inventory is empty")
            validate_locations(rows, args.root, rust_tests)
    except (CoverageError, OSError, SyntaxError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"drd-coverage: {error}\n")
    print(f"drd-coverage: {len(rows)} cases across {len(EXPECTED_IDS)} requirements")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
