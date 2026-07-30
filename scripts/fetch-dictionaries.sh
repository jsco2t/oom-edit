#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT_DIR=$(cd "$SCRIPT_DIR/.." && pwd)
OUTPUT_DIR=$ROOT_DIR/crates/oom-edit/assets/dict
RELEASE=2026.02.25
SOURCE_BASE=https://sourceforge.net/projects/wordlist/files/speller/$RELEASE

fail() {
    echo "fetch-dictionaries: $*" >&2
    exit 1
}

command -v curl >/dev/null 2>&1 || fail "required command not found: curl"

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/oom-edit-dictionary-fetch.XXXXXX")
trap 'rm -rf "$WORK_DIR"' EXIT
ARCHIVE_DIR=$WORK_DIR/archives
CONFIG_FILE=$WORK_DIR/config
mkdir -p "$ARCHIVE_DIR"

cat >"$CONFIG_FILE" <<'EOF'
en_US|caeb6ee8a38e98ccbd6e5c717889a1bab68073dcba8a9c0ca6570641926913e9|5e7f675015d514fd87824230043751576559a8683d1e3aeb15229d0c8bad874f|109902
en_CA|775d01fdd60e86f8f9e48da75b1cd3caa02c677b39629644b9bd4f3af42f820b|77581170607b92e7479520a44f666033692d97a546a8165f35654b97771a3507|109544
en_AU|93295124db43eba58f5257d7e17865930e8ad3696a849d6a366ec9ae9f7da20a|bf808e578445dff4cc8c7af19272ac32544d1031f94423201d3be9b2fdf784b9|110082
EOF

for locale in en_US en_CA en_AU; do
    archive_name=wordlist-$locale-$RELEASE.zip
    curl -fL --retry 3 -o "$ARCHIVE_DIR/$archive_name" "$SOURCE_BASE/$archive_name/download"
done

bash "$SCRIPT_DIR/generate-dictionaries.sh" \
    "$ARCHIVE_DIR" \
    "$OUTPUT_DIR" \
    "$RELEASE" \
    "$SOURCE_BASE" \
    "$CONFIG_FILE"
