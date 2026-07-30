use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const ATKINSON_NOTICE: &str = "Copyright 2000-2026 by Kevin Atkinson\n\n\
Permission to use, copy, modify, distribute, and sell any part of SCOWLv2, or\n\
word lists created from it, is hereby granted without fee, provided that the\n\
above copyright notice appears in all copies and that both the above\n\
copyright notice and this notice appear in supporting documentation.  Kevin\n\
Atkinson makes no representations about the suitability of this database for\n\
any purpose.  It is provided \"as is\" without express or implied warranty.";
const TITZE_NOTICE: &str = r#"  Copyright 2016 by Benjamin Titze

  Permission to use, copy, modify, distribute and sell this array, the
  associated software, and its documentation for any purpose is hereby
  granted without fee, provided that the above copyright notice appears
  in all copies and that both that copyright notice and this permission
  notice appear in supporting documentation. Benjamin Titze makes no
  representations about the suitability of this array for any
  purpose. It is provided "as is" without express or implied warranty."#;

const KEVIN_HEADER: &str = "# Copyright 2000-2026 by Kevin Atkinson\n\
# SPDX-License-Identifier: HPND-sell-variant\n\
# Full permission notice and disclaimer: SCOWL-LICENSE.txt\n";
const AU_HEADER: &str = "# Copyright 2000-2026 by Kevin Atkinson\n\
# Copyright 2016 by Benjamin Titze\n\
# SPDX-License-Identifier: HPND-sell-variant\n\
# Full permission notice and disclaimer: SCOWL-LICENSE.txt\n";

struct ExpectedAsset {
    locale: &'static str,
    sha256: &'static str,
    entries: usize,
    header: &'static str,
}

const ASSETS: &[ExpectedAsset] = &[
    ExpectedAsset {
        locale: "en_US",
        sha256: "5e7f675015d514fd87824230043751576559a8683d1e3aeb15229d0c8bad874f",
        entries: 109_902,
        header: KEVIN_HEADER,
    },
    ExpectedAsset {
        locale: "en_CA",
        sha256: "77581170607b92e7479520a44f666033692d97a546a8165f35654b97771a3507",
        entries: 109_544,
        header: KEVIN_HEADER,
    },
    ExpectedAsset {
        locale: "en_AU",
        sha256: "bf808e578445dff4cc8c7af19272ac32544d1031f94423201d3be9b2fdf784b9",
        entries: 110_082,
        header: AU_HEADER,
    },
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root should resolve")
}

fn sha256(path: &Path) -> String {
    let output = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .or_else(|_| Command::new("sha256sum").arg(path).output())
        .expect("shasum or sha256sum should be installed");
    assert!(
        output.status.success(),
        "hash command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("hash output should be UTF-8")
        .split_whitespace()
        .next()
        .expect("hash output should contain a digest")
        .to_string()
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xedb8_8320
            };
        }
    }
    !crc
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_stored_zip(path: &Path, member: &str, contents: &[u8]) {
    let member = member.as_bytes();
    let member_len = u16::try_from(member.len()).expect("fixture member name should fit u16");
    let content_len = u32::try_from(contents.len()).expect("fixture contents should fit u32");
    let checksum = crc32(contents);
    let mut archive = Vec::new();

    push_u32(&mut archive, 0x0403_4b50);
    for value in [20, 0, 0, 0, 0] {
        push_u16(&mut archive, value);
    }
    push_u32(&mut archive, checksum);
    push_u32(&mut archive, content_len);
    push_u32(&mut archive, content_len);
    push_u16(&mut archive, member_len);
    push_u16(&mut archive, 0);
    archive.extend_from_slice(member);
    archive.extend_from_slice(contents);

    let central_offset = u32::try_from(archive.len()).expect("fixture offset should fit u32");
    push_u32(&mut archive, 0x0201_4b50);
    for value in [20, 20, 0, 0, 0, 0] {
        push_u16(&mut archive, value);
    }
    push_u32(&mut archive, checksum);
    push_u32(&mut archive, content_len);
    push_u32(&mut archive, content_len);
    push_u16(&mut archive, member_len);
    for value in [0, 0, 0, 0] {
        push_u16(&mut archive, value);
    }
    push_u32(&mut archive, 0);
    push_u32(&mut archive, 0);
    archive.extend_from_slice(member);

    let central_size =
        u32::try_from(archive.len()).expect("fixture size should fit u32") - central_offset;
    push_u32(&mut archive, 0x0605_4b50);
    for value in [0, 0, 1, 1] {
        push_u16(&mut archive, value);
    }
    push_u32(&mut archive, central_size);
    push_u32(&mut archive, central_offset);
    push_u16(&mut archive, 0);

    let mut file = fs::File::create(path).expect("fixture archive should be created");
    file.write_all(&archive)
        .expect("fixture archive should be written");
}

