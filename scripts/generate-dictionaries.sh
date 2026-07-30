#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 5 ]]; then
    echo "usage: generate-dictionaries.sh ARCHIVE_DIR OUTPUT_DIR RELEASE SOURCE_BASE CONFIG_FILE" >&2
    exit 2
fi

ARCHIVE_DIR=$1
OUTPUT_DIR=$2
RELEASE=$3
SOURCE_BASE=$4
CONFIG_FILE=$5
HEADER_VERSION=1

fail() {
    echo "generate-dictionaries: $*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
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

require_command awk
require_command iconv
require_command sort
require_command unzip
[[ -f "$CONFIG_FILE" ]] || fail "config not found: $CONFIG_FILE"

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/oom-edit-dictionary-generate.XXXXXX")
trap 'rm -rf "$WORK_DIR"' EXIT
STAGE_DIR=$WORK_DIR/output
mkdir -p "$STAGE_DIR"

cat >"$STAGE_DIR/PROVENANCE.txt" <<EOF
format-version=1
release=$RELEASE
source-base-url=$SOURCE_BASE
header-version=$HEADER_VERSION
normalization=extract only the locale .txt member; validate UTF-8 and sorted uniqueness; convert CRLF to LF; retain every entry; write exactly one final newline; prepend the locale copyright, SPDX, and license-pointer header
EOF
: >"$STAGE_DIR/MANIFEST.sha256"

locale_count=0
while IFS='|' read -r locale archive_sha output_sha expected_entries; do
    [[ -n "$locale" ]] || continue
    locale_count=$((locale_count + 1))
    case "$locale" in
        en_US | en_CA | en_AU) ;;
        *) fail "unsupported locale in dictionary config: $locale" ;;
    esac
    [[ "$archive_sha" =~ ^[0-9a-f]{64}$ ]] || fail "$locale has an invalid archive SHA-256"
    [[ "$output_sha" =~ ^[0-9a-f]{64}$ ]] || fail "$locale has an invalid output SHA-256"
    [[ "$expected_entries" =~ ^[0-9]+$ ]] || fail "$locale has an invalid entry count"

    archive_name=wordlist-$locale-$RELEASE.zip
    archive_path=$ARCHIVE_DIR/$archive_name
    source_url=$SOURCE_BASE/$archive_name/download
    [[ -f "$archive_path" ]] || fail "archive not found: $archive_path"

    actual_archive_sha=$(sha256_file "$archive_path")
    [[ "$actual_archive_sha" == "$archive_sha" ]] ||
        fail "$archive_name SHA-256 mismatch: expected $archive_sha, got $actual_archive_sha"

    raw_path=$WORK_DIR/$locale.raw
    words_path=$WORK_DIR/$locale.words
    asset_path=$STAGE_DIR/$locale.txt

    unzip -p "$archive_path" "$locale.txt" >"$raw_path" ||
        fail "$archive_name does not contain the expected $locale.txt member"
    iconv -f UTF-8 -t UTF-8 "$raw_path" >/dev/null ||
        fail "$locale.txt is not valid UTF-8"

    awk '
        {
            sub(/\r$/, "")
            lines[NR] = $0
        }
        END {
            last = NR
            while (last > 0 && lines[last] == "") {
                last--
            }
            for (line = 1; line <= last; line++) {
                print lines[line]
            }
        }
    ' "$raw_path" >"$words_path"

    if LC_ALL=C grep -q "$(printf '\r')" "$words_path"; then
        fail "$locale.txt contains a carriage return after normalization"
    fi
    LC_ALL=C sort -c -u "$words_path" || fail "$locale.txt is not sorted and unique"

    actual_entries=$(awk 'END { print NR }' "$words_path")
    [[ "$actual_entries" == "$expected_entries" ]] ||
        fail "$locale.txt entry count mismatch: expected $expected_entries, got $actual_entries"

    if [[ "$locale" == en_AU ]]; then
        printf '%s\n' \
            '# Copyright 2000-2026 by Kevin Atkinson' \
            '# Copyright 2016 by Benjamin Titze' \
            '# SPDX-License-Identifier: HPND-sell-variant' \
            '# Full permission notice and disclaimer: SCOWL-LICENSE.txt' \
            >"$asset_path"
    else
        printf '%s\n' \
            '# Copyright 2000-2026 by Kevin Atkinson' \
            '# SPDX-License-Identifier: HPND-sell-variant' \
            '# Full permission notice and disclaimer: SCOWL-LICENSE.txt' \
            >"$asset_path"
    fi
    sed -n '1,$p' "$words_path" >>"$asset_path"

    actual_output_sha=$(sha256_file "$asset_path")
    [[ "$actual_output_sha" == "$output_sha" ]] ||
        fail "$locale.txt normalized SHA-256 mismatch: expected $output_sha, got $actual_output_sha"

    printf '%s  %s.txt\n' "$output_sha" "$locale" >>"$STAGE_DIR/MANIFEST.sha256"
    cat >>"$STAGE_DIR/PROVENANCE.txt" <<EOF

[$locale]
url=$source_url
archive-sha256=$archive_sha
source-member=$locale.txt
license-source=README_$locale.txt
output-sha256=$output_sha
entry-count=$expected_entries
EOF
done <"$CONFIG_FILE"

[[ "$locale_count" == 3 ]] || fail "dictionary config must contain exactly three locales"

mkdir -p "$OUTPUT_DIR"
for generated in en_US.txt en_CA.txt en_AU.txt MANIFEST.sha256 PROVENANCE.txt; do
    cp "$STAGE_DIR/$generated" "$OUTPUT_DIR/$generated"
done

echo "Generated en_US, en_CA, and en_AU dictionaries in $OUTPUT_DIR"
