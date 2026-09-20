#!/usr/bin/env bash
set -euo pipefail

if [[ $# -eq 0 ]]; then
    echo "usage: $0 cargo-deny-arguments..." >&2
    exit 2
fi

script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd -- "${script_directory}/.." && pwd)"
temporary_base="${TMPDIR:-/tmp}"
temporary_working_directory="$(mktemp -d "${temporary_base%/}/oom-edit-cargo-deny.XXXXXX")"

cleanup() {
    cleanup_status=$?
    trap - EXIT HUP INT TERM
    rm -rf -- "${temporary_working_directory}"
    exit "${cleanup_status}"
}
trap cleanup EXIT HUP INT TERM

# Cargo discovers .cargo/config.toml from its working directory. Fetch outside
# the repository so yank checks have current registry metadata, then return so
# cargo-deny still evaluates licenses and sources through the vendored config.
cd -- "${temporary_working_directory}"
cargo fetch --locked --manifest-path "${repository_root}/Cargo.toml"

cd -- "${repository_root}"
cargo deny "$@"
