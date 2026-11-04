"""Link, anchor, path and reachability checks of tools/check_docs.py."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("check_docs", Path(__file__).resolve().parents[1] / "check_docs.py")
docs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(docs)


class SlugTests(unittest.TestCase):
    def test_slugs_follow_github_heading_anchors(self):
        cases = {
            "AVX-512F/VL, DQ, BW, CD and EVEX": "avx-512fvl-dq-bw-cd-and-evex",
            "`Pipeline` and the `--isel` flag": "pipeline-and-the---isel-flag",
            "Crates and data flow": "crates-and-data-flow",
            "Adding a [pass](optimizer.md)": "adding-a-pass",
            "**Bold** _words_ and snake_case": "bold-words-and-snake_case",
            "x0–x7 registers": "x0x7-registers",
        }
        for heading, expected in cases.items():
            with self.subTest(heading=heading):
                self.assertEqual(docs.slug(heading), expected)

    def test_repeated_headings_get_numbered_anchors(self):
        text = "# Tests\n## Tests\n<a id=\"legacy\"></a>\n```\n# Not a heading\n```\n"
        self.assertEqual(docs.anchors(text), {"tests", "tests-1", "legacy"})


class LineTests(unittest.TestCase):
    def test_links_ignore_code_spans_and_include_reference_definitions(self):
        self.assertEqual(docs.links("See [a](a.md#x) and `[b](b.md)`, ![c](c.png \"t\")"),
                         ["a.md#x", "c.png"])
        self.assertEqual(docs.links("[ref]: ../other.md"), ["../other.md"])

    def test_code_paths_skip_placeholders_and_strip_line_suffixes(self):
        line = ("`crates/a/src/lib.rs:12`, `docs/x.md#y`, `crates/<pkg>/tests`, "
                "`suite/*.py`, `tools/`, `cargo test`, `crates/a` and `target/x`")
        self.assertEqual(docs.code_paths(line),
                         ["crates/a/src/lib.rs", "docs/x.md", "tools", "crates/a"])


class RepositoryTests(unittest.TestCase):
    def repository(self, files: dict[str, str]) -> Path:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        for name, text in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(["git", "add", "-A"], cwd=root, check=True)
        return root

    def test_a_consistent_tree_has_no_problems(self):
        root = self.repository({
            ".gitignore": "/suite/reports/\n",
            "docs/README.md": "[Guide](guide/a.md#usage), [crate](../crates/c/README.md)\n",
            "docs/guide/a.md": "# A\n## Usage\nSee [top](#a) and `crates/c/src/lib.rs:3`.\n"
                               "Results go in `suite/reports/`.\n```\n[gone](gone.md)\n```\n",
            "crates/c/README.md": "Run `tools/run.py` from here.\n",
            "crates/c/tools/run.py": "",
            "crates/c/src/lib.rs": "",
            "crates/c/tests/fixture/README.md": "Not indexed.\n",
            "AGENTS.md": "Rules.\n",
        })
        self.assertEqual(docs.check(root), [])

    def test_broken_links_anchors_paths_and_orphans_are_reported(self):
        root = self.repository({
            "docs/README.md": "[A](a.md#missing) [B](b.md) [C](../../outside.md)\n",
            "docs/a.md": "# A\n`crates/none/lib.rs`\n",
            "docs/orphan.md": "# Orphan\n",
        })
        self.assertEqual(sorted(docs.check(root)), [
            "docs/README.md:1: link leaves the repository: ../../outside.md",
            "docs/README.md:1: missing anchor: a.md#missing",
            "docs/README.md:1: missing link target: b.md",
            "docs/a.md:2: missing path: crates/none/lib.rs",
            "docs/orphan.md:1: not reachable from docs/README.md",
        ])


if __name__ == "__main__":
    unittest.main()
