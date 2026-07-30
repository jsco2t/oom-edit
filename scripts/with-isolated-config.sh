#!/usr/bin/env bash
set -euo pipefail

if [[ $# -eq 0 ]]; then
    echo "usage: $0 command [args ...]" >&2
    exit 2
fi

isolation_base="${TMPDIR:-/tmp}"
isolation_root="$(mktemp -d "${isolation_base%/}/oom-edit-config.XXXXXX")"

cleanup() {
    cleanup_status=$?
    trap - EXIT HUP INT TERM
    rm -rf -- "${isolation_root}"
    exit "${cleanup_status}"
}
trap cleanup EXIT HUP INT TERM

export XDG_CONFIG_HOME="${isolation_root}/xdg"
mkdir -p -- "${XDG_CONFIG_HOME}"

"$@"
