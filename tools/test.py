#!/usr/bin/env python3
"""Run Rust tests by component or conservatively select consumers of changed files.

The lint and generators scopes run repository checks instead of Rust tests."""
from __future__ import annotations

import argparse
import json
import os
import platform
import re
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SCOPES = {
    "assembler": {"keel-asm-ir", "keel-asm"},
    "decoder": {"keel-disasm"},
    "linker": {"keel-link"},
    "ir": {"keel-ir", "keel-ir-interp"},
    "cli": {"keel"},
    "wasm": {"keel-wasm"},
    "llvm": {"keel-llvm"},
    "frontend": {"keel-c", "keel-frontend"},
    "optimizer": {"keel-opt"},
    "codegen": {"keel-codegen"},
}
CHECK_SCOPES = ("lint", "generators", "cranelift")
# profile-with-tracy builds the Tracy C++ client; the lint lane skips it.
LINT_SKIPPED_FEATURES = {"profile-with-tracy"}
# Each directory holds a generate.py with --check and its test_*.py unit tests.
GENERATOR_DIRECTORIES = ("tools/x86-isa", "crates/keel-asm/tools")
# keel-cranelift is a separate workspace. Its sources are indexed as readers of
# workspace files, and a change that reaches keel-ir also selects its tests.
CRANELIFT_DIRECTORY = "crates/keel-cranelift"
CRANELIFT_TRIGGER = "keel-ir"
READER_DIRECTORIES = (CRANELIFT_DIRECTORY,)
# Changed paths outside the Rust package rules, first match wins. A covered path
# selects its check (None selects none) and Rust tests only through a Rust
# source that reads it. Uncovered paths outside a package select the workspace.
PATH_POLICY = (
    (r"tools/x86-isa/|crates/keel-asm/(?:tools|isa)/", "generators"),
    (r".*\.md$", None),
    (CRANELIFT_DIRECTORY + "/", "cranelift"),
    (r"suite/(?P<group>c|ir|asm|linker|integration)/", "suite {group}"),
    (r"suite/", "suite"),
    (r"tools/", "tools"),
    (r"docs/", None),
)


# Core backend contracts, not a feature-completion inventory. Feature work should
# name its integration target explicitly; merge qualification runs every target.
CODEGEN_CORE_TARGETS = (
    "allocation_contract", "commutative_two_address", "function_policies",
    "narrow_extensions", "scalar_address", "selection", "stack_frame",
    "switch_edges", "tiers", "unwind_tables", "fast_cc", "special_entries",
    "arm_constants", "arm_indexed_memory", "arm_zero_branches",
    "neon_d_vectors", "vector_ir", "vector_mask_proofs", "vector_x86_256",
    "x86_features", "x86_aggregate_boundary", "x86_asm_clobbers",
    "x86_generated_binding_smoke", "f80_allocation", "asm_snapshots",
)
# keel-opt's own tests check IR text. These crates/keel tests compile C or IR
# through optimizing pipelines and run the result against an independent oracle.
OPTIMIZER_E2E_TARGETS = (
    "c_lane_reduce", "conditional_reduce", "constant_division", "full_bounded_inline",
    "if_convert_nested", "inline", "noninterposable_inline", "optimization_levels",
    "private_aggregate", "small_memory_default", "stack_cache", "stackobj", "vectorize",
)
# Under --quick, a selected package also runs these targets of other packages.
# keel-codegen's run and keel-c's run are the program-level differential oracles.
QUICK_EXTRA_TARGETS = {
    "keel-opt": (("keel-codegen", ("run",)), ("keel-c", ("run",)), ("keel", OPTIMIZER_E2E_TARGETS)),
}
# Their tests execute programs with a keel binary named by KEEL_TEST_KEEL.
KEEL_TEST_KEEL_PACKAGES = {"keel-opt", "keel-llvm"}
SKIP_SUMMARY_TESTS = 10


def consumers(seeds: set[str], packages: dict) -> set[str]:
    """Transitive normal and build consumers, then one hop of dev-dependents.

    A dev-dependency reaches only the dependent's own tests. Cargo builds a
    dependency without its dev-dependencies, so the walk does not continue."""
    def dependents(selected: set[str], dev: bool) -> set[str]:
        return {name for name, package in packages.items()
                if any(d["name"] in selected and (d.get("kind") == "dev") == dev
                       for d in package["dependencies"])}
    selected = set(seeds)
    while True:
        additions = dependents(selected, False) - selected
        if not additions:
            return selected | dependents(selected, True)
        selected |= additions


