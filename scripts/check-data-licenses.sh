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
THEME_SOURCE_FILE=$ROOT_DIR/crates/oom-edit/src/theme.rs
THEME_DIR=$ROOT_DIR/crates/oom-edit/assets/themes
THEME_PROVENANCE_FILE=$THEME_DIR/PROVENANCE.toml
THEME_MAPPING_FILE=$THEME_DIR/MAPPING.md
THEME_MANIFEST_FILE=$THEME_DIR/MANIFEST.sha256

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

require_absent() {
    local path=$1
    local literal=$2
    if LC_ALL=C grep -Fq -- "$literal" "$path"; then
        fail "$path contains excluded text: $literal"
    fi
}

require_count() {
    local path=$1
    local literal=$2
    local expected=$3
    local actual
    actual=$(LC_ALL=C grep -Fxc -- "$literal" "$path" || true)
    [[ "$actual" == "$expected" ]] ||
        fail "$path must contain '$literal' exactly $expected time(s), found $actual"
}

require_file_embedded() {
    local needle=$1
    local haystack=$2
    python3 -c 'import pathlib, sys; raise SystemExit(0 if pathlib.Path(sys.argv[1]).read_bytes() in pathlib.Path(sys.argv[2]).read_bytes() else 1)' "$needle" "$haystack" ||
        fail "$haystack does not contain complete file bytes from $needle"
}

for path in \
    "$LICENSE_FILE" \
    "$NOTICE_FILE" \
    "$PROVENANCE_FILE" \
    "$MANIFEST_FILE" \
    "$DEPENDENCIES_FILE" \
    "$ARGS_FILE" \
    "$THEME_SOURCE_FILE" \
    "$THEME_PROVENANCE_FILE" \
    "$THEME_MAPPING_FILE" \
    "$THEME_MANIFEST_FILE"; do
    require_file "$path"
done

require_digest "$LICENSE_FILE" e72bf965adc079738d41f13cd8f03d5dfbe2da50ddcd96d1e0640d29c8ae9742
require_digest "$NOTICE_FILE" febf3a92df9b085baa024d9ad175998b1cf0179ca6f06028cbd7890663d51cf5
require_digest "$PROVENANCE_FILE" 849bd99d9a040c724e59dc2a799f3ca4fdc7b48117c845e60395a4ad8186a502
require_digest "$MANIFEST_FILE" 697d5188f328d1232ed27a213db4ab5b8c3eba9c1a22d77667c85c75975e54f0
require_digest "$THEME_PROVENANCE_FILE" b581bc88ec9e532bbb5cc392edea924a9ed66ce7914a708138b29165b0a8f74a
require_digest "$THEME_MAPPING_FILE" 24975a6a240f793eeb5933b03de57a928a28ea26484b4705c57ca7bc77a9a546
require_digest "$THEME_MANIFEST_FILE" 9848a87d4165cf292c0683f888247e9b809a53ac5f4b434d888113026fb6a78b

while read -r expected relative_path; do
    [[ -n "$expected" && -n "$relative_path" ]] ||
        fail "$THEME_MANIFEST_FILE contains an invalid row"
    asset=$THEME_DIR/$relative_path
    require_file "$asset"
    require_digest "$asset" "$expected"
done <"$THEME_MANIFEST_FILE"

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
require_literal "$ARGS_FILE" 'include_str!("../../../THIRD-PARTY-NOTICES.md")'

for literal in \
    'retrieved = "2026-08-25"' \
    'mapping-notes = "MAPPING.md"' \
    'name = "catppuccin-mocha"' \
    'name = "dracula"' \
    'name = "nord"' \
    'name = "solarized-dark"' \
    'name = "tokyo-night"' \
    'declared-absence = "The pinned repository and license contain no project-specific copyright notice"' \
    'origin-declared-fact = "Copyright (c) 2018-present Enkia"'; do
    require_literal "$THEME_PROVENANCE_FILE" "$literal"
done
require_count "$THEME_PROVENANCE_FILE" '[[themes]]' 5

python3 - "$THEME_SOURCE_FILE" "$THEME_PROVENANCE_FILE" <<'PY' ||
import pathlib
import re
import sys

theme_source = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
provenance = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")

registry_match = re.search(
    r"static BUILTIN_THEMES:.*?= &\[(.*?)\n\];",
    theme_source,
    flags=re.DOTALL,
)
if registry_match is None:
    raise SystemExit("built-in theme registry not found")

registry = registry_match.group(1)
registry_pairs = re.findall(
    r'identifier: "([^"]+)",\s*spdx: "([^"]+)"',
    registry,
)
if len(registry_pairs) != len(set(registry_pairs)):
    raise SystemExit("built-in theme attribution identities are duplicated")

provenance_pairs = []
for block in provenance.split("[[themes]]")[1:]:
    name_match = re.search(r'^name = "([^"]+)"$', block, flags=re.MULTILINE)
    spdx_match = re.search(r'^spdx = "([^"]+)"$', block, flags=re.MULTILINE)
    if name_match is None or spdx_match is None:
        raise SystemExit("theme provenance row is missing name or SPDX identity")
    provenance_pairs.append((name_match.group(1), spdx_match.group(1)))

if len(provenance_pairs) != len(set(provenance_pairs)):
    raise SystemExit("theme provenance identities are duplicated")
if set(registry_pairs) != set(provenance_pairs):
    raise SystemExit(
        f"theme registry attribution identities {registry_pairs!r} do not match "
        f"provenance identities {provenance_pairs!r}"
    )
PY
    fail "built-in theme registry attribution does not match pinned provenance"

for heading in \
    '## Catppuccin Mocha bundled theme' \
    '## Dracula bundled theme' \
    '## Nord bundled theme' \
    '## Solarized Dark bundled theme' \
    '## Tokyo Night bundled theme'; do
    require_count "$NOTICE_FILE" "$heading" 1
done

for license in \
    "$THEME_DIR/upstream/catppuccin-mocha.LICENSE" \
    "$THEME_DIR/upstream/dracula.LICENSE" \
    "$THEME_DIR/upstream/nord.LICENSE" \
    "$THEME_DIR/upstream/solarized-dark.LICENSE" \
    "$THEME_DIR/upstream/tokyo-night.LICENSE" \
    "$THEME_DIR/upstream/tokyo-night-enkia-origin.LICENSE.txt"; do
    require_file_embedded "$license" "$NOTICE_FILE"
done

for path in "$THEME_PROVENANCE_FILE" "$THEME_MAPPING_FILE" "$NOTICE_FILE"; do
    require_absent "$path" 'Gruvbox'
    require_absent "$path" 'Rosé Pine'
done

echo "data-license-check: all bundled data and attribution surfaces are valid"
