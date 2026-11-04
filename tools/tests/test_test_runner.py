"""Selection safety and Cargo/nextest command parity."""
import importlib.util
import contextlib
import io
from pathlib import Path
import shutil
import unittest
from unittest import mock
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

    def test_unread_test_files_belong_to_their_package(self):
        for path in ["crates/ir/tests/fixtures/input.c", "crates/ir/tests/common/mod.rs",
                     "crates/ir/tests/golden/fold.keel", "crates/ir/tests/runner.py"]:
            with self.subTest(path=path):
                self.assertEqual(runner.changed_packages([path], self.packages), {"ir"})

    def test_literal_paths_select_runtime_fixture_readers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_sources(root, {
                "crates/codegen/tests/corpus.rs":
                    'let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ir/tests/programs");',
                "crates/unrelated/src/lib.rs": 'const DATA: &str = "crates/ir/tests/data/table.txt";',
                "crates/cli/tests/words.rs": 'const TEXT: &str = "not a path";',
            })
            select = lambda path: runner.changed_packages([path], self.packages, root)
            self.assertEqual(select("crates/ir/tests/programs/loop.c"), {"ir", "codegen"})
            self.assertEqual(select("crates/ir/tests/data/table.txt"), {"ir", "unrelated"})
            self.assertEqual(select("crates/ir/tests/other/input.c"), {"ir"})

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
        for path in ["Cargo.lock", ".cargo/config.toml", "third-party/lib/src/lib.rs",
                     "crates/deleted/src/lib.rs"]:
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