#[test]
fn dictionary_assets_sane() {
    let asset_dir = workspace_root().join("crates/oom-edit/assets/dict");
    let mut merged = BTreeSet::new();

    for expected in ASSETS {
        let path = asset_dir.join(format!("{}.txt", expected.locale));
        let bytes = fs::read(&path)
            .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
        let text = std::str::from_utf8(&bytes)
            .unwrap_or_else(|error| panic!("{} should be UTF-8: {error}", path.display()));

        assert!(
            text.starts_with(expected.header),
            "{} header",
            expected.locale
        );
        assert!(!text.contains('\r'), "{} should use LF", expected.locale);
        assert!(text.ends_with('\n'), "{} should end in LF", expected.locale);
        assert!(
            !text.ends_with("\n\n"),
            "{} should have exactly one final LF",
            expected.locale
        );
        assert_eq!(sha256(&path), expected.sha256, "{} hash", expected.locale);

        let words: Vec<_> = text.lines().filter(|line| !line.starts_with('#')).collect();
        assert_eq!(words.len(), expected.entries, "{} entries", expected.locale);
        assert!(
            words.windows(2).all(|pair| pair[0] < pair[1]),
            "{} entries should be sorted and unique",
            expected.locale
        );
        merged.extend(words.iter().map(|word| (*word).to_string()));

        let has_color = words.binary_search(&"color").is_ok();
        let has_colour = words.binary_search(&"colour").is_ok();
        assert_eq!(has_color, expected.locale == "en_US");
        assert_eq!(has_colour, expected.locale != "en_US");
    }

    assert_eq!(merged.len(), 113_642, "merged distinct entry count");

    let manifest = fs::read_to_string(asset_dir.join("MANIFEST.sha256"))
        .expect("generated hash manifest should be readable");
    let expected_manifest = ASSETS
        .iter()
        .map(|asset| format!("{}  {}.txt\n", asset.sha256, asset.locale))
        .collect::<String>();
    assert_eq!(manifest, expected_manifest);

    let provenance = fs::read_to_string(asset_dir.join("PROVENANCE.txt"))
        .expect("generated provenance manifest should be readable");
    for field in [
        "format-version=1",
        "release=2026.02.25",
        "source-base-url=https://sourceforge.net/projects/wordlist/files/speller/2026.02.25",
        "header-version=1",
        "normalization=extract only the locale .txt member",
    ] {
        assert!(
            provenance.contains(field),
            "missing provenance field: {field}"
        );
    }
    for asset in ASSETS {
        assert!(provenance.contains(&format!("[{}]", asset.locale)));
        assert!(provenance.contains(&format!("source-member={}.txt", asset.locale)));
        assert!(provenance.contains(&format!("license-source=README_{}.txt", asset.locale)));
        assert!(provenance.contains(&format!("output-sha256={}", asset.sha256)));
        assert!(provenance.contains(&format!("entry-count={}", asset.entries)));
    }
}

