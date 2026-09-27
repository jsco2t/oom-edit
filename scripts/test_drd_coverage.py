import unittest
import tempfile
from pathlib import Path

import drd_coverage as coverage


ROOT = Path(__file__).resolve().parent.parent


class DrdCoverageTests(unittest.TestCase):
    def test_manifest_names_every_included_requirement(self):
        rows = coverage.load_manifest(ROOT / "coverage/edit_drd.tsv")
        coverage.validate_manifest(rows)
        self.assertEqual({row["requirement"] for row in rows}, coverage.EXPECTED_IDS)

    def test_missing_unknown_ignored_and_unjustified_exclusions_fail(self):
        rows = coverage.load_manifest(ROOT / "coverage/edit_drd.tsv")
        without_one = [row for row in rows if row["requirement"] != "FR-040"]
        with self.assertRaisesRegex(coverage.CoverageError, "missing"):
            coverage.validate_manifest(without_one)

        invented = [dict(row) for row in rows]
        invented.append(dict(invented[0], requirement="FR-999", case="EDIT-FR-999-test"))
        with self.assertRaisesRegex(coverage.CoverageError, "unknown"):
            coverage.validate_manifest(invented)

        ignored = [dict(row) for row in rows]
        ignored[0]["status"] = "ignored"
        with self.assertRaisesRegex(coverage.CoverageError, "status"):
            coverage.validate_manifest(ignored)

        excluded = [dict(row) for row in rows]
        excluded[0]["status"] = "excluded"
        with self.assertRaisesRegex(coverage.CoverageError, "exclusion"):
            coverage.validate_manifest(excluded)

    def test_final_gate_rejects_planned_cases(self):
        rows = coverage.load_manifest(ROOT / "coverage/edit_drd.tsv")
        coverage.validate_manifest(rows, final=True)
        rows = [dict(row) for row in rows]
        rows[0]["status"] = "planned"
        with self.assertRaisesRegex(coverage.CoverageError, "planned"):
            coverage.validate_manifest(rows, final=True)

    def test_locations_reject_missing_disabled_cases_and_unknown_targets(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Makefile").write_text("test-case: ## fixture\n\tcargo test --offline --locked\n")
            (root / "case.rs").write_text("#[test]\nfn works() { assert_eq!(1, 1); }\n#[test]\n#[ignore]\nfn ignored() { assert!(true); }\n")
            row = {"case": "case", "location": "case.rs::works", "target": "make test-case", "status": "covered"}
            coverage.validate_locations([row], root)
            coverage.validate_locations([row], root, {"tests::works"})
            with self.assertRaisesRegex(coverage.CoverageError, "compiled test inventory"):
                coverage.validate_locations([row], root, {"tests::another_case"})
            for changes in [{"location": "absent.rs::works"}, {"location": "case.rs::absent"}, {"location": "case.rs::ignored"}, {"target": "make invented"}, {"location": "../outside.rs::works"}]:
                with self.subTest(changes=changes):
                    with self.assertRaises(coverage.CoverageError):
                        coverage.validate_locations([dict(row, **changes)], root)

    def test_ignored_release_case_requires_an_exact_executing_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "case.rs").write_text("#[test]\n#[ignore]\nfn measured() { assert_eq!(2 + 2, 4); }\n")
            row = {"case": "release-case", "location": "case.rs::measured", "target": "make bench-case", "status": "covered"}
            (root / "Makefile").write_text("bench-case:\n\tcargo test --release measured -- --exact --ignored\n")
            coverage.validate_locations([row], root, {"measured"})
            for recipe in ["cargo test measured", "cargo test measured -- --ignored", "cargo test another -- --exact --ignored"]:
                (root / "Makefile").write_text(f"bench-case:\n\t{recipe}\n")
                with self.subTest(recipe=recipe):
                    with self.assertRaisesRegex(coverage.CoverageError, "ignored"):
                        coverage.validate_locations([row], root, {"measured"})

    def test_stubbed_rust_and_disabled_or_stubbed_python_cases_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Makefile").write_text("test-case:\n\tpython3 -m unittest\n")
            row = {"case": "case", "location": "case.py::works", "target": "make test-case", "status": "covered"}
            for source in ["def works():\n    pass\n", "def works():\n    return None\n", "import unittest\n@unittest.skip('unused')\ndef works():\n    assert 2 + 2 == 4\n"]:
                (root / "case.py").write_text(source)
                with self.subTest(source=source):
                    with self.assertRaises(coverage.CoverageError):
                        coverage.validate_locations([row], root)
            (root / "case.rs").write_text("#[test]\nfn empty() {}\n")
            with self.assertRaisesRegex(coverage.CoverageError, "stubbed"):
                coverage.validate_locations([dict(row, location="case.rs::empty")], root, {"empty"})

    def test_python_single_assertion_is_executable_but_skip_aliases_are_not(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Makefile").write_text("test-case:\n\tpython3 -m unittest\n")
            row = {"case": "case", "location": "case.py::works", "target": "make test-case", "status": "covered"}
            (root / "case.py").write_text("def works(self):\n    self.assertEqual(2 + 2, 4)\n")
            coverage.validate_locations([row], root)
            for source in [
                "import unittest\n@unittest.expectedFailure\ndef works(self):\n    self.assertEqual(2 + 2, 4)\n",
                "from unittest import skip as omit\n@omit('unused')\ndef works(self):\n    self.assertEqual(2 + 2, 4)\n",
            ]:
                (root / "case.py").write_text(source)
                with self.subTest(source=source):
                    with self.assertRaisesRegex(coverage.CoverageError, "disabled"):
                        coverage.validate_locations([row], root)
