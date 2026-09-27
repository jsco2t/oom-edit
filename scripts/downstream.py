"""Independent candidate consumption: explicit seeding, then offline/locked checks."""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORIGIN = "https://github.com/jsco2t/oom-edit"
PACKAGES = ("oom-edit", "hjkl-buffer", "hjkl-engine", "tree-sitter-md", "dirs-sys")


def checked_revision(revision):
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("revision must be a full immutable 40-character commit SHA")
    return revision


def checked_tag(tag):
    if tag and not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
        raise ValueError("tag must be an explicit version tag")
    return tag


def assert_provenance(packages, revision, tag=""):
    checked_revision(revision)
    checked_tag(tag)
    selector = f"tag={tag}" if tag else f"rev={revision}"
    expected = f"git+{ORIGIN}?{selector}#{revision}"
    for name in PACKAGES:
        rows = [row for row in packages if row["name"] == name]
        if len(rows) != 1 or rows[0].get("source") != expected:
            raise ValueError(f"{name}: expected exactly one canonical candidate Git source")


def outside_workspace(path):
    path = Path(path).resolve()
    if path == ROOT or ROOT in path.parents:
        raise ValueError("consumer/snapshot directory must be outside the workspace")
    if path == Path(path.anchor):
        raise ValueError("a filesystem root is not a fixture directory")
    return path


def empty_directory(path):
    path = outside_workspace(path)
    if path.exists() and (not path.is_dir() or any(path.iterdir())):
        raise ValueError("output directory must be absent or empty; nothing is overwritten")
    path.mkdir(parents=True, exist_ok=True)
    return path


def run(arguments, directory, environment=None, capture=False):
    return subprocess.run(arguments, cwd=directory, env=environment, check=True,
                          text=True, stdout=subprocess.PIPE if capture else None)


def isolated_environment(cargo_home, target):
    environment = os.environ.copy()
    for name in list(environment):
        if name.startswith(("CARGO_", "GIT_CONFIG_")):
            del environment[name]
    environment.update(CARGO_HOME=str(cargo_home), CARGO_TARGET_DIR=str(target),
                       GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
    return environment


def snapshot(directory):
    directory = empty_directory(directory)
    # Git is read-only in the actual workspace; this commit lives in the fixture.
    names = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        cwd=ROOT).decode().split("\0")
    for name in names:
        if not name or name.split("/")[0] in {
            "vendor", "target", ".ai", ".agents", ".codex", ".agent-context.local.md"
        }:
            continue
        source = ROOT / name
        if source.is_symlink():
            raise ValueError(f"snapshot refuses symlinks: {name}")
        if not source.is_file():
            continue
        destination = directory / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
    environment = isolated_environment(directory / ".cargo-home-unused", directory / "target")
    run(["git", "init", "--quiet"], directory, environment)
    run(["git", "add", "--all"], directory, environment)
    run(["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false",
         "-c", "user.name=Fixture Builder", "-c", "user.email=fixture@example.invalid",
         "commit", "--quiet", "-m", "Temporary immutable candidate snapshot"],
        directory, environment)
    revision = run(["git", "rev-parse", "HEAD"], directory, environment, True).stdout.strip()
    print(json.dumps({"revision": revision, "source": str(directory),
                      "kind": "temporary-source-snapshot", "release_candidate": False}))


def manifest(revision, tag=""):
    checked_revision(revision)
    checked_tag(tag)
    git = json.dumps(ORIGIN)
    selector = f"tag = {json.dumps(tag)}" if tag else f"rev = {json.dumps(revision)}"
    patches = "\n".join(f"{name} = {{ git = {git}, {selector} }}" for name in PACKAGES[1:])
    return f"""[package]
name = "oom-edit-independent-consumer"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
oom-edit = {{ git = {git}, {selector} }}

[patch.crates-io]
{patches}
"""


def normalize_generated_checksums(vendor):
    # Vendoring from a directory source lists its old checksum metadata as a
    # source file, then rewrites it. That self-reference cannot remain valid.
    # Keep every real source-file hash and the registry package hash intact.
    for path in vendor.glob("*/.cargo-checksum.json"):
        checksums = json.loads(path.read_text())
        if ".cargo-checksum.json" in checksums["files"]:
            del checksums["files"][".cargo-checksum.json"]
            path.write_text(json.dumps(checksums), encoding="utf-8")


