"""Selection safety and Cargo/nextest command parity."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("test_runner", Path(__file__).parents[1] / "test.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class SelectionTests(unittest.TestCase):
    def setUp(self):
        self.packages = {
            "ir": {"directory": "crates/ir", "dependencies": []},
            "codegen": {"directory": "crates/codegen", "dependencies": [{"name": "ir"}]},
            "cli": {"directory": "crates/cli", "dependencies": [{"name": "codegen"}]},
            "unrelated": {"directory": "crates/unrelated", "dependencies": []},
        }

    def test_source_change_includes_transitive_consumers(self):
        self.assertEqual(runner.changed_packages(["crates/ir/src/lib.rs"], self.packages),
                         {"ir", "codegen", "cli"})

    def test_test_only_change_stays_in_owner(self):
        self.assertEqual(runner.changed_packages(["crates/ir/tests/smoke.rs"], self.packages), {"ir"})

    def test_shared_fixture_change_broadens_beyond_cargo_consumers(self):
        for path in ["crates/ir/tests/fixtures/input.c", "crates/ir/tests/common/helper.rs"]:
            with self.subTest(path=path):
                self.assertEqual(runner.changed_packages([path], self.packages), set(self.packages))

    def test_unknown_and_deleted_paths_broaden(self):
        for path in ["Cargo.lock", "tools/generate.py", "crates/deleted/src/lib.rs"]:
            with self.subTest(path=path):
                self.assertEqual(runner.changed_packages([path], self.packages), set(self.packages))

    def test_empty_change_selects_nothing(self):
        self.assertEqual(runner.changed_packages([], self.packages), set())

    def test_dependency_cycles_terminate(self):
        self.packages["ir"]["dependencies"] = [{"name": "cli"}]
        self.assertEqual(runner.consumers({"ir"}, self.packages), {"ir", "codegen", "cli"})

    def test_full_nextest_always_adds_doctests(self):
        tests, docs = runner.commands("cargo", "nextest", "test-fast", ["ir"], False)
        self.assertIn("--cargo-profile", tests)
        self.assertIn("--doc", docs)
        self.assertNotIn("-E", tests)
        quick = runner.commands("cargo", "nextest", "test-fast", ["ir"], True)[0]
        self.assertIn("not test(exhaustive_)", quick)

    def test_quick_is_explicit_for_both_runners(self):
        for backend in ["cargo", "nextest"]:
            with self.subTest(backend=backend):
                commands = runner.commands("cargo", backend, "test-fast", ["ir"], True)
                self.assertEqual(len(commands), 1)
                self.assertNotIn("--doc", commands[0])
                self.assertTrue(any("exhaustive_" in arg for arg in commands[0]))

    def test_cargo_full_keeps_default_tests_and_doctests(self):
        commands = runner.commands("cargo", "cargo", "test-fast", ["ir"], False)
        self.assertEqual(commands, [["cargo", "test", "--profile", "test-fast", "--no-fail-fast", "-p", "ir"]])


if __name__ == "__main__":
    unittest.main()
