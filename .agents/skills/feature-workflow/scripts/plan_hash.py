#!/usr/bin/env python3

import hashlib
from pathlib import Path
import subprocess


def repository_workflow_root() -> Path:
    root = Path(
        subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"],
            text=True,
        ).strip()
    )
    return root / ".ai" / "workflow"


def calculate_plan_hash(workflow: Path) -> str:
    files = [
        workflow / "request.md",
        workflow / "plan.md",
        workflow / "gate.json",
        *sorted((workflow / "tasks").glob("*.md")),
    ]

    digest = hashlib.sha256()
    for path in files:
        if not path.exists():
            raise FileNotFoundError(f"missing planning artifact: {path}")
        digest.update(path.relative_to(workflow).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def main() -> None:
    try:
        print(calculate_plan_hash(repository_workflow_root()))
    except FileNotFoundError as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