# A dependency expression is narrow only when its whole argument is literal.
_INCLUDE = re.compile(r"\binclude(?:_str|_bytes)?\s*!\s*(?P<opening>[({\[])")
_PATH = re.compile(r"#\s*\[\s*path\s*=")
# Passing a builtin's name to a macro or importing it under an alias hides the
# invocation from the literal-call index. Identifier concatenation can too.
_INDIRECT_INCLUDE = re.compile(r"(?:[\[({,=]|::)\s*include(?:_str|_bytes)\b(?!\s*!)|::\s*include\b(?!\s*!)")
_CONCAT_IDENT = re.compile(r"\bconcat_idents\s*!|\$\s*\{\s*concat\s*\(")
_LITERAL = re.compile(r'\s*(?:r(?P<hashes>\#{0,})"(?P<raw>.*?)"(?P=hashes)|(?P<quoted>"(?:[^"\\]|\\.)*"))', re.S)
_NON_CODE = re.compile(r"//[^\n]*|/\*.*?\*/|'(?:\\.|[^'\\\n])'|" + _LITERAL.pattern, re.S)
_GENERATED = re.compile(r'\s*concat!\s*\(\s*env!\s*\(\s*"OUT_DIR"\s*\)\s*,\s*"[^"\n]*"\s*,?\s*\)')


def passed_include(tokens: str) -> bool:
    """Recognize the bare include builtin passed within a macro token tree."""
    if not re.search(r"\binclude\b", tokens):
        return False
    openings = {match.end() - 1 for match in re.finditer(r"\b\w+\s*!\s*[(\[{]", tokens)}
    active = []
    for token in re.finditer(r"[(){}\[\]]|\binclude\b", tokens):
        if token[0] in {"(", "{", "["}:
            active.append(token.start() in openings or bool(active and active[-1]))
        elif token[0] in {")", "}", "]"}:
            if active:
                active.pop()
        elif active and active[-1] and not re.match(r"\s*[!.:\w]", tokens[token.end():]):
            return True
    return False


def path_mention(value: str, directories: dict[str, str]) -> str | None:
    """Repository path named by a string literal, for runtime fixture reads.

    Leading `..` components are dropped, so a literal relative to a manifest
    directory and one relative to the repository root name the same path."""
    if not re.fullmatch(r"[\w.+-]+(?:/[\w.+-]+)+/?", value):
        return None
    parts = [part for part in value.split("/") if part not in ("", ".")]
    while parts and parts[0] == "..":
        parts.pop(0)
    if len(parts) < 2:
        return None
    if parts[0] in directories:
        parts[0] = directories[parts[0]]
    return "/".join(parts)