#[test]
fn license_notices_match_all_distribution_surfaces() {
    let root = workspace_root();
    let license = fs::read_to_string(root.join("crates/oom-edit/assets/dict/SCOWL-LICENSE.txt"))
        .expect("SCOWL license should be readable");
    let notices = fs::read_to_string(root.join("THIRD-PARTY-NOTICES.md"))
        .expect("third-party notices should be readable");

    for surface in [&license, &notices] {
        assert!(surface.contains(ATKINSON_NOTICE));
        assert!(surface.contains(TITZE_NOTICE));
    }

    let output = Command::new(env!("CARGO_BIN_EXE_oom-edit"))
        .arg("--licenses")
        .output()
        .expect("oom-edit --licenses should run without a terminal");
    assert!(
        output.status.success(),
        "--licenses failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "--licenses should not initialize the TUI"
    );
    let binary_output = String::from_utf8(output.stdout).expect("license output should be UTF-8");
    assert_eq!(binary_output, license);
    assert!(binary_output.contains(ATKINSON_NOTICE));
    assert!(binary_output.contains(TITZE_NOTICE));
    for absent in ["Geoff Kuenning", "BSD", "MIT", "endorse or promote"] {
        assert!(
            !binary_output.contains(absent),
            "binary output must not contain {absent:?}"
        );
    }
}

#[test]
fn dictionary_make_targets_are_discoverable_and_integrated() {
    let root = workspace_root();
    for (target, expected) in [
        ("dictionaries", "bash scripts/fetch-dictionaries.sh"),
        ("data-license-check", "bash scripts/check-data-licenses.sh"),
        ("check", "bash scripts/check-data-licenses.sh"),
    ] {
        let output = Command::new("make")
            .args(["--dry-run", target])
            .current_dir(&root)
            .output()
            .unwrap_or_else(|error| panic!("make --dry-run {target} should run: {error}"));
        assert!(
            output.status.success(),
            "make --dry-run {target} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(expected),
            "make --dry-run {target} should contain {expected:?}:\n{stdout}"
        );
    }

    let output = Command::new("make")
        .arg("help")
        .current_dir(&root)
        .output()
        .expect("make help should run");
    assert!(
        output.status.success(),
        "make help should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for (target, description) in [
        (
            "dictionaries",
            "Regenerate pinned en_US/en_CA/en_AU dictionaries (requires network)",
        ),
        (
            "data-license-check",
            "Verify bundled-data hashes, headers, notices, and provenance",
        ),
    ] {
        assert!(
            stdout.contains(target),
            "make help should list {target}:\n{stdout}"
        );
        assert!(
            stdout.contains(description),
            "make help should describe {target} as {description:?}:\n{stdout}"
        );
    }
}

#[test]
fn spell_configuration_and_plain_wordlist_rules_are_documented() {
    let root = workspace_root();
    let readme = fs::read_to_string(root.join("README.md")).expect("README should be readable");
    let spell_readme = fs::read_to_string(root.join("crates/oom-spell/README.md"))
        .expect("oom-spell README should be readable");

    for required in [
        "[spell]",
        "language = \"en_US\"",
        "additional_dictionaries",
        "directory containing `config.toml`",
        "larger-than-16-MiB",
        "one entry per line",
        "non-whitespace character is `#`",
        "64-byte maximum",
        "dictionary.txt",
        "`oom-edit --licenses`",
    ] {
        assert!(
            readme.contains(required),
            "root README must document {required:?}"
        );
    }
    for required in [
        "entry per physical line",
        "first non-whitespace byte is `#`",
        "Invalid, non-ASCII, and",
        "at most 64 bytes",
        "at most nine results",
    ] {
        assert!(
            spell_readme.contains(required),
            "oom-spell README must document {required:?}"
        );
    }
}