class ConsumerAndPolicyTests(unittest.TestCase):
    def setUp(self):
        # asm's tests use codegen; link's tests use asm, built without asm's dev-dependencies.
        self.packages = {
            "keel-ir": {"directory": "crates/keel-ir", "dependencies": []},
            "asm": {"directory": "crates/asm", "dependencies": [{"name": "codegen", "kind": "dev"}]},
            "codegen": {"directory": "crates/codegen",
                        "dependencies": [{"name": "asm", "kind": None}, {"name": "keel-ir", "kind": None}]},
            "cli": {"directory": "crates/cli", "dependencies": [{"name": "codegen", "kind": None}]},
            "link": {"directory": "crates/link", "dependencies": [{"name": "asm", "kind": "dev"}]},
            "build-user": {"directory": "crates/build-user",
                           "dependencies": [{"name": "cli", "kind": "build"}]},
        }

    def test_consumers_follow_normal_edges_then_one_dev_hop(self):
        self.assertEqual(runner.consumers({"codegen"}, self.packages),
                         {"codegen", "cli", "build-user", "asm"})
        self.assertEqual(runner.consumers({"asm"}, self.packages),
                         {"asm", "codegen", "cli", "build-user", "link"})

    def test_documentation_selects_nothing_unless_a_source_reads_it(self):
        for path in ["docs/compiler/optimizer.md", "README.md", "suite/README.md",
                     "crates/codegen/tests/fixtures/README.md", "docs/assets/chart.svg"]:
            with self.subTest(path=path):
                self.assertEqual(runner.changed_selection([path], self.packages), (set(), set()))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            SelectionTests.write_sources(root, {
                "crates/codegen/src/lib.rs": '#![doc = include_str!("../README.md")]',
                "crates/asm/tests/ledger.rs": 'include_str!("../../../docs/ledger.json");',
            })
            self.assertEqual(runner.changed_packages(["crates/codegen/README.md"], self.packages, root),
                             {"codegen", "cli", "build-user", "asm"})
            self.assertEqual(runner.changed_packages(["docs/ledger.json"], self.packages, root), {"asm"})

    def test_policy_paths_select_their_checks(self):
        cases = {
            "suite/ir/llvm_import.py": {"suite ir"},
            "suite/c/lua/suite.toml": {"suite c"},
            "suite/common/harness.py": {"suite"},
            "suite/configs.toml": {"suite"},
            "tools/x86-isa/generate.py": {"generators"},
            "tools/x86-isa/feature-policy.json": {"generators"},
            "crates/keel-asm/tools/generate.py": {"generators"},
            "crates/keel-asm/isa/a64.json": {"generators"},
            "tools/test.py": {"tools"},
            "tools/tests/test_test_runner.py": {"tools"},
            "crates/keel-cranelift/src/lib.rs": {"cranelift"},
        }
        for path, checks in cases.items():
            with self.subTest(path=path), tempfile.TemporaryDirectory() as directory:
                self.assertEqual(runner.changed_selection([path], self.packages, Path(directory)),
                                 (set(), checks))

    def test_generator_inputs_select_their_rust_readers(self):
        packages = {**self.packages, "keel-asm": {"directory": "crates/keel-asm", "dependencies": []}}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            SelectionTests.write_sources(root, {
                "crates/codegen/tests/neon.rs": 'include_str!("../../keel-asm/isa/neon.txt");',
            })
            self.assertEqual(runner.changed_selection(["crates/keel-asm/isa/neon.txt"], packages, root),
                             ({"codegen"}, {"generators"}))

    def test_keel_ir_changes_and_cranelift_readers_select_cranelift(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            SelectionTests.write_sources(root, {
                "crates/keel-cranelift/tests/translate.rs": 'include_str!("../../codegen/tests/fixtures/core.keel");',
            })
            select = lambda path: runner.changed_selection([path], self.packages, root)
            self.assertEqual(select("crates/keel-ir/src/lib.rs")[1], {"cranelift"})
            self.assertEqual(select("crates/keel-ir/tests/golden.rs")[1], set())
            self.assertEqual(select("crates/cli/src/main.rs")[1], set())
            self.assertEqual(select("crates/codegen/tests/fixtures/core.keel"), ({"codegen"}, {"cranelift"}))


@unittest.skipUnless(shutil.which("cargo") and (runner.ROOT / ".git").exists(), "needs cargo and the repository")
class RepositorySelectionTests(unittest.TestCase):
    """Selections on this repository's packages and sources."""

    @classmethod
    def setUpClass(cls):
        cls.packages = runner.workspace_packages("cargo")
        cached = runner.static_inputs(cls.packages, runner.ROOT)
        cls.patch = mock.patch.object(runner, "static_inputs", lambda *_: cached)
        cls.patch.start()

    @classmethod
    def tearDownClass(cls):
        cls.patch.stop()

    def select(self, *paths):
        return runner.changed_selection(list(paths), self.packages)

    def test_revert_1550bde0_selects_codegen_opt_and_their_consumers(self):
        # git show --stat 1550bde0: codegen and opt sources and tests, plus two docs pages.
        paths = [
            "crates/keel-codegen/src/kinds.rs", "crates/keel-codegen/src/select.rs",
            "crates/keel-codegen/src/x86_64.rs", "crates/keel-codegen/tests/x86_scalar_selection.rs",
            "crates/keel-codegen/tests/x86_scalar_selection/x86_integer_idioms.rs",
            "crates/keel-opt/src/instcombine.rs", "crates/keel-opt/tests/instcombine.rs",
            "crates/keel-opt/tests/instcombine/negation.rs", "crates/keel-opt/tests/loop_vectorize.rs",
            "docs/compiler/optimizer.md", "docs/targets/x86_64/codegen.md",
        ]
        packages, checks = self.select(*paths)
        self.assertEqual(packages, {"keel-codegen", "keel-opt", "keel", "keel-c", "keel-asm"})
        self.assertEqual(checks, set())
        self.assertLess(len(packages), len(self.packages))

    def test_common_edits_select_narrow_sets(self):
        cases = {
            "docs/compiler/optimizer.md": (set(), set()),
            "suite/README.md": (set(), set()),
            "tools/x86-isa/generate.py": (set(), {"generators"}),
            "crates/keel-opt/tests/golden/const_fold.keel": ({"keel-opt"}, set()),
            # keel-asm's compiler coverage test reads keel-c's program directory.
            "crates/keel-c/tests/programs/aggregate_snapshot.c": ({"keel-c", "keel-asm"}, set()),
            "crates/keel-asm/tests/common/mod.rs": ({"keel-asm"}, set()),
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                self.assertEqual(self.select(path), expected)
        # keel-suite names the suite/ir group; keel-llvm's parity test runs the Python importer.
        self.assertEqual(self.select("suite/ir/llvm_import.py"), ({"keel-suite", "keel-llvm"}, {"suite ir"}))


class OptimizerQuickTests(unittest.TestCase):
    @staticmethod
    def batches(plan):
        return [([c[i + 1] for i, a in enumerate(c) if a == "-p"],
                 [c[i + 1] for i, a in enumerate(c) if a == "--test"]) for c in plan]

    def test_optimizer_quick_runs_both_oracles_and_end_to_end_targets(self):
        for backend in ["cargo", "nextest"]:
            with self.subTest(backend=backend):
                batches = self.batches(runner.commands("cargo", backend, "test-fast", ["keel-opt"], True))
                self.assertEqual(batches, [(["keel-opt"], []),
                                           (["keel"], list(runner.OPTIMIZER_E2E_TARGETS)),
                                           (["keel-c"], ["run"]), (["keel-codegen"], ["run"])])

    def test_selected_consumers_absorb_the_extra_targets(self):
        batches = self.batches(runner.commands("cargo", "nextest", "test-fast",
                                               ["keel", "keel-c", "keel-codegen", "keel-opt"], True))
        self.assertEqual(batches[0], (["keel", "keel-c", "keel-opt"], []))
        packages, targets = batches[1]
        self.assertEqual(packages, ["keel-codegen"])
        self.assertEqual(targets, [*runner.CODEGEN_CORE_TARGETS, "run"])
        self.assertEqual(len(batches), 2)

    def test_optimizer_targets_exist_and_named_or_full_runs_add_nothing(self):
        for target in runner.OPTIMIZER_E2E_TARGETS:
            self.assertTrue((runner.ROOT / "crates/keel/tests" / f"{target}.rs").is_file(), target)
        for kwargs in [{"unit": True}, {"targets": ["golden"]}]:
            self.assertEqual(len(runner.commands("cargo", "nextest", "test-fast", ["keel-opt"], True, **kwargs)), 1)
        self.assertEqual(self.batches(runner.commands("cargo", "cargo", "test-fast", ["keel-opt"], False)),
                         [(["keel-opt"], [])])


class EnvironmentAndSkipTests(unittest.TestCase):
    def test_libllvm_default_applies_only_to_apple_silicon(self):
        self.assertIsNone(runner.default_libllvm("Linux", "x86_64"))
        with mock.patch.object(runner.subprocess, "run",
                               return_value=mock.Mock(stdout="/missing/sysroot\n")):
            self.assertIsNone(runner.default_libllvm("Darwin", "arm64"))
        with tempfile.TemporaryDirectory() as sysroot:
            (Path(sysroot) / "lib").mkdir()
            (Path(sysroot) / "lib/libLLVM.dylib").write_bytes(b"")
            with mock.patch.object(runner.subprocess, "run", return_value=mock.Mock(stdout=sysroot + "\n")):
                self.assertEqual(runner.default_libllvm("Darwin", "arm64"), Path(sysroot) / "lib/libLLVM.dylib")

    def test_keel_executable_comes_from_the_bin_artifact(self):
        messages = "\n".join([
            "warning text",
            '{"reason":"compiler-artifact","target":{"name":"keel_ir"},"executable":null}',
            '{"reason":"compiler-artifact","target":{"name":"keel"},"executable":"/t/test-fast/keel"}',
            '{"reason":"build-finished","success":true}',
        ])
        self.assertEqual(runner.keel_executable(messages), "/t/test-fast/keel")
        self.assertIsNone(runner.keel_executable('{"reason":"build-finished"}'))

    def test_skip_summary_groups_by_reason(self):
        log = ("keel-asm::x86::gnu_matches\tGNU as not found\n"
               "keel-llvm::import::flags\tKEEL_TEST_LIBLLVM is not set\n"
               "keel-asm::x86::gnu_relocations\tGNU as not found\n"
               "keel-c::odd_line\n")
        self.assertEqual(runner.skip_summary(log), [
            "Skipped tests: 4",
            "  2: GNU as not found",
            "      keel-asm::x86::gnu_matches",
            "      keel-asm::x86::gnu_relocations",
            "  1: KEEL_TEST_LIBLLVM is not set",
            "      keel-llvm::import::flags",
            "  1: no reason given",
            "      keel-c::odd_line",
        ])
        self.assertEqual(runner.skip_summary(""), ["Skipped tests: none recorded"])
        many = "".join(f"keel::t{i}\tno clang\n" for i in range(runner.SKIP_SUMMARY_TESTS + 3))
        self.assertEqual(runner.skip_summary(many)[-1], "      ... and 3 more")

    def test_each_run_gets_a_fresh_skip_log_that_is_summarized_and_removed(self):
        logs = []

        def run(command, cwd, env):
            logs.append(env["KEEL_SKIP_LOG"])
            if command[-1] == "skips":
                with open(env["KEEL_SKIP_LOG"], "a") as log:
                    log.write("keel-asm::numeric::sme\tSME is not available\n")
            return mock.Mock(returncode=0)

        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report.json"
            args = runner.arguments(["ir", "--report", str(report)])
            for command, expected in [(["cargo", "skips"], "  1: SME is not available"),
                                      (["cargo", "quiet"], "Skipped tests: none recorded")]:
                output = io.StringIO()
                with mock.patch.object(runner.subprocess, "run", run), contextlib.redirect_stdout(output):
                    self.assertEqual(runner.run_plan(args, [("tests", command)], {}, True), 0)
                self.assertIn(expected, output.getvalue())
                self.assertFalse(Path(logs[-1]).exists())
            self.assertNotEqual(logs[0], logs[1])
            self.assertEqual(runner.json.loads(report.read_text())["skips"], [])

    def test_check_commands_for_changed_paths(self):
        plan = dict(runner.check_commands(["tools", "suite ir", "suite c", "cranelift"], "cargo", "python3", []))
        self.assertEqual(plan["tools unit tests"][-4:], ["-s", "tools/tests", "-p", "test_*.py"])
        self.assertEqual(plan["suite runner tests"], ["cargo", "suite", "check", "c", "ir"])
        self.assertEqual(plan["keel-cranelift tests"][:4],
                         ["cargo", "test", "--manifest-path", "crates/keel-cranelift/Cargo.toml"])
        self.assertIn("--locked", plan["keel-cranelift tests"])
        every = dict(runner.check_commands(["suite", "suite ir"], "cargo", "python3", []))
        self.assertEqual(every["suite runner tests"], ["cargo", "suite", "check"])


class CheckScopeTests(unittest.TestCase):
    packages = {
        "cli": {"features": {"default": ["extra"], "extra": [], "verify-each": ["opt/verify-each"],
                             "profile-with-tracy": ["dep:tracy-client"],
                             "trace-all": ["codegen/profile-with-tracy"]}},
        "opt": {"features": {"verify-each": []}},
        "plain": {"features": {}},
    }

    def test_lint_features_skip_tracy_and_features_that_enable_it(self):
        self.assertEqual(runner.lint_features(self.packages),
                         ["cli/extra", "cli/verify-each", "opt/verify-each"])

    def test_lint_runs_format_and_both_clippy_passes_with_denied_warnings(self):
        plan = runner.check_commands(["lint"], "cargo", "python3", ["opt/verify-each", "cli/extra"])
        commands = dict(plan)
        self.assertEqual([label for label, _ in plan],
                         ["format", "clippy, default features", "clippy, experimental features"])
        self.assertEqual(commands["format"], ["cargo", "fmt", "--all", "--check"])
        for label in ["clippy, default features", "clippy, experimental features"]:
            self.assertEqual(commands[label][:4], ["cargo", "clippy", "--workspace", "--all-targets"])
            self.assertEqual(commands[label][-3:], ["--", "-D", "warnings"])
        self.assertNotIn("--features", commands["clippy, default features"])
        experimental = commands["clippy, experimental features"]
        self.assertEqual(experimental[experimental.index("--features") + 1], "opt/verify-each,cli/extra")
        self.assertEqual(len(runner.check_commands(["lint"], "cargo", "python3", [])), 2)

    def test_generators_check_outputs_and_run_unit_tests_for_each_generator(self):
        plan = runner.check_commands(["generators"], "cargo", "python3", [])
        self.assertEqual(len(plan), 2 * len(runner.GENERATOR_DIRECTORIES))
        commands = [command for _, command in plan]
        for directory in ["tools/x86-isa", "crates/keel-asm/tools"]:
            with self.subTest(directory=directory):
                self.assertIn(["python3", f"{directory}/generate.py", "--check"], commands)
                self.assertIn(["python3", "-m", "unittest", "discover", "-s", directory,
                               "-p", "test_*.py"], commands)
                self.assertTrue((runner.ROOT / directory / "generate.py").is_file())

    def test_a_failed_check_does_not_stop_later_checks_and_is_named_in_results(self):
        args = runner.arguments(["lint"])
        failing = "clippy, experimental features"
        labels = {tuple(command): label for label, command in runner.check_commands(
            ["lint"], args.cargo, runner.sys.executable, runner.lint_features(self.packages))}
        ran = []

        def run(command, cwd, env):
            ran.append(labels[tuple(command)])
            return mock.Mock(returncode=101 if ran[-1] == failing else 0)

        output = io.StringIO()
        with mock.patch.object(runner.subprocess, "run", run), contextlib.redirect_stdout(output):
            self.assertEqual(runner.run_checks(args, self.packages), 1)
        self.assertEqual(ran, ["format", "clippy, default features", failing])
        results = output.getvalue().split("Results:\n", 1)[1]
        self.assertIn(f"FAILED (exit 101): {failing}", results)
        self.assertIn("ok: format", results)

    def test_check_scopes_combine_with_each_other_but_not_with_test_selection(self):
        self.assertEqual(runner.arguments(["lint", "generators", "cranelift"]).scopes,
                         ["lint", "generators", "cranelift"])
        for args in [["lint", "codegen"], ["generators", "full"], ["lint", "--quick"],
                     ["cranelift", "ir"], ["cranelift", "--quick"],
                     ["lint", "--changed", "main"], ["generators", "--unit"],
                     ["lint", "--test", "run"]]:
            with self.subTest(args=args), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    runner.arguments(args)
                self.assertEqual(error.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