def static_inputs(packages: dict, root: Path) -> tuple[dict[str, set[str]], list[tuple[str, str]],
                                                       list[tuple[str, str]], bool]:
    """Index source inputs; unresolved expressions require conservative selection.

    Returns include and module edges, module path suffixes, literal path
    mentions and whether any input expression is unresolved."""
    root = root.resolve()
    edges: dict[str, set[str]] = {}
    module_suffixes = []
    mentions = []
    sources = []
    unresolved = False
    directories = [package["directory"] for package in packages.values()]
    directories += [d for d in READER_DIRECTORIES if (root / d).is_dir() and d not in directories]
    names = {Path(directory).name: directory for directory in directories}
    for directory in directories:
        for source in (root / directory).rglob("*.rs"):
            if "target" in source.relative_to(root / directory).parts:
                continue
            try:
                text = source.read_text()
            except (OSError, UnicodeError):
                unresolved = True
                continue
            importer = source.relative_to(root).as_posix()
            sources.append(importer)
            tokens = _NON_CODE.sub(" ", text)
            unresolved |= bool(_INDIRECT_INCLUDE.search(tokens) or passed_include(tokens) or _CONCAT_IDENT.search(tokens))
            # Conditional path attributes need a Rust attribute parser.
            unresolved |= bool(re.search(r"#\s*\[\s*cfg_attr\s*\([^]]*\bpath\s*=", text))
            for literal in _NON_CODE.finditer(text):
                value = literal["raw"]
                if value is None and literal["quoted"] is not None:
                    try:
                        value = json.loads(literal["quoted"])
                    except ValueError:
                        continue
                mention = path_mention(value, names) if value is not None else None
                if mention is not None:
                    mentions.append((mention, importer))
            for pattern, closing in [(_INCLUDE, ")"), (_PATH, "]")]:
                for expression in pattern.finditer(text):
                    if pattern is _INCLUDE:
                        closing = {"(": ")", "{": "}", "[": "]"}[expression["opening"]]
                    argument = text[expression.end():]
                    literal = _LITERAL.match(argument)
                    if literal is None or not re.match(r"\s*,?\s*" + re.escape(closing), argument[literal.end():]):
                        # Generated Rust outputs are not Git changed-file inputs.
                        generated = _GENERATED.match(argument) if pattern is _INCLUDE else None
                        if generated is None or not re.match(r"\s*,?\s*" + re.escape(closing), argument[generated.end():]):
                            unresolved = True
                        continue
                    try:
                        value = literal["raw"] if literal["raw"] is not None else json.loads(literal["quoted"])
                        target = (source.parent / value).resolve().relative_to(root).as_posix()
                    except (ValueError, OSError):
                        unresolved = True
                        continue
                    edges.setdefault(target, set()).add(importer)
                    if pattern is _PATH:
                        # Inline modules change the relative attribute base. A
                        # suffix match includes those possible readers as well.
                        suffix = os.path.normpath(value).replace(os.sep, "/")
                        while suffix.startswith("../"):
                            suffix = suffix[3:]
                        module_suffixes.append((suffix, importer))
    for suffix, importer in module_suffixes:
        for source in sources:
            if source == suffix or source.endswith("/" + suffix):
                edges.setdefault(source, set()).add(importer)
    return edges, module_suffixes, mentions, unresolved


def path_policy(path: str) -> tuple[bool, str | None]:
    """Whether PATH_POLICY covers a path, and the check it selects."""
    for pattern, check in PATH_POLICY:
        match = re.match(pattern, path)
        if match:
            return True, check.format(**match.groupdict()) if check else None
    return False, None


def changed_selection(paths: list[str], packages: dict, root: Path = ROOT) -> tuple[set[str], set[str]]:
    """Packages to test and checks to run for changed paths.

    Unknown paths broaden to the workspace rather than silently skipping tests."""
    if not paths:
        return set(), set()
    checks = set()
    policy = {}
    for path in paths:
        covered, check = path_policy(path)
        if covered:
            policy[path] = check
            if check:
                checks.add(check)
    edges, module_suffixes, mentions, unresolved = static_inputs(packages, root)
    if unresolved:
        return set(packages), checks
    # A source file can be included as a module and itself use ordinary `mod`
    # children. Treat cross-package source imports as package dependencies too.
    # This covers edits to those children without parsing Rust's module tree.
    source_packages = {name: {**package, "dependencies": list(package["dependencies"])}
                       for name, package in packages.items()}

    def package_of(path: str) -> str | None:
        return next((name for name, package in packages.items()
                     if path.startswith(package["directory"] + "/")), None)

    for target, importers in edges.items():
        dependency = next((name for name, package in packages.items()
                           if target.startswith(package["directory"] + "/src/")), None)
        if dependency is None:
            continue
        for importer in importers:
            reader = package_of(importer)
            if reader is not None and reader != dependency:
                kind = "dev" if importer.startswith(packages[reader]["directory"] + "/tests/") else None
                source_packages[reader]["dependencies"].append({"name": dependency, "kind": kind})
    selected = set()
    for path in paths:
        owner = package_of(path)
        if owner is None and path not in policy:
            return set(packages), checks
        pending = [path]
        reached = set()
        while pending:
            current = pending.pop()
            if current in reached:
                continue
            reached.add(current)
            readers = edges.get(current, set()) | {
                importer for suffix, importer in module_suffixes
                if current == suffix or current.endswith("/" + suffix)
            } | {
                importer for prefix, importer in mentions
                if current == prefix.rstrip("/") or current.startswith(prefix.rstrip("/") + "/")
            }
            pending.extend(readers - reached)
        for current in reached:
            if current == path and path in policy:
                # Policy paths select Rust tests only through their readers.
                continue
            if current.startswith(CRANELIFT_DIRECTORY + "/"):
                checks.add("cranelift")
                continue
            reader = package_of(current)
            if reader is None:
                return set(packages), checks
            # Tests, fixtures and test helpers belong to their package alone;
            # other packages that read them are found as readers above.
            if current[len(packages[reader]["directory"]) + 1:].startswith("tests/"):
                selected.add(reader)
            else:
                closure = consumers({reader}, source_packages)
                selected |= closure
                if CRANELIFT_TRIGGER in closure:
                    checks.add("cranelift")
    return selected, checks


