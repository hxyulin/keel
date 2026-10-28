#!/usr/bin/env python3
"""Run Rust tests by component or conservatively select consumers of changed files."""
from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
SCOPES = {
    "assembler": {"keel-asm-ir", "keel-asm"},
    "decoder": {"keel-disasm"},
    "linker": {"keel-link"},
    "ir": {"keel-ir"},
    "cli": {"keel"},
    "wasm": {"keel-wasm"},
    "llvm": {"keel-llvm"},
    "toolchain": {"keel-toolchain"},
    "frontend": {"keel-c", "keel-frontend"},
    "optimizer": {"keel-opt"},
    "codegen": {"keel-codegen"},
}


# Core backend contracts, not a feature-completion inventory. Feature work should
# name its integration target explicitly; merge qualification runs every target.
CODEGEN_CORE_TARGETS = (
    "allocation_contract", "commutative_two_address", "function_policies",
    "narrow_extensions", "scalar_address", "selection", "stack_frame",
    "switch_edges", "tiers", "unwind_tables", "fast_cc", "special_entries",
    "arm_constants", "arm_indexed_memory", "arm_zero_branches",
    "neon_d_vectors", "vector_ir", "vector_mask_proofs", "vector_x86_256",
    "x86_features", "x86_aggregate_boundary", "x86_asm_clobbers",
    "x86_generated_binding_smoke", "f80_allocation",
)


def consumers(seeds: set[str], packages: dict) -> set[str]:
    """Include transitive workspace consumers, including test dependencies."""
    selected = set(seeds)
    while True:
        additions = {
            name for name, package in packages.items()
            if any(d["name"] in selected for d in package["dependencies"])
        }
        expanded = selected | additions
        if expanded == selected:
            return selected
        selected = expanded


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


def static_inputs(packages: dict, root: Path) -> tuple[dict[str, set[str]], list[tuple[str, str]], bool]:
    """Index source inputs; unresolved expressions require conservative selection."""
    root = root.resolve()
    edges: dict[str, set[str]] = {}
    module_suffixes = []
    sources = []
    unresolved = False
    for package in packages.values():
        for source in (root / package["directory"]).rglob("*.rs"):
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
    return edges, module_suffixes, unresolved


def changed_packages(paths: list[str], packages: dict, root: Path = ROOT) -> set[str]:
    """Unknown paths broaden to the workspace rather than silently skipping tests."""
    if not paths:
        return set()
    edges, module_suffixes, unresolved = static_inputs(packages, root)
    if unresolved:
        return set(packages)
    # A source file can be included as a module and itself use ordinary `mod`
    # children. Treat cross-package source imports as package dependencies too.
    # This covers edits to those children without parsing Rust's module tree.
    source_packages = {name: {**package, "dependencies": list(package["dependencies"])}
                       for name, package in packages.items()}
    for target, importers in edges.items():
        dependency = next((name for name, package in packages.items()
                           if target.startswith(package["directory"] + "/src/")), None)
        if dependency is None:
            continue
        for importer in importers:
            reader = next((name for name, package in packages.items()
                           if importer.startswith(package["directory"] + "/")), None)
            if reader is not None and reader != dependency:
                source_packages[reader]["dependencies"].append({"name": dependency})
    selected = set()
    for path in paths:
        owner = next((name for name, p in packages.items()
                      if path.startswith(p["directory"] + "/")), None)
        if owner is None:
            return set(packages)
        pending = [path]
        reached = set()
        indexed = False
        while pending:
            current = pending.pop()
            if current in reached:
                continue
            reached.add(current)
            readers = edges.get(current, set()) | {
                importer for suffix, importer in module_suffixes
                if current == suffix or current.endswith("/" + suffix)
            }
            indexed |= bool(readers)
            pending.extend(readers - reached)
        relative = path[len(packages[owner]["directory"]) + 1:]
        fixture = relative.startswith("tests/") and not (
            Path(relative).parent == Path("tests") and relative.endswith(".rs"))
        if fixture and not indexed:
            return set(packages)
        for current in reached:
            reader = next((name for name, p in packages.items()
                           if current.startswith(p["directory"] + "/")), None)
            if reader is None:
                return set(packages)
            relative = current[len(packages[reader]["directory"]) + 1:]
            selected |= {reader} if relative.startswith("tests/") else consumers({reader}, source_packages)
    return selected


def component_packages(scopes: list[str], packages: dict, include_consumers: bool = False) -> set[str]:
    if "full" in scopes:
        return set(packages)
    selected = set().union(*(SCOPES[scope] for scope in scopes))
    return consumers(selected, packages) if include_consumers else selected