#[test]
fn dictionary_generator_normalizes_local_archives_byte_identically() {
    let root = workspace_root();
    let fixture = tempfile::tempdir().expect("generator fixture tempdir");
    let archive_dir = fixture.path().join("archives");
    let output_dir = fixture.path().join("output");
    fs::create_dir(&archive_dir).expect("archive directory should be created");

    let mut config = String::new();
    let mut expected_manifest = String::new();
    let mut expected_provenance = "format-version=1\n\
release=fixture\n\
source-base-url=https://example.invalid/fixture\n\
header-version=1\n\
normalization=extract only the locale .txt member; validate UTF-8 and sorted uniqueness; convert CRLF to LF; retain every entry; write exactly one final newline; prepend the locale copyright, SPDX, and license-pointer header\n"
        .to_string();
    for locale in ["en_US", "en_CA", "en_AU"] {
        let words = if locale == "en_US" {
            b"alpha\r\ncolor\r\n\r\n".as_slice()
        } else {
            b"alpha\r\ncolour\r\n\r\n".as_slice()
        };
        let archive = archive_dir.join(format!("wordlist-{locale}-fixture.zip"));
        write_stored_zip(&archive, &format!("{locale}.txt"), words);

        let expected = fixture.path().join(format!("expected-{locale}.txt"));
        let header = if locale == "en_AU" {
            AU_HEADER
        } else {
            KEVIN_HEADER
        };
        let normalized_words = if locale == "en_US" {
            "alpha\ncolor\n"
        } else {
            "alpha\ncolour\n"
        };
        fs::write(&expected, format!("{header}{normalized_words}"))
            .expect("expected fixture asset should write");

        let archive_sha = sha256(&archive);
        let output_sha = sha256(&expected);
        config.push_str(&format!("{locale}|{archive_sha}|{output_sha}|2\n"));
        expected_manifest.push_str(&format!("{output_sha}  {locale}.txt\n"));
        expected_provenance.push_str(&format!(
            "\n[{locale}]\n\
url=https://example.invalid/fixture/wordlist-{locale}-fixture.zip/download\n\
archive-sha256={archive_sha}\n\
source-member={locale}.txt\n\
license-source=README_{locale}.txt\n\
output-sha256={output_sha}\n\
entry-count=2\n"
        ));
    }
    let config_path = fixture.path().join("config.txt");
    fs::write(&config_path, &config).expect("fixture config should write");

    let output = run_dictionary_generator(&root, &archive_dir, &output_dir, &config_path);
    assert!(
        output.status.success(),
        "dictionary generator failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    for locale in ["en_US", "en_CA", "en_AU"] {
        let actual = fs::read(output_dir.join(format!("{locale}.txt")))
            .expect("generated fixture asset should be readable");
        let expected = fs::read(fixture.path().join(format!("expected-{locale}.txt")))
            .expect("expected fixture asset should be readable");
        assert_eq!(actual, expected, "{locale} normalization should be exact");
    }
    assert_eq!(
        fs::read_to_string(output_dir.join("MANIFEST.sha256"))
            .expect("fixture manifest should be readable"),
        expected_manifest
    );
    assert_eq!(
        fs::read_to_string(output_dir.join("PROVENANCE.txt"))
            .expect("fixture provenance should be readable"),
        expected_provenance
    );

    let bad_archive_output = fixture.path().join("bad-archive-output");
    let us_archive = archive_dir.join("wordlist-en_US-fixture.zip");
    OpenOptions::new()
        .append(true)
        .open(&us_archive)
        .expect("fixture archive should reopen")
        .write_all(b"tampered")
        .expect("fixture archive should be tampered");
    let output = run_dictionary_generator(&root, &archive_dir, &bad_archive_output, &config_path);
    assert!(
        !output.status.success(),
        "tampered archive must be rejected"
    );
    assert!(
        !bad_archive_output.exists(),
        "archive mismatch must publish no output"
    );

    write_stored_zip(&us_archive, "en_US.txt", b"alpha\r\ncolor\r\n\r\n");
    let bad_output_config = fixture.path().join("bad-output-config.txt");
    let invalid_config = config
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                let mut fields: Vec<_> = line.split('|').collect();
                fields[2] = "0000000000000000000000000000000000000000000000000000000000000000";
                fields.join("|")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(&bad_output_config, invalid_config).expect("invalid output config should write");
    let bad_digest_output = fixture.path().join("bad-digest-output");
    let output =
        run_dictionary_generator(&root, &archive_dir, &bad_digest_output, &bad_output_config);
    assert!(
        !output.status.success(),
        "incorrect normalized output digest must be rejected"
    );
    assert!(
        !bad_digest_output.exists(),
        "output digest mismatch must publish no output"
    );
}