def changed_packages(paths: list[str], packages: dict, root: Path = ROOT) -> set[str]:
    """The Rust packages part of changed_selection."""
    return changed_selection(paths, packages, root)[0]


def component_packages(scopes: list[str], packages: dict, include_consumers: bool = False) -> set[str]:
    if "full" in scopes:
        return set(packages)
    selected = set().union(*(SCOPES[scope] for scope in scopes))
    return consumers(selected, packages) if include_consumers else selected


def lint_features(packages: dict) -> list[str]:
    """Every workspace feature except skipped ones and features that enable them."""
    return [
        f"{name}/{feature}"
        for name, package in sorted(packages.items())
        for feature, enables in sorted(package.get("features", {}).items())
        if feature != "default" and feature not in LINT_SKIPPED_FEATURES
        and not any(value.split("/")[-1] in LINT_SKIPPED_FEATURES for value in enables)
    ]


def check_commands(scopes: list[str], cargo: str, python: str,
                   features: list[str]) -> list[tuple[str, list[str]]]:
    """Labelled check commands, in run order.

    Besides CHECK_SCOPES, change selection uses "tools", "suite" (every group)
    and "suite GROUP"."""
    plan = []
    if "lint" in scopes:
        clippy = [cargo, "clippy", "--workspace", "--all-targets"]
        plan += [("format", [cargo, "fmt", "--all", "--check"]),
                 ("clippy, default features", [*clippy, "--", "-D", "warnings"])]
        if features:
            plan.append(("clippy, experimental features",
                         [*clippy, "--features", ",".join(features), "--", "-D", "warnings"]))
    if "generators" in scopes:
        for directory in GENERATOR_DIRECTORIES:
            plan += [(f"{directory} generator check", [python, f"{directory}/generate.py", "--check"]),
                     (f"{directory} unit tests",
                      [python, "-m", "unittest", "discover", "-s", directory, "-p", "test_*.py"])]
    if "tools" in scopes:
        plan.append(("tools unit tests",
                     [python, "-m", "unittest", "discover", "-s", "tools/tests", "-p", "test_*.py"]))
    groups = sorted(scope.split()[1] for scope in scopes if scope.startswith("suite "))
    if "suite" in scopes or groups:
        plan.append(("suite runner tests", [cargo, "suite", "check", *([] if "suite" in scopes else groups)]))
    if "cranelift" in scopes:
        plan.append(("keel-cranelift tests", [cargo, "test", "--manifest-path",
                                              f"{CRANELIFT_DIRECTORY}/Cargo.toml", "--locked",
                                              "--no-fail-fast"]))
    return plan