def commands(cargo: str, runner: str, profile: str, packages: list[str], quick: bool,
             unit: bool = False, targets: list[str] | None = None,
             doctest_packages: list[str] | None = None) -> list[list[str]]:
    target_args = ["--lib"] if unit else [arg for target in targets or [] for arg in ("--test", target)]
    batches = [(packages, target_args)]
    if quick and "keel-codegen" in packages and not unit and not targets:
        others = [package for package in packages if package != "keel-codegen"]
        batches = [(others, [])] if others else []
        core_targets = ["--lib", *[arg for target in CODEGEN_CORE_TARGETS for arg in ("--test", target)]]
        batches.append((["keel-codegen"], core_targets))
    result = []
    for batch_packages, batch_targets in batches:
        selection = [arg for package in batch_packages for arg in ("-p", package)]
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
        doc_selection = [arg for package in docs for arg in ("-p", package)]
        result.append([cargo, "test", "--profile", profile, "--doc", "--no-fail-fast", *doc_selection])
    return result


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scopes", nargs="*", metavar="COMPONENT", help="one or more of: " + ", ".join(["full", *SCOPES]) + " (default: full)")
    parser.add_argument("--changed", metavar="BASE", help="include committed, staged, unstaged and untracked changes since BASE")
    coverage = parser.add_mutually_exclusive_group()
    coverage.add_argument("--consumers", action="store_true", help="also test transitive consumers of named components")
    coverage.add_argument("--direct", action="store_true", help="component packages only (the default)")
    targets = parser.add_mutually_exclusive_group()
    targets.add_argument("--unit", action="store_true", help="library unit tests only; no integration tests or doctests")
    targets.add_argument("--test", action="append", default=[], metavar="TARGET", help="specific integration-test target; repeat to select several")
    parser.add_argument("--quick", action="store_true", help="core codegen regressions; elsewhere exclude exhaustive_ matrices and doctests")
    parser.add_argument("--runner", choices=["auto", "cargo", "nextest"], default="auto")
    parser.add_argument("--profile", default="test-fast", help="Cargo build profile (default: test-fast)")
    parser.add_argument("--cargo", default="cargo", help="Cargo command or local scheduler executable")
    parser.add_argument("--dry-run", action="store_true", help="print selected packages and commands without building")
    parser.add_argument("--report", type=Path, help="write command timings and exit codes as JSON")
    args = parser.parse_args(argv)
    args.scopes = args.scopes or ["full"]
    unknown = set(args.scopes) - {"full", *SCOPES}
    if unknown:
        parser.error("unknown component: " + ", ".join(sorted(unknown)))
    if "full" in args.scopes and len(args.scopes) > 1:
        parser.error("full cannot be combined with named components")
    if (args.direct or args.consumers) and args.scopes == ["full"]:
        parser.error("--direct and --consumers require named components")
    if args.changed and (args.scopes != ["full"] or args.direct or args.consumers):
        parser.error("--changed cannot be combined with named components, --direct or --consumers")
    return args


def main() -> int:
    args = arguments()
    metadata = json.loads(subprocess.check_output(
        [args.cargo, "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT))
    members = set(metadata["workspace_members"])
    packages = {p["name"]: {**p, "directory": str(Path(p["manifest_path"]).parent.relative_to(ROOT))}
                for p in metadata["packages"] if p["id"] in members}
    if args.changed:
        tracked = subprocess.check_output(
            ["git", "diff", "--name-only", "--no-renames", "-z", args.changed, "--"], cwd=ROOT)
        untracked = subprocess.check_output(
            ["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=ROOT)
        paths = os.fsdecode(tracked + untracked).strip("\0").split("\0") if tracked or untracked else []
        selected = changed_packages(paths, packages)
    else:
        selected = component_packages(args.scopes, packages, args.consumers)
    if not selected:
        print("No changed files; no tests selected.")
        return 0
    runner = args.runner
    if runner == "auto":
        runner = "nextest" if shutil.which("cargo-nextest") else "cargo"
    if runner == "nextest" and not shutil.which("cargo-nextest"):
        print("cargo-nextest is missing; install it or select --runner cargo", file=sys.stderr)
        return 2
    doctest_packages = sorted(name for name in selected
                              if any(target["doctest"] for target in packages[name]["targets"]))
    plan = commands(args.cargo, runner, args.profile, sorted(selected), args.quick,
                    args.unit, args.test, doctest_packages)
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
    if args.quick:
        coverage += " (exhaustive tests and doctests excluded)"
    print("Coverage: " + coverage, flush=True)
    records = []
    for command in plan:
        print("+ " + shlex.join(command), flush=True)
        if args.dry_run:
            continue
        started = time.monotonic()
        result = subprocess.run(command, cwd=ROOT)
        records.append({"command": command, "seconds": time.monotonic() - started,
                        "returncode": result.returncode})
    if args.report and not args.dry_run:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps({"packages": sorted(selected), "quick": args.quick,
                                          "unit": args.unit, "targets": args.test,
                                          "codegen_core_targets": list(CODEGEN_CORE_TARGETS) if core_codegen else [],
                                          "runner": runner, "commands": records}, indent=2) + "\n")
    return int(any(r["returncode"] for r in records))


if __name__ == "__main__":
    sys.exit(main())
