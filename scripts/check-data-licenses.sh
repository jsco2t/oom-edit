#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR=${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
ASSET_DIR=$ROOT_DIR/crates/oom-edit/assets/dict
LICENSE_FILE=$ASSET_DIR/SCOWL-LICENSE.txt
NOTICE_FILE=$ROOT_DIR/THIRD-PARTY-NOTICES.md
PROVENANCE_FILE=$ASSET_DIR/PROVENANCE.txt
MANIFEST_FILE=$ASSET_DIR/MANIFEST.sha256
DEPENDENCIES_FILE=$ROOT_DIR/docs/dependencies.md
ARGS_FILE=$ROOT_DIR/crates/oom-edit/src/args.rs

fail() {
    echo "data-license-check: $*" >&2
    exit 1
}

sha256_file() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        fail "required command not found: shasum or sha256sum"
    fi
}

require_file() {
    [[ -f "$1" ]] || fail "required file missing: $1"
}

require_digest() {
    local path=$1
    local expected=$2
    local actual
    actual=$(sha256_file "$path")
    [[ "$actual" == "$expected" ]] ||
        fail "$path SHA-256 mismatch: expected $expected, got $actual"
}

require_literal() {
    local path=$1
    local literal=$2
    LC_ALL=C grep -Fq -- "$literal" "$path" ||
        fail "$path is missing required text: $literal"
}

for path in \
    "$LICENSE_FILE" \
    "$NOTICE_FILE" \
    "$PROVENANCE_FILE" \
    "$MANIFEST_FILE" \
    "$DEPENDENCIES_FILE" \
    "$ARGS_FILE"; do
    require_file "$path"
done

require_digest "$LICENSE_FILE" e72bf965adc079738d41f13cd8f03d5dfbe2da50ddcd96d1e0640d29c8ae9742
require_digest "$NOTICE_FILE" 8d2597df5f1e802482bcacc9f100baf6c1be7dc4a95578d91baedd3313239019
require_digest "$PROVENANCE_FILE" 849bd99d9a040c724e59dc2a799f3ca4fdc7b48117c845e60395a4ad8186a502
require_digest "$MANIFEST_FILE" 697d5188f328d1232ed27a213db4ab5b8c3eba9c1a22d77667c85c75975e54f0

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/oom-edit-data-license.XXXXXX")
trap 'rm -rf "$WORK_DIR"' EXIT

for locale in en_US en_CA en_AU; do
    case "$locale" in
        en_US)
            expected_sha=5e7f675015d514fd87824230043751576559a8683d1e3aeb15229d0c8bad874f
            expected_entries=109902
            expected_header_lines=3
            ;;
        en_CA)
            expected_sha=77581170607b92e7479520a44f666033692d97a546a8165f35654b97771a3507
            expected_entries=109544
            expected_header_lines=3
            ;;
        en_AU)
            expected_sha=bf808e578445dff4cc8c7af19272ac32544d1031f94423201d3be9b2fdf784b9
            expected_entries=110082
            expected_header_lines=4
            ;;
        *)
            fail "unsupported locale: $locale"
            ;;
    esac

    asset=$ASSET_DIR/$locale.txt
    require_file "$asset"
    require_digest "$asset" "$expected_sha"
    iconv -f UTF-8 -t UTF-8 "$asset" >/dev/null || fail "$asset is not valid UTF-8"

    if LC_ALL=C grep -q "$(printf '\r')" "$asset"; then
        fail "$asset contains a carriage return"
    fi
    final_hex=$(tail -c 2 "$asset" | od -An -tx1 | tr -d ' \n')
    [[ "$final_hex" == *0a ]] || fail "$asset does not end with LF"
    [[ "$final_hex" != 0a0a ]] || fail "$asset has more than one final LF"

    header=$WORK_DIR/$locale.header
    words=$WORK_DIR/$locale.words
    sed -n "1,${expected_header_lines}p" "$asset" >"$header"
    sed -n "$((expected_header_lines + 1)),\$p" "$asset" >"$words"

    if [[ "$locale" == en_AU ]]; then
        cat >"$WORK_DIR/expected-header" <<'EOF'
# Copyright 2000-2026 by Kevin Atkinson
# Copyright 2016 by Benjamin Titze
# SPDX-License-Identifier: HPND-sell-variant
# Full permission notice and disclaimer: SCOWL-LICENSE.txt
EOF
    else
        cat >"$WORK_DIR/expected-header" <<'EOF'
# Copyright 2000-2026 by Kevin Atkinson
# SPDX-License-Identifier: HPND-sell-variant
# Full permission notice and disclaimer: SCOWL-LICENSE.txt
EOF
        if LC_ALL=C grep -Fq 'Benjamin Titze' "$asset"; then
            fail "$asset must not carry the en_AU-only Benjamin Titze header"
        fi
    fi
    cmp -s "$header" "$WORK_DIR/expected-header" || fail "$asset has an invalid header"

    LC_ALL=C sort -c -u "$words" || fail "$asset entries are not sorted and unique"
    actual_entries=$(awk 'END { print NR }' "$words")
    [[ "$actual_entries" == "$expected_entries" ]] ||
        fail "$asset entry count mismatch: expected $expected_entries, got $actual_entries"
done

LC_ALL=C sort -u "$WORK_DIR/en_US.words" "$WORK_DIR/en_CA.words" "$WORK_DIR/en_AU.words" \
    >"$WORK_DIR/merged.words"
merged_entries=$(awk 'END { print NR }' "$WORK_DIR/merged.words")
[[ "$merged_entries" == 113642 ]] ||
    fail "merged entry count mismatch: expected 113642, got $merged_entries"

LC_ALL=C grep -qx color "$WORK_DIR/en_US.words" || fail "en_US must contain color"
if LC_ALL=C grep -qx colour "$WORK_DIR/en_US.words"; then
    fail "en_US must not contain colour"
fi
for locale in en_CA en_AU; do
    LC_ALL=C grep -qx colour "$WORK_DIR/$locale.words" || fail "$locale must contain colour"
    if LC_ALL=C grep -qx color "$WORK_DIR/$locale.words"; then
        fail "$locale must not contain color"
    fi
done

for path in "$LICENSE_FILE" "$NOTICE_FILE"; do
    require_literal "$path" 'Copyright 2000-2026 by Kevin Atkinson'
    require_literal "$path" 'Permission to use, copy, modify, distribute, and sell any part of SCOWLv2, or'
    require_literal "$path" 'any purpose.  It is provided "as is" without express or implied warranty.'
    require_literal "$path" 'Copyright 2016 by Benjamin Titze'
    require_literal "$path" 'Permission to use, copy, modify, distribute and sell this array, the'
    require_literal "$path" 'purpose. It is provided "as is" without express or implied warranty.'
done

for literal in \
    'HPND-sell-variant' \
    'approved on 2026-08-13 only for' \
    'wordlist-en_{US,CA,AU}-2026.02.25.zip' \
    'CRLF to LF' \
    'Complete unmodified permission notices and warranty' \
    'Update/removal condition'; do
    require_literal "$DEPENDENCIES_FILE" "$literal"
done

require_literal "$ARGS_FILE" '"--licenses"'
require_literal "$ARGS_FILE" 'include_str!("../assets/dict/SCOWL-LICENSE.txt")'

echo "data-license-check: all bundled dictionary data and attribution surfaces are valid"