def commands(cargo: str, runner: str, profile: str, packages: list[str], quick: bool,
             unit: bool = False, targets: list[str] | None = None,
             doctest_packages: list[str] | None = None,
             features: str | None = None) -> list[list[str]]:
    feature_args = ["--features", features] if features else []
    target_args = ["--lib"] if unit else [arg for target in targets or [] for arg in ("--test", target)]
    batches = [(packages, target_args)]
    if quick and not unit and not targets:
        extras: dict[str, list[str]] = {}
        for package in packages:
            for dependent, names in QUICK_EXTRA_TARGETS.get(package, ()):
                extras.setdefault(dependent, []).extend(names)
        others = [package for package in packages if package != "keel-codegen"]
        batches = [(others, [])] if others else []
        if "keel-codegen" in packages:
            core = [*CODEGEN_CORE_TARGETS, *extras.pop("keel-codegen", [])]
            batches.append((["keel-codegen"], ["--lib", *[arg for target in core for arg in ("--test", target)]]))
        for dependent, names in sorted(extras.items()):
            # A selected package already runs all of its targets.
            if dependent not in packages:
                batches.append(([dependent], [arg for target in names for arg in ("--test", target)]))
    result = []
    for batch_packages, batch_targets in batches:
        selection = [arg for package in batch_packages for arg in ("-p", package)] + feature_args
        if runner == "nextest":
            tests = [cargo, "nextest", "run", "--cargo-profile", profile,
                     "--no-fail-fast", *selection, *batch_targets]
            if quick:
                tests += ["-E", "not test(exhaustive_)"]
        else:
            tests = [cargo, "test", "--profile", profile, "--no-fail-fast", *selection, *batch_targets]
            if quick:
                if not batch_targets:
                    tests += ["--lib", "--bins", "--tests"]
                tests += ["--", "--skip", "exhaustive_"]
        result.append(tests)
    # nextest does not run doctests. Only libraries with doctests need this phase.
    docs = packages if doctest_packages is None else doctest_packages
    if runner == "nextest" and not quick and not target_args and docs:
        doc_selection = [arg for package in docs for arg in ("-p", package)] + feature_args
        result.append([cargo, "test", "--profile", profile, "--doc", "--no-fail-fast", *doc_selection])
    return result


def test_label(command: list[str]) -> str:
    """Short result-list name for a Rust test command."""
    packages = [command[i + 1] for i, arg in enumerate(command) if arg == "-p"]
    targets = [command[i + 1] for i, arg in enumerate(command) if arg == "--test"]
    label = ("doctests " if "--doc" in command else "tests ") + ", ".join(packages)
    if len(targets) == 1:
        label += f" --test {targets[0]}"
    elif targets:
        label += f" ({len(targets)} targets)"
    return label