def prepare(directory, revision, source, registry_vendor, tag=""):
    revision = checked_revision(revision)
    tag = checked_tag(tag)
    if source:
        source = Path(source).resolve()
        # Require an existing immutable commit, never consume a mutable path dep.
        run(["git", "cat-file", "-e", f"{revision}^{{commit}}"], source)
    directory = empty_directory(directory)
    (directory / "src").mkdir()
    (directory / "Cargo.toml").write_text(manifest(revision, tag), encoding="utf-8")
    shutil.copy2(ROOT / "fixtures/downstream/src/lib.rs", directory / "src/lib.rs")
    shutil.copy2(ROOT / "fixtures/downstream/deny.toml", directory / "deny.toml")
    identity = {"origin": ORIGIN, "revision": revision}
    if tag:
        identity["tag"] = tag
    (directory / "candidate.json").write_text(json.dumps(identity, indent=2) + "\n", encoding="utf-8")
    # This is the sole preparation phase allowed to fetch the pinned Git source.
    # Registry seeding uses the audited repository vendor tree, not its lockfile.
    with tempfile.TemporaryDirectory(prefix="oom-edit-downstream-seed-") as temporary:
        cargo_home = Path(temporary)
        (cargo_home / "config.toml").write_text(
            '[source.crates-io]\nreplace-with = "seed"\n[source.seed]\n'
            f'directory = {json.dumps(str(Path(registry_vendor).resolve()))}\n'
            '[net]\ngit-fetch-with-cli = true\n', encoding="utf-8")
        environment = isolated_environment(cargo_home, directory / "target")
        environment["CARGO_NET_OFFLINE"] = "false"
        if source:
            environment.update(GIT_CONFIG_COUNT="1",
                               GIT_CONFIG_KEY_0=f"url.{source.as_uri()}.insteadOf",
                               GIT_CONFIG_VALUE_0=ORIGIN)
        run(["cargo", "generate-lockfile"], directory, environment)
        generated = run(["cargo", "vendor", "--offline", "--locked",
                         "--respect-source-config", "--versioned-dirs",
                         str(directory / "vendor")], directory, environment, True).stdout
    # Cargo's complete Git/registry replacement tables become fixture-owned.
    generated = generated.replace(json.dumps(str(directory / "vendor")), '"vendor"')
    normalize_generated_checksums(directory / "vendor")
    configuration = directory / ".cargo"
    configuration.mkdir()
    (configuration / "config.toml").write_text(generated, encoding="utf-8")
    assert_provenance(tomllib.loads((directory / "Cargo.lock").read_text())["package"], revision, tag)
    print(f"Prepared independent consumer at {directory}; revision {revision}")


def check(directory, revision, tag=""):
    directory = outside_workspace(directory)
    revision = checked_revision(revision)
    tag = checked_tag(tag)
    expected = json.loads((directory / "candidate.json").read_text())
    identity = {"origin": ORIGIN, "revision": revision}
    if tag:
        identity["tag"] = tag
    if expected != identity:
        raise ValueError("candidate manifest does not match the requested revision")
    parsed_manifest = tomllib.loads((directory / "Cargo.toml").read_text())
    required = {"git": ORIGIN, "tag": tag} if tag else {"git": ORIGIN, "rev": revision}
    if parsed_manifest["dependencies"].get("oom-edit") != required:
        raise ValueError("oom-edit: manifest source mismatch")
    for name in PACKAGES[1:]:
        if parsed_manifest.get("patch", {}).get("crates-io", {}).get(name) != required:
            raise ValueError(f"{name}: manifest patch missing or mismatched")
    configuration = tomllib.loads((directory / ".cargo/config.toml").read_text())
    replacements = configuration["source"]
    if not any(row.get("directory") == "vendor" for row in replacements.values()):
        raise ValueError("consumer must own a relative vendor tree")
    if any("directory" in row and row["directory"] != "vendor" for row in replacements.values()):
        raise ValueError("consumer may not inherit external source directories")
    assert_provenance(tomllib.loads((directory / "Cargo.lock").read_text())["package"], revision, tag)
    # New empty Cargo home each time: no inherited caches/config/patches.
    with tempfile.TemporaryDirectory(prefix="oom-edit-downstream-check-") as temporary:
        environment = isolated_environment(Path(temporary), directory / "target")
        environment["CARGO_NET_OFFLINE"] = "true"
        metadata = json.loads(run(["cargo", "metadata", "--offline", "--locked",
                                   "--format-version", "1"], directory, environment, True).stdout)
        if Path(metadata["workspace_root"]).resolve() != directory:
            raise ValueError("consumer inherited another workspace")
        for package in metadata["packages"]:
            if package["name"] == "oom-edit-independent-consumer":
                continue
            manifest_path = Path(package["manifest_path"]).resolve()
            if directory / "vendor" not in manifest_path.parents:
                raise ValueError(f"{package['name']}: resolved outside the consumer vendor tree")
        assert_provenance(metadata["packages"], revision, tag)
        run(["cargo", "clippy", "--offline", "--locked", "--all-targets", "--", "-D", "warnings"], directory, environment)
        run(["cargo", "test", "--offline", "--locked"], directory, environment)
    print(f"PASS independent offline/locked consumer and all five sources: {revision}")


