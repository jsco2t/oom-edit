import copy
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from downstream import ORIGIN, PACKAGES, ROOT, assert_provenance, checked_revision, empty_directory, isolated_environment, manifest, normalize_generated_checksums, outside_workspace


class DownstreamTests(unittest.TestCase):
    def setUp(self):
        self.revision = "a" * 40
        self.packages = [
            {"name": name, "source": f"git+{ORIGIN}?rev={self.revision}#{self.revision}"}
            for name in PACKAGES
        ]

    def test_fr_112_independent_consumer_sources(self):
        assert_provenance(self.packages, self.revision)

    def test_fr_112_patch_provenance_negative(self):
        for name in PACKAGES:
            with self.subTest(name=name, failure="missing"):
                with self.assertRaisesRegex(ValueError, name):
                    assert_provenance([row for row in self.packages if row["name"] != name], self.revision)
            for bad_source in [None, "registry+https://github.com/rust-lang/crates.io-index", f"git+{ORIGIN}?rev={'b' * 40}#{'b' * 40}", f"git+https://example.invalid/other?rev={self.revision}#{self.revision}"]:
                with self.subTest(name=name, source=bad_source):
                    broken = copy.deepcopy(self.packages)
                    next(row for row in broken if row["name"] == name)["source"] = bad_source
                    with self.assertRaisesRegex(ValueError, name):
                        assert_provenance(broken, self.revision)
            with self.subTest(name=name, failure="duplicate"):
                with self.assertRaisesRegex(ValueError, name):
                    assert_provenance(self.packages + [next(row for row in self.packages if row["name"] == name)], self.revision)

    def test_manifest_pins_exactly_one_revision_for_every_source(self):
        import tomllib
        parsed = tomllib.loads(manifest(self.revision))
        expected = {"git": ORIGIN, "rev": self.revision}
        self.assertEqual(parsed["workspace"], {})
        self.assertEqual(parsed["dependencies"], {"oom-edit": expected})
        self.assertEqual(parsed["patch"]["crates-io"], {name: expected for name in PACKAGES[1:]})
        for revision in ["", "main", "v0.6.0", "a" * 39, "a" * 41, "z" * 40]:
            with self.assertRaises(ValueError):
                checked_revision(revision)

    def test_tag_consumption_requires_the_exact_tag_and_gated_revision(self):
        import tomllib
        tagged = [{"name": name, "source": f"git+{ORIGIN}?tag=v0.6.0#{self.revision}"} for name in PACKAGES]
        assert_provenance(tagged, self.revision, "v0.6.0")
        with self.assertRaises(ValueError):
            assert_provenance(tagged, self.revision)
        with self.assertRaises(ValueError):
            assert_provenance(tagged, "b" * 40, "v0.6.0")
        with self.assertRaises(ValueError):
            assert_provenance(tagged, self.revision, "v0.6.1")
        with self.assertRaises(ValueError):
            manifest(self.revision, "main")
        parsed = tomllib.loads(manifest(self.revision, "v0.6.0"))
        expected = {"git": ORIGIN, "tag": "v0.6.0"}
        self.assertEqual(parsed["dependencies"], {"oom-edit": expected})
        self.assertEqual(parsed["patch"]["crates-io"], {name: expected for name in PACKAGES[1:]})

    def test_never_overwrites_or_uses_the_workspace_as_a_fixture(self):
        for path in [ROOT, ROOT / "fixtures", "/"]:
            with self.assertRaises(ValueError):
                outside_workspace(path)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            protected = directory / "sentinel"
            protected.write_text("retained")
            with self.assertRaises(ValueError):
                empty_directory(directory)
            self.assertEqual(protected.read_text(), "retained")

    def test_clean_environment_drops_inherited_cargo_and_git_configuration(self):
        with patch.dict(os.environ, {"CARGO_HOME": "wrong", "CARGO_TARGET_DIR": "wrong", "CARGO_BUILD_RUSTFLAGS": "wrong", "GIT_CONFIG_COUNT": "5", "GIT_CONFIG_KEY_0": "wrong"}):
            environment = isolated_environment(Path("/tmp/fresh-home"), Path("/tmp/fixture-target"))
        self.assertEqual(environment["CARGO_HOME"], "/tmp/fresh-home")
        self.assertEqual(environment["CARGO_TARGET_DIR"], "/tmp/fixture-target")
        self.assertNotIn("CARGO_BUILD_RUSTFLAGS", environment)
        self.assertNotIn("GIT_CONFIG_COUNT", environment)
        self.assertNotIn("GIT_CONFIG_KEY_0", environment)
        self.assertEqual(environment["GIT_CONFIG_GLOBAL"], os.devnull)

    def test_self_reference_repair_preserves_all_real_source_hashes(self):
        with tempfile.TemporaryDirectory() as temporary:
            vendor = Path(temporary)
            crate = vendor / "example"
            crate.mkdir()
            path = crate / ".cargo-checksum.json"
            original = {"package": "registry-package-hash", "files": {".cargo-checksum.json": "old-metadata-hash", "src/lib.rs": "real-source-hash", "Cargo.toml": "manifest-hash"}}
            path.write_text(json.dumps(original))
            normalize_generated_checksums(vendor)
            expected = copy.deepcopy(original)
            del expected["files"][".cargo-checksum.json"]
            self.assertEqual(json.loads(path.read_text()), expected)
            before = path.read_bytes()
            normalize_generated_checksums(vendor)
            self.assertEqual(path.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