def default_libllvm(system: str, machine: str) -> Path | None:
    """The rustc sysroot's LLVM, on the host where the import tests use it."""
    if system != "Darwin" or machine not in ("arm64", "aarch64"):
        return None
    try:
        sysroot = subprocess.run(["rustc", "--print", "sysroot"], capture_output=True,
                                 text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    library = Path(sysroot) / "lib/libLLVM.dylib"
    return library if library.is_file() else None


def keel_executable(build_output: str) -> str | None:
    """The keel binary path from Cargo JSON build messages."""
    for line in build_output.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if (message.get("reason") == "compiler-artifact" and message.get("executable")
                and message["target"]["name"] == "keel"):
            return message["executable"]
    return None


def skip_entries(log: str) -> list[dict[str, str]]:
    """Parse KEEL_SKIP_LOG lines, "<test>\\t<reason>"."""
    entries = []
    for line in log.splitlines():
        if line.strip():
            test, _, reason = line.partition("\t")
            entries.append({"test": test.strip(), "reason": reason.strip() or "no reason given"})
    return entries


def skip_summary(log: str) -> list[str]:
    """Skipped tests grouped by reason, largest group first.

    A test that records the same reason several times counts once."""
    reasons: dict[str, set[str]] = {}
    for entry in skip_entries(log):
        reasons.setdefault(entry["reason"], set()).add(entry["test"])
    if not reasons:
        return ["Skipped tests: none recorded"]
    lines = [f"Skipped tests: {sum(map(len, reasons.values()))}"]
    for reason, tests in sorted(reasons.items(), key=lambda item: (-len(item[1]), item[0])):
        lines.append(f"  {len(tests)}: {reason}")
        lines += [f"      {test}" for test in sorted(tests)[:SKIP_SUMMARY_TESTS]]
        if len(tests) > SKIP_SUMMARY_TESTS:
            lines.append(f"      ... and {len(tests) - SKIP_SUMMARY_TESTS} more")
    return lines


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scopes", nargs="*", metavar="COMPONENT", help="one or more of: " + ", ".join(["full", *SCOPES]) + " (default: full); or the checks " + ", ".join(CHECK_SCOPES))
    parser.add_argument("--changed", metavar="BASE", help="include committed, staged, unstaged and untracked changes since BASE")
    coverage = parser.add_mutually_exclusive_group()
    coverage.add_argument("--consumers", action="store_true", help="also test workspace consumers of named components")
    coverage.add_argument("--direct", action="store_true", help="component packages only (the default)")
    targets = parser.add_mutually_exclusive_group()
    targets.add_argument("--unit", action="store_true", help="library unit tests only; no integration tests or doctests")
    targets.add_argument("--test", action="append", default=[], metavar="TARGET", help="specific integration-test target; repeat to select several")
    parser.add_argument("--features", metavar="LIST", help="Cargo features to enable, such as experimental-regalloc")
    parser.add_argument("--quick", action="store_true", help="core codegen regressions and optimizer oracles; elsewhere exclude exhaustive_ matrices and doctests")
    parser.add_argument("--runner", choices=["auto", "cargo", "nextest"], default="auto")
    parser.add_argument("--profile", default="test-fast", help="Cargo build profile (default: test-fast)")
    parser.add_argument("--cargo", default="cargo", help="Cargo command or local scheduler executable")
    parser.add_argument("--dry-run", action="store_true", help="print selected packages and commands without building")
    parser.add_argument("--report", type=Path, help="write command timings and exit codes as JSON")
    args = parser.parse_args(argv)
    args.scopes = args.scopes or ["full"]
    unknown = set(args.scopes) - {"full", *SCOPES, *CHECK_SCOPES}
    if unknown:
        parser.error("unknown component: " + ", ".join(sorted(unknown)))
    checks = set(args.scopes) & set(CHECK_SCOPES)
    if checks:
        if set(args.scopes) - checks:
            parser.error(", ".join(CHECK_SCOPES) + " cannot be combined with test components")
        if args.changed or args.consumers or args.direct or args.unit or args.test or args.quick:
            parser.error(", ".join(CHECK_SCOPES) + " take no test selection options")
    if "full" in args.scopes and len(args.scopes) > 1:
        parser.error("full cannot be combined with named components")
    if (args.direct or args.consumers) and args.scopes == ["full"]:
        parser.error("--direct and --consumers require named components")
    if args.changed and (args.scopes != ["full"] or args.direct or args.consumers):
        parser.error("--changed cannot be combined with named components, --direct or --consumers")
    return args


def run_steps(steps: list[tuple[str, list[str]]], env: dict[str, str]) -> list[dict]:
    """Run every step, even after a failure, and record each result."""
    records = []
    for label, command in steps:
        print(f"== {label}\n+ " + shlex.join(command), flush=True)
        started = time.monotonic()
        result = subprocess.run(command, cwd=ROOT, env=env)
        records.append({"label": label, "command": command,
                        "seconds": time.monotonic() - started, "returncode": result.returncode})
    return records


def run_checks(args: argparse.Namespace, packages: dict) -> int:
    """Run the named checks, then list each result so a failed step is easy to find."""
    plan = check_commands(args.scopes, args.cargo, sys.executable, lint_features(packages))
    return run_plan(args, plan, {"checks": args.scopes}, "cranelift" in args.scopes)


def run_plan(args: argparse.Namespace, plan: list[tuple[str, list[str]]], report: dict,
             runs_tests: bool, env: dict[str, str] | None = None,
             records: list[dict] | None = None) -> int:
    """Run a labelled plan with a fresh skip log, then print results and skips."""
    if args.dry_run:
        for label, command in plan:
            print(f"== {label}\n+ " + shlex.join(command))
        return 0
    env = dict(os.environ if env is None else env)
    descriptor, log = tempfile.mkstemp(prefix="keel-skips-", suffix=".log")
    os.close(descriptor)
    env["KEEL_SKIP_LOG"] = log
    try:
        records = [*(records or []), *run_steps(plan, env)]
        skips = Path(log).read_text(errors="replace")
    finally:
        os.unlink(log)
    print("Results:")
    for record in records:
        status = "ok" if record["returncode"] == 0 else f"FAILED (exit {record['returncode']})"
        print(f"  {status}: {record['label']}")
    if runs_tests:
        print("\n".join(skip_summary(skips)))
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps({**report, "commands": records, "skips": skip_entries(skips)},
                                          indent=2) + "\n")
    return int(any(r["returncode"] for r in records))