def negative_check(directory, revision):
    directory = outside_workspace(directory)
    revision = checked_revision(revision)
    with tempfile.TemporaryDirectory(prefix="oom-edit-downstream-negative-") as temporary:
        scratch = Path(temporary)
        environment = isolated_environment(scratch / "cargo-home", scratch / "target")
        environment["CARGO_NET_OFFLINE"] = "true"
        metadata = json.loads(run(["cargo", "metadata", "--offline", "--locked",
                                   "--format-version", "1"], directory, environment, True).stdout)
        assert_provenance(metadata["packages"], revision)
        # Mutate the actual resolved graph, not a compilation-only surrogate.
        for name in PACKAGES:
            for bad_source in ["missing", None, "registry+https://github.com/rust-lang/crates.io-index",
                               f"git+{ORIGIN}?rev={'0' * 40}#{'0' * 40}"]:
                broken = [dict(row) for row in metadata["packages"]]
                if bad_source == "missing":
                    broken = [row for row in broken if row["name"] != name]
                else:
                    next(row for row in broken if row["name"] == name)["source"] = bad_source
                try:
                    assert_provenance(broken, revision)
                except ValueError as error:
                    if name not in str(error):
                        raise
                else:
                    raise ValueError(f"{name}: provenance negative oracle accepted a bad source")
        replica = scratch / "consumer"
        shutil.copytree(directory, replica,
                        ignore=lambda path, names: ["target"] if Path(path) == directory else [])
        crossterm = next(row for row in metadata["packages"] if row["name"] == "crossterm")
        original = Path(crossterm["manifest_path"]).parent / "src/lib.rs"
        corrupted = replica / original.relative_to(directory)
        with corrupted.open("a", encoding="utf-8") as output:
            output.write("\n// Deliberate corruption in an isolated negative fixture.\n")
        result = subprocess.run(["cargo", "check", "--package", "crossterm", "--offline", "--locked"],
                                cwd=replica, env=environment, text=True, capture_output=True)
        if result.returncode == 0 or "checksum" not in result.stderr or "crossterm" not in result.stderr:
            raise ValueError("source corruption must fail with the specific crossterm checksum error")
    print("PASS actual-source provenance negatives (20 cases) and source checksum corruption")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    snap = commands.add_parser("snapshot")
    snap.add_argument("--directory", required=True)
    seed = commands.add_parser("prepare")
    seed.add_argument("--directory", required=True)
    seed.add_argument("--revision", required=True)
    seed.add_argument("--tag", default="")
    seed.add_argument("--source", default="")
    seed.add_argument("--registry-vendor", required=True)
    verify = commands.add_parser("check")
    verify.add_argument("--directory", required=True)
    verify.add_argument("--revision", required=True)
    verify.add_argument("--tag", default="")
    negative = commands.add_parser("negative-check")
    negative.add_argument("--directory", required=True)
    negative.add_argument("--revision", required=True)
    arguments = parser.parse_args()
    try:
        if arguments.command == "snapshot":
            snapshot(arguments.directory)
        elif arguments.command == "prepare":
            prepare(arguments.directory, arguments.revision, arguments.source,
                    arguments.registry_vendor, arguments.tag)
        elif arguments.command == "check":
            check(arguments.directory, arguments.revision, arguments.tag)
        else:
            negative_check(arguments.directory, arguments.revision)
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"downstream: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
