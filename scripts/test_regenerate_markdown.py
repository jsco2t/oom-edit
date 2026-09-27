import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import regenerate_markdown


PARSER = '''#define LANGUAGE_VERSION 15
#ifdef _MSC_VER
#pragma optimize("", off)
#elif defined(__clang__)
#pragma clang optimize off
#elif defined(__GNUC__)
#pragma GCC optimize ("O0")
#endif
static const int grammar_table = 42;
'''


class RegenerationTests(unittest.TestCase):
    def test_guards_only_optimization_directives(self):
        guarded = regenerate_markdown.guard_generated_parser(PARSER)
        self.assertIn("#ifndef TREE_SITTER_MD_OPTIMIZED_BUILD\n#ifdef _MSC_VER", guarded)
        self.assertIn('#pragma GCC optimize ("O0")\n#endif\n#endif', guarded)
        self.assertTrue(guarded.endswith("static const int grammar_table = 42;\n"))
        self.assertEqual(guarded.count("TREE_SITTER_MD_OPTIMIZED_BUILD"), 1)

    def test_rejects_abi_or_directive_drift(self):
        for parser in [PARSER.replace("VERSION 15", "VERSION 14"), PARSER + PARSER,
                       PARSER.replace('optimize ("O0")', 'optimize ("O1")')]:
            with self.subTest(parser=parser), self.assertRaises(ValueError):
                regenerate_markdown.guard_generated_parser(parser)

    def test_rejects_other_generator_versions_before_writing(self):
        with patch.object(subprocess, "check_output", return_value="tree-sitter 0.26.2\n"), \
                patch.object(subprocess, "run") as generate:
            with self.assertRaisesRegex(ValueError, "exactly tree-sitter 0.26.3"):
                regenerate_markdown.regenerate("generator", "/unused", "/unused")
            generate.assert_not_called()

    def test_preserves_pinned_headers_and_default_extensions(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            headers = output / "tree_sitter"
            headers.mkdir()
            for name in ["array.h", "alloc.h", "parser.h"]:
                (headers / name).write_bytes(b"pinned\x00header")

            def generate(command, **kwargs):
                self.assertEqual(command, ["generator", "generate", "--abi", "15", "--output", str(output.resolve())])
                self.assertEqual(kwargs["cwd"], "grammar")
                self.assertTrue(kwargs["check"])
                self.assertFalse(any(key.startswith("EXTENSION_") for key in kwargs["env"]))
                self.assertNotIn("NO_DEFAULT_EXTENSIONS", kwargs["env"])
                self.assertNotIn("ALL_EXTENSIONS", kwargs["env"])
                for header in headers.iterdir():
                    header.write_bytes(b"new generator header")
                (output / "parser.c").write_text(PARSER, encoding="utf-8")

            with patch.dict(os.environ, {"EXTENSION_WIKI_LINK": "1", "NO_DEFAULT_EXTENSIONS": "1", "ALL_EXTENSIONS": "1"}), \
                    patch.object(subprocess, "check_output", return_value="tree-sitter 0.26.3\n"), \
                    patch.object(subprocess, "run", side_effect=generate):
                regenerate_markdown.regenerate("generator", output, "grammar")
            self.assertEqual((output / "parser.c").read_text(), regenerate_markdown.guard_generated_parser(PARSER))
            for header in headers.iterdir():
                self.assertEqual(header.read_bytes(), b"pinned\x00header")

    def test_resolves_symlinked_output_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "generated"
            output.mkdir()
            alias = Path(temporary) / "alias"
            alias.symlink_to(output, target_is_directory=True)

            def generate(command, **kwargs):
                self.assertEqual(command[-2:], ["--output", str(output.resolve())])
                (output / "parser.c").write_text(PARSER, encoding="utf-8")

            with patch.object(subprocess, "check_output", return_value="tree-sitter 0.26.3\n"), \
                    patch.object(subprocess, "run", side_effect=generate):
                regenerate_markdown.regenerate("generator", alias, "grammar")
            self.assertEqual((alias / "parser.c").read_text(), regenerate_markdown.guard_generated_parser(PARSER))

    def test_restores_headers_even_when_generation_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            header = output / "tree_sitter" / "parser.h"
            header.parent.mkdir()
            header.write_bytes(b"pinned")

            def fail(*args, **kwargs):
                header.write_bytes(b"partial replacement")
                raise subprocess.CalledProcessError(1, args[0])

            with patch.object(subprocess, "check_output", return_value="tree-sitter 0.26.3\n"), \
                    patch.object(subprocess, "run", side_effect=fail):
                with self.assertRaises(subprocess.CalledProcessError):
                    regenerate_markdown.regenerate("generator", output, "grammar")
            self.assertEqual(header.read_bytes(), b"pinned")


if __name__ == "__main__":
    unittest.main()