def workspace_packages(cargo: str) -> dict:
    metadata = json.loads(subprocess.check_output(
        [cargo, "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT))
    members = set(metadata["workspace_members"])
    return {p["name"]: {**p, "directory": str(Path(p["manifest_path"]).parent.relative_to(ROOT))}
            for p in metadata["packages"] if p["id"] in members}


def changed_paths(base: str) -> list[str]:
    tracked = subprocess.check_output(
        ["git", "diff", "--name-only", "--no-renames", "-z", base, "--"], cwd=ROOT)
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=ROOT)
    return os.fsdecode(tracked + untracked).strip("\0").split("\0") if tracked or untracked else []


def main() -> int:
    args = arguments()
    packages = workspace_packages(args.cargo)
    if set(args.scopes) <= set(CHECK_SCOPES):
        return run_checks(args, packages)
    checks: list[str] = []
    if args.changed:
        paths = changed_paths(args.changed)
        selected, changed_checks = changed_selection(paths, packages)
        checks = sorted(changed_checks)
        if not selected and not checks:
            print("No changed files; no tests selected." if not paths
                  else "No tests or checks selected for the changed files.")
            return 0
    else:
        selected = component_packages(args.scopes, packages, args.consumers)
    plan: list[tuple[str, list[str]]] = []
    records: list[dict] = []
    env = dict(os.environ)
    runner = args.runner
    core_codegen = False
    if selected:
        if runner == "auto":
            runner = "nextest" if shutil.which("cargo-nextest") else "cargo"
        if runner == "nextest" and not shutil.which("cargo-nextest"):
            print("cargo-nextest is missing; install it or select --runner cargo", file=sys.stderr)
            return 2
        doctest_packages = sorted(name for name in selected
                                  if any(target["doctest"] for target in packages[name]["targets"]))
        tests = commands(args.cargo, runner, args.profile, sorted(selected), args.quick,
                         args.unit, args.test, doctest_packages, args.features)
        print("Packages: " + ", ".join(sorted(selected)), flush=True)
        if args.unit:
            coverage = "library unit tests"
        elif args.test:
            coverage = "integration targets: " + ", ".join(args.test)
        else:
            coverage = "selected Rust tests" + (" and doctests" if not args.quick and doctest_packages else "")
        core_codegen = args.quick and "keel-codegen" in selected and not args.unit and not args.test
        if core_codegen:
            coverage += " (codegen library and core integration targets only)"
        if args.quick and not args.unit and not args.test and any(p in QUICK_EXTRA_TARGETS for p in selected):
            coverage += " (plus optimizer oracle and end-to-end targets)"
        if args.quick:
            coverage += " (exhaustive tests and doctests excluded)"
        print("Coverage: " + coverage, flush=True)
        if "keel-llvm" in selected and "KEEL_TEST_LIBLLVM" not in env:
            library = default_libllvm(platform.system(), platform.machine())
            if library is not None:
                env["KEEL_TEST_LIBLLVM"] = str(library)
                print(f"KEEL_TEST_LIBLLVM={library}", flush=True)
        if selected & KEEL_TEST_KEEL_PACKAGES and not args.unit and "KEEL_TEST_KEEL" not in env:
            build = [args.cargo, "build", "--profile", args.profile, "-p", "keel", "--bin", "keel",
                     "--message-format=json-render-diagnostics"]
            label = "keel binary for KEEL_TEST_KEEL"
            print(f"== {label}\n+ " + shlex.join(build), flush=True)
            if not args.dry_run:
                started = time.monotonic()
                result = subprocess.run(build, cwd=ROOT, env=env, stdout=subprocess.PIPE, text=True)
                executable = keel_executable(result.stdout) if result.returncode == 0 else None
                if result.returncode == 0 and executable is None:
                    print("the build reported no keel executable", file=sys.stderr)
                records.append({"label": label, "command": build, "seconds": time.monotonic() - started,
                                "returncode": result.returncode or int(executable is None)})
                if executable:
                    env["KEEL_TEST_KEEL"] = executable
                    print(f"KEEL_TEST_KEEL={executable}", flush=True)
        plan += [(test_label(command), command) for command in tests]
    if checks:
        print("Checks: " + ", ".join(checks), flush=True)
        plan += check_commands(checks, args.cargo, sys.executable, [])
    report = {"packages": sorted(selected), "checks": checks, "quick": args.quick,
              "unit": args.unit, "targets": args.test,
              "codegen_core_targets": list(CODEGEN_CORE_TARGETS) if core_codegen else [],
              "runner": runner}
    return run_plan(args, plan, report, True, env, records)


if __name__ == "__main__":
    sys.exit(main())
