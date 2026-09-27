#!/usr/bin/env python3
"""Regenerate the pinned Markdown block grammar offline."""

import argparse
import os
from pathlib import Path
import subprocess


def guard_generated_parser(source):
    if "#define LANGUAGE_VERSION 15\n" not in source:
        raise ValueError("generated parser must retain ABI 15")
    start = "#ifdef _MSC_VER\n#pragma optimize(\"\", off)"
    end = '#pragma GCC optimize ("O0")\n#endif'
    if source.count(start) != 1 or source.count(end) != 1:
        raise ValueError("unexpected generated optimization directives")
    source = source.replace(start, "#ifndef TREE_SITTER_MD_OPTIMIZED_BUILD\n" + start, 1)
    return source.replace(end, end + "\n#endif", 1)


def regenerate(generator, output, grammar):
    version = subprocess.check_output([generator, "--version"], text=True).strip()
    if version != "tree-sitter 0.26.3":
        raise ValueError("regeneration requires exactly tree-sitter 0.26.3")
    output = Path(output).resolve()
    headers = {
        path: path.read_bytes()
        for name in ("array.h", "alloc.h", "parser.h")
        if (path := output / "tree_sitter" / name).exists()
    }
    environment = {
        key: value for key, value in os.environ.items()
        if key not in {"NO_DEFAULT_EXTENSIONS", "ALL_EXTENSIONS"}
        and not key.startswith("EXTENSION_")
    }
    try:
        subprocess.run(
            [generator, "generate", "--abi", "15", "--output", str(output)],
            cwd=grammar,
            env=environment,
            check=True,
        )
    finally:
        for path, original in headers.items():
            path.write_bytes(original)
    generated = output / "parser.c"
    generated.write_text(guard_generated_parser(generated.read_text(encoding="utf-8")), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--generator", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    try:
        regenerate(args.generator, args.output, root / "patches/tree-sitter-md/tree-sitter-markdown")
    except ValueError as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