fn run_dictionary_generator(
    root: &Path,
    archive_dir: &Path,
    output_dir: &Path,
    config_path: &Path,
) -> std::process::Output {
    Command::new("bash")
        .arg("scripts/generate-dictionaries.sh")
        .arg(archive_dir)
        .arg(output_dir)
        .arg("fixture")
        .arg("https://example.invalid/fixture")
        .arg(config_path)
        .current_dir(root)
        .output()
        .expect("dictionary generator should run against local fixtures")
}

fn copy_fixture_file(root: &Path, fixture: &Path, relative: &str) {
    let destination = fixture.join(relative);
    fs::create_dir_all(destination.parent().expect("fixture file has a parent"))
        .expect("fixture directory should be created");
    fs::copy(root.join(relative), &destination)
        .unwrap_or_else(|error| panic!("copy fixture {relative}: {error}"));
}

fn data_license_check(root: &Path, fixture: &Path) -> std::process::Output {
    Command::new("make")
        .args([
            "--no-print-directory",
            "data-license-check",
            &format!("DATA_LICENSE_ROOT={}", fixture.display()),
        ])
        .current_dir(root)
        .output()
        .expect("make data-license-check should run")
}

#[test]
fn data_license_check_fails_closed_for_contract_mutations() {
    let root = workspace_root();
    let fixture = tempfile::tempdir().expect("fixture tempdir");
    let files = [
        "THIRD-PARTY-NOTICES.md",
        "docs/dependencies.md",
        "crates/oom-edit/src/args.rs",
        "crates/oom-edit/assets/dict/SCOWL-LICENSE.txt",
        "crates/oom-edit/assets/dict/MANIFEST.sha256",
        "crates/oom-edit/assets/dict/PROVENANCE.txt",
        "crates/oom-edit/assets/dict/en_US.txt",
        "crates/oom-edit/assets/dict/en_CA.txt",
        "crates/oom-edit/assets/dict/en_AU.txt",
    ];
    for relative in files {
        copy_fixture_file(&root, fixture.path(), relative);
    }

    let baseline = data_license_check(&root, fixture.path());
    assert!(
        baseline.status.success(),
        "unmodified fixture should pass:\n{}",
        String::from_utf8_lossy(&baseline.stderr)
    );

    let mutations = [
        (
            "crates/oom-edit/assets/dict/en_US.txt",
            "# Copyright 2000-2026 by Kevin Atkinson\n",
        ),
        (
            "crates/oom-edit/assets/dict/en_CA.txt",
            "# Copyright 2000-2026 by Kevin Atkinson\n",
        ),
        (
            "crates/oom-edit/assets/dict/en_AU.txt",
            "# Copyright 2000-2026 by Kevin Atkinson\n",
        ),
        (
            "crates/oom-edit/assets/dict/en_AU.txt",
            "# Copyright 2016 by Benjamin Titze\n",
        ),
        (
            "crates/oom-edit/assets/dict/SCOWL-LICENSE.txt",
            "Permission to use, copy, modify, distribute, and sell any part of SCOWLv2, or\n",
        ),
        (
            "crates/oom-edit/assets/dict/SCOWL-LICENSE.txt",
            "any purpose.  It is provided \"as is\" without express or implied warranty.\n",
        ),
        (
            "crates/oom-edit/assets/dict/SCOWL-LICENSE.txt",
            "  Permission to use, copy, modify, distribute and sell this array, the\n",
        ),
        (
            "crates/oom-edit/assets/dict/SCOWL-LICENSE.txt",
            "  purpose. It is provided \"as is\" without express or implied warranty.\n",
        ),
        (
            "THIRD-PARTY-NOTICES.md",
            "Permission to use, copy, modify, distribute, and sell any part of SCOWLv2, or\n",
        ),
        (
            "THIRD-PARTY-NOTICES.md",
            "any purpose.  It is provided \"as is\" without express or implied warranty.\n",
        ),
        (
            "THIRD-PARTY-NOTICES.md",
            "  Permission to use, copy, modify, distribute and sell this array, the\n",
        ),
        (
            "THIRD-PARTY-NOTICES.md",
            "  purpose. It is provided \"as is\" without express or implied warranty.\n",
        ),
        ("docs/dependencies.md", "HPND-sell-variant"),
        ("docs/dependencies.md", "approved on 2026-08-13 only for"),
        (
            "docs/dependencies.md",
            "wordlist-en_{US,CA,AU}-2026.02.25.zip",
        ),
        ("docs/dependencies.md", "CRLF to LF"),
        (
            "docs/dependencies.md",
            "Complete unmodified permission notices and warranty",
        ),
        ("docs/dependencies.md", "Update/removal condition"),
        ("crates/oom-edit/src/args.rs", "\"--licenses\""),
        (
            "crates/oom-edit/src/args.rs",
            "include_str!(\"../assets/dict/SCOWL-LICENSE.txt\")",
        ),
        (
            "crates/oom-edit/assets/dict/PROVENANCE.txt",
            "format-version=1\n",
        ),
        (
            "crates/oom-edit/assets/dict/PROVENANCE.txt",
            "release=2026.02.25\n",
        ),
        (
            "crates/oom-edit/assets/dict/PROVENANCE.txt",
            "source-base-url=",
        ),
        (
            "crates/oom-edit/assets/dict/PROVENANCE.txt",
            "header-version=1\n",
        ),
        (
            "crates/oom-edit/assets/dict/PROVENANCE.txt",
            "normalization=",
        ),
        ("crates/oom-edit/assets/dict/PROVENANCE.txt", "url="),
        (
            "crates/oom-edit/assets/dict/PROVENANCE.txt",
            "archive-sha256=",
        ),
        (
            "crates/oom-edit/assets/dict/PROVENANCE.txt",
            "source-member=",
        ),
        (
            "crates/oom-edit/assets/dict/PROVENANCE.txt",
            "license-source=",
        ),
        (
            "crates/oom-edit/assets/dict/PROVENANCE.txt",
            "output-sha256=",
        ),
        ("crates/oom-edit/assets/dict/PROVENANCE.txt", "entry-count="),
        (
            "crates/oom-edit/assets/dict/MANIFEST.sha256",
            "5e7f675015d514fd87824230043751576559a8683d1e3aeb15229d0c8bad874f  en_US.txt\n",
        ),
        (
            "crates/oom-edit/assets/dict/MANIFEST.sha256",
            "77581170607b92e7479520a44f666033692d97a546a8165f35654b97771a3507  en_CA.txt\n",
        ),
        (
            "crates/oom-edit/assets/dict/MANIFEST.sha256",
            "bf808e578445dff4cc8c7af19272ac32544d1031f94423201d3be9b2fdf784b9  en_AU.txt\n",
        ),
    ];

    for (relative, needle) in mutations {
        let path = fixture.path().join(relative);
        let original = fs::read_to_string(&path).expect("fixture mutation target should be UTF-8");
        assert!(
            original.contains(needle),
            "mutation needle should exist in {relative}: {needle:?}"
        );
        fs::write(&path, original.replace(needle, "")).expect("mutated fixture should write");
        let output = data_license_check(&root, fixture.path());
        assert!(
            !output.status.success(),
            "data-license-check must fail after removing {needle:?} from {relative}"
        );
        fs::write(&path, original).expect("fixture should restore");
    }
}
