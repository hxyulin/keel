#!/usr/bin/env python3
"""Run Rust tests by component or conservatively select consumers of changed files."""
from __future__ import annotations

import argparse
import json
import os
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
    "frontend": {"keel-c", "keel-frontend"},
    "optimizer": {"keel-opt"},
    "codegen": {"keel-codegen"},
}


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


def changed_packages(paths: list[str], packages: dict) -> set[str]:
    """Unknown paths broaden to the workspace rather than silently skipping tests."""
    selected = set()
    for path in paths:
        owner = next((name for name, p in packages.items()
                      if path.startswith(p["directory"] + "/")), None)
        if owner is None:
            return set(packages)
        relative = path[len(packages[owner]["directory"]) + 1:]
        # Cross-crate include_str!/#[path] fixture edges are absent from Cargo metadata.
        # Broaden shared inputs to the workspace rather than guessing their consumers.
        if relative.startswith("tests/"):
            standalone_test = Path(relative).parent == Path("tests") and relative.endswith(".rs")
            selected |= {owner} if standalone_test else set(packages)
        else:
            selected |= consumers({owner}, packages)
    return selected


def commands(cargo: str, runner: str, profile: str, packages: list[str], quick: bool) -> list[list[str]]:
    selection = [arg for package in packages for arg in ("-p", package)]
    if runner == "nextest":
        tests = [cargo, "nextest", "run", "--cargo-profile", profile,
                 "--no-fail-fast", *selection]
        if quick:
            tests += ["-E", "not test(exhaustive_)"]
    else:
        tests = [cargo, "test", "--profile", profile, "--no-fail-fast", *selection]
        if quick:
            tests += ["--lib", "--bins", "--tests", "--", "--skip", "exhaustive_"]
    result = [tests]
    # nextest does not run doctests. A full selection must run them separately.
    if runner == "nextest" and not quick:
        result.append([cargo, "test", "--profile", profile, "--doc", "--no-fail-fast", *selection])
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scope", nargs="?", default="full", choices=["full", *SCOPES])
    parser.add_argument("--changed", metavar="BASE", help="include committed, staged, unstaged and untracked changes since BASE")
    parser.add_argument("--direct", action="store_true", help="named component only, without consumer tests")
    parser.add_argument("--quick", action="store_true", help="exclude exhaustive_ matrices and doctests")
    parser.add_argument("--runner", choices=["auto", "cargo", "nextest"], default="auto")
    parser.add_argument("--profile", default="test-fast", help="Cargo build profile (default: test-fast)")
    parser.add_argument("--cargo", default="cargo", help="Cargo command or local scheduler executable")
    parser.add_argument("--dry-run", action="store_true", help="print selected packages and commands without building")
    parser.add_argument("--report", type=Path, help="write command timings and exit codes as JSON")
    args = parser.parse_args()
    if args.direct and args.scope == "full":
        parser.error("--direct requires a named component")
    if args.changed and (args.scope != "full" or args.direct):
        parser.error("--changed cannot be combined with a named scope or --direct")
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
    elif args.scope == "full":
        selected = set(packages)
    else:
        selected = SCOPES[args.scope]
        if not args.direct:
            selected = consumers(selected, packages)
    if not selected:
        print("No changed files; no tests selected.")
        return 0
    runner = args.runner
    if runner == "auto":
        runner = "nextest" if shutil.which("cargo-nextest") else "cargo"
    if runner == "nextest" and not shutil.which("cargo-nextest"):
        parser.error("cargo-nextest is missing; install it or select --runner cargo")
    plan = commands(args.cargo, runner, args.profile, sorted(selected), args.quick)
    print("Packages: " + ", ".join(sorted(selected)), flush=True)
    print("Coverage: " + ("quick (exhaustive matrices and doctests excluded)" if args.quick else "full selected Rust tests and doctests"), flush=True)
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
                                          "runner": runner, "commands": records}, indent=2) + "\n")
    return int(any(r["returncode"] for r in records))


if __name__ == "__main__":
    sys.exit(main())
