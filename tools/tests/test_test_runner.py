"""Selection safety and Cargo/nextest command parity."""
import importlib.util
import contextlib
import io
from pathlib import Path
import unittest
import tempfile

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

    def test_named_components_are_independent_by_default(self):
        packages = {
            "keel-asm-ir": {"dependencies": []},
            "keel-asm": {"dependencies": [{"name": "keel-asm-ir"}]},
            "keel-codegen": {"dependencies": [{"name": "keel-asm"}]},
            "keel": {"dependencies": [{"name": "keel-codegen"}]},
            "keel-link": {"dependencies": []},
        }
        direct = {"keel-asm-ir", "keel-asm"}
        self.assertEqual(runner.component_packages(["assembler"], packages), direct)
        self.assertEqual(runner.component_packages(["assembler", "linker"], packages), direct | {"keel-link"})
        self.assertEqual(runner.component_packages(["assembler"], packages, True),
                         direct | {"keel-codegen", "keel"})
        self.assertEqual(runner.component_packages(["full"], packages), set(packages))

    def test_no_arguments_select_full_workspace(self):
        args = runner.arguments([])
        self.assertEqual(args.scopes, ["full"])
        self.assertFalse(args.consumers)

    def test_invalid_combinations_fail_before_running_tests(self):
        for args in [["typo"], ["full", "linker"], ["--consumers"],
                     ["linker", "--changed", "main"],
                     ["linker", "--unit", "--test", "archive"]]:
            with self.subTest(args=args), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    runner.arguments(args)
                self.assertEqual(error.exception.code, 2)

    def test_multiple_components_and_narrow_targets_parse(self):
        args = runner.arguments(["assembler", "linker", "--test", "archive", "--quick"])
        self.assertEqual(args.scopes, ["assembler", "linker"])
        self.assertEqual(args.test, ["archive"])
        self.assertTrue(args.quick)

    def test_explicit_targets_skip_doctests_in_both_runners(self):
        for backend in ["cargo", "nextest"]:
            with self.subTest(backend=backend):
                unit = runner.commands("cargo", backend, "test-fast", ["ir"], False, unit=True)
                self.assertEqual(len(unit), 1)
                self.assertIn("--lib", unit[0])
                targets = runner.commands("cargo", backend, "test-fast", ["ir"], True,
                                          targets=["parse", "format"])
                self.assertEqual(len(targets), 1)
                self.assertEqual(targets[0].count("--test"), 2)
                self.assertNotIn("--tests", targets[0])
                self.assertNotIn("--lib", targets[0])

    def test_codegen_quick_runs_units_and_core_contracts_in_both_runners(self):
        for backend in ["cargo", "nextest"]:
            with self.subTest(backend=backend):
                plan = runner.commands("cargo", backend, "test-fast", ["keel-codegen"], True)
                self.assertEqual(len(plan), 1)
                command = plan[0]
                self.assertIn("--lib", command)
                targets = [command[i + 1] for i, arg in enumerate(command) if arg == "--test"]
                self.assertIn("selection", targets)
                self.assertIn("x86_features", targets)
                self.assertNotIn("arm_constant_division", targets)
                self.assertNotIn("run", targets)

    def test_codegen_quick_does_not_narrow_an_explicit_target(self):
        for backend in ["cargo", "nextest"]:
            command = runner.commands("cargo", backend, "test-fast", ["keel-codegen"], True,
                                      targets=["arm_constant_division"])[0]
            self.assertNotIn("--lib", command)
            self.assertEqual(command.count("--test"), 1)
            self.assertIn("arm_constant_division", command)

    def test_mixed_quick_selection_preserves_other_packages_without_duplication(self):
        for backend in ["cargo", "nextest"]:
            plan = runner.commands("cargo", backend, "test-fast",
                                   ["keel-codegen", "keel-ir"], True)
            self.assertEqual(len(plan), 2)
            selected = [[command[i + 1] for i, arg in enumerate(command) if arg == "-p"]
                        for command in plan]
            self.assertEqual(selected, [["keel-ir"], ["keel-codegen"]])
            self.assertNotIn("--test", plan[0])
            self.assertIn("--test", plan[1])

    def test_full_codegen_still_includes_all_integration_targets_and_doctests(self):
        for backend in ["cargo", "nextest"]:
            plan = runner.commands("cargo", backend, "test-fast", ["keel-codegen"], False)
            self.assertNotIn("--test", plan[0])
            self.assertNotIn("--lib", plan[0])
            if backend == "nextest":
                self.assertIn("--doc", plan[1])

    def test_source_change_includes_transitive_consumers(self):
        self.assertEqual(runner.changed_packages(["crates/ir/src/lib.rs"], self.packages),
                         {"ir", "codegen", "cli"})

    def test_test_only_change_stays_in_owner(self):
        self.assertEqual(runner.changed_packages(["crates/ir/tests/smoke.rs"], self.packages), {"ir"})

    def test_shared_fixture_change_broadens_beyond_cargo_consumers(self):
        for path in ["crates/ir/tests/fixtures/input.c", "crates/ir/tests/common/helper.rs"]:
            with self.subTest(path=path):
                self.assertEqual(runner.changed_packages([path], self.packages), set(self.packages))

    def test_static_fixture_edges_include_cross_crate_readers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_sources(root, {
                "crates/codegen/tests/cross.rs": 'const INPUT: &str = include_str!("../../ir/tests/fixtures/input.c");',
                "crates/unrelated/tests/cross.rs": 'const INPUT: &[u8] = include_bytes!(r#"../../ir/tests/fixtures/input.c"#);',
            })
            self.assertEqual(runner.changed_packages(["crates/ir/tests/fixtures/input.c"], self.packages, root),
                             {"ir", "codegen", "unrelated"})

    def test_include_macro_delimiters_index_literal_inputs(self):
        for macro in ["include", "include_str", "include_bytes"]:
            for opening, closing in [("(", ")"), ("{", "}"), ("[", "]")]:
                with self.subTest(macro=macro, opening=opening), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    self.write_sources(root, {
                        "crates/codegen/tests/cross.rs": f'{macro}!{opening}"../../ir/tests/fixtures/input.c"{closing};',
                    })
                    self.assertEqual(runner.changed_packages(["crates/ir/tests/fixtures/input.c"], self.packages, root),
                                     {"ir", "codegen"})

    def test_dynamic_include_macro_delimiters_keep_workspace_fallback(self):
        for macro in ["include", "include_str", "include_bytes"]:
            for opening, closing in [("(", ")"), ("{", "}"), ("[", "]")]:
                with self.subTest(macro=macro, opening=opening), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    self.write_sources(root, {
                        "crates/codegen/tests/static.rs": 'include_str!("../../ir/tests/fixtures/input.c");',
                        "crates/unrelated/tests/dynamic.rs": f'{macro}!{opening}PATH{closing};',
                    })
                    self.assertEqual(runner.changed_packages(["crates/ir/tests/fixtures/input.c"], self.packages, root),
                                     set(self.packages))

    def test_library_inputs_include_transitive_cargo_consumers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_sources(root, {
                "crates/codegen/src/lib.rs": '#[path = "../../ir/tests/common/helper.rs"] mod helper;',
                "crates/ir/tests/common/helper.rs": 'const INPUT: &str = include_str!("../fixtures/input.c");',
            })
            self.assertEqual(runner.changed_packages(["crates/ir/tests/fixtures/input.c"], self.packages, root),
                             {"ir", "codegen", "cli"})

    def test_cross_package_source_modules_include_their_unindexed_children(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_sources(root, {
                "crates/unrelated/src/lib.rs": '#[path = "../../ir/src/shared.rs"] mod shared;',
                "crates/ir/src/shared.rs": 'mod child;',
                "crates/ir/src/shared/child.rs": 'pub fn value() {}',
            })
            self.assertEqual(runner.changed_packages(["crates/ir/src/shared/child.rs"], self.packages, root),
                             {"ir", "codegen", "cli", "unrelated"})

    def test_path_attributes_cover_inline_module_bases_and_deleted_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_sources(root, {
                "crates/codegen/tests/cross.rs": 'mod nested { #[path = "../../../ir/tests/common/helper.rs"] mod helper; }',
            })
            # The deleted helper need not exist to retain its consuming edge.
            self.assertEqual(runner.changed_packages(["crates/ir/tests/common/helper.rs"], self.packages, root),
                             {"ir", "codegen"})

    def test_dynamic_inputs_keep_full_workspace_fallback(self):
        for expression in ['include_str!(concat!("../../ir/tests/fixtures/", name))',
                           'include_bytes!(PATH)', '#[path = module_path!()] mod helper;',
                           '#[cfg_attr(feature = "x", path = "helper.rs")] mod helper;']:
            with self.subTest(expression=expression), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.write_sources(root, {
                    "crates/codegen/tests/cross.rs": expression,
                    "crates/codegen/tests/static.rs": 'include_str!("../../ir/tests/fixtures/input.c");',
                })
                self.assertEqual(runner.changed_packages(["crates/ir/tests/fixtures/input.c"], self.packages, root),
                                 set(self.packages))

    def test_macro_generated_include_calls_keep_workspace_fallback(self):
        expressions = [
            'macro_rules! load { ($path:literal) => { include_str!{$path} }; }',
            'macro_rules! load { ($reader:ident) => { $reader!("input.c") }; } load!(include_str);',
            'macro_rules! load { ($reader:ident) => { $reader!("input.c") }; } load![include_str];',
            'use std::include_bytes as load; load![PATH];',
            'macro_rules! load { ($reader:ident) => { $reader!(PATH) }; } load!(\n include\n);',
            'concat_idents!(include_, str)!(PATH);',
            'macro_rules! load { () => { ${concat(include_, str)}!(PATH) }; }',
        ]
        for expression in expressions:
            with self.subTest(expression=expression), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.write_sources(root, {
                    "crates/codegen/tests/static.rs": 'include_str!("../../ir/tests/fixtures/input.c");',
                    "crates/unrelated/tests/generated.rs": expression,
                })
                self.assertEqual(runner.changed_packages(["crates/ir/tests/fixtures/input.c"], self.packages, root),
                                 set(self.packages))

    def test_include_names_in_strings_comments_and_characters_are_not_dynamic_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_sources(root, {
                "crates/codegen/tests/static.rs": 'include_str!("../../ir/tests/fixtures/input.c");',
                "crates/unrelated/tests/strings.rs": "const QUOTE: char = '\"';\n"
                    'const HELP: &str = r#"load!(include); use std::include_str as x;"#;\n'
                    '// load!(include_str);\n',
            })
            self.assertEqual(runner.changed_packages(["crates/ir/tests/fixtures/input.c"], self.packages, root),
                             {"ir", "codegen"})

    def test_generated_output_does_not_hide_static_fixture_edges(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_sources(root, {
                "crates/codegen/src/lib.rs": 'include!(concat!(env!("OUT_DIR"), "/generated.rs"));',
                "crates/codegen/tests/cross.rs": 'include_str!("../../ir/tests/fixtures/input.c");',
            })
            self.assertEqual(runner.changed_packages(["crates/ir/tests/fixtures/input.c"], self.packages, root),
                             {"ir", "codegen"})

    @staticmethod
    def write_sources(root, sources):
        for name, text in sources.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)

    def test_unknown_and_deleted_paths_broaden(self):
        for path in ["Cargo.lock", "tools/generate.py", "crates/deleted/src/lib.rs"]:
            with self.subTest(path=path):
                self.assertEqual(runner.changed_packages([path], self.packages), set(self.packages))

    def test_empty_change_selects_nothing(self):
        self.assertEqual(runner.changed_packages([], self.packages), set())

    def test_dependency_cycles_terminate(self):
        self.packages["ir"]["dependencies"] = [{"name": "cli"}]
        self.assertEqual(runner.consumers({"ir"}, self.packages), {"ir", "codegen", "cli"})

    def test_full_nextest_adds_applicable_doctests(self):
        tests, docs = runner.commands("cargo", "nextest", "test-fast", ["ir"], False)
        self.assertIn("--cargo-profile", tests)
        self.assertIn("--doc", docs)
        self.assertNotIn("-E", tests)
        quick = runner.commands("cargo", "nextest", "test-fast", ["ir"], True)[0]
        self.assertIn("not test(exhaustive_)", quick)

    def test_binary_only_and_mixed_selections_do_not_request_binary_doctests(self):
        binary = runner.commands("cargo", "nextest", "test-fast", ["cli"], False,
                                 doctest_packages=[])
        self.assertEqual(len(binary), 1)
        tests, docs = runner.commands("cargo", "nextest", "test-fast", ["cli", "ir"], False,
                                      doctest_packages=["ir"])
        self.assertIn("cli", tests)
        self.assertNotIn("cli", docs)
        self.assertEqual(docs[-2:], ["-p", "ir"])

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
