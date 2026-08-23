#!/usr/bin/env python3

from pathlib import Path
import hashlib
import subprocess

root = Path(
    subprocess.check_output(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
    ).strip()
)

workflow = root / ".ai" / "workflow"

files = [
    workflow / "request.md",
    workflow / "plan.md",
    workflow / "gate.json",
    *sorted((workflow / "tasks").glob("*.md")),
]

digest = hashlib.sha256()

for path in files:
    if not path.exists():
        raise SystemExit(f"missing planning artifact: {path}")
    digest.update(path.relative_to(workflow).as_posix().encode())
    digest.update(b"\0")
    digest.update(path.read_bytes())
    digest.update(b"\0")

print(digest.hexdigest())
