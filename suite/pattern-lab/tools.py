#!/usr/bin/env python3
"""Standalone, stdlib-only LLVM discovery and declared-model MCA adapter."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess
import sys


PROBE_TIMEOUT = 5
MAX_ITERATIONS = 100000
MAX_SOURCE_BYTES = 4 * 1024 * 1024
# Deliberately finite: no directory walks, globbing, package manager or installation.
CANDIDATE_DIRS = (
    "/opt/homebrew/opt/llvm/bin",
    "/usr/local/opt/llvm/bin",
    "/usr/lib/llvm-19/bin",
    "/usr/lib/llvm-18/bin",
    "/usr/lib/llvm-17/bin",
)
TOKEN = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_.+-]{0,127}\Z")


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            digest.update(block)
    return digest.hexdigest()


def regular_file(value):
    path = Path(value).expanduser().resolve()
    if not stat.S_ISREG(path.stat().st_mode):
        raise ValueError(f"not a regular file: {path}")
    return path


def run(argv, timeout):
    """Always retain exact argv and partial output, including on a timeout."""
    result = {"argv": [str(arg) for arg in argv], "timeout_seconds": timeout,
              "returncode": None, "stdout": "", "stderr": ""}
    try:
        completed = subprocess.run(result["argv"], stdin=subprocess.DEVNULL,
                                   capture_output=True, timeout=timeout, check=False)
        result.update(status="ok" if completed.returncode == 0 else "failed",
                      returncode=completed.returncode,
                      stdout=completed.stdout.decode("utf-8", errors="replace"),
                      stderr=completed.stderr.decode("utf-8", errors="replace"))
    except subprocess.TimeoutExpired as exc:
        result.update(status="timeout",
                      stdout=(exc.stdout or b"").decode("utf-8", errors="replace"),
                      stderr=(exc.stderr or b"").decode("utf-8", errors="replace"))
    except OSError as exc:
        result.update(status="execution_failure", error=str(exc))
    return result


def host():
    return {"system": platform.system(), "release": platform.release(),
            "machine": platform.machine(), "processor": platform.processor(),
            "cpu_model_accuracy": "unverified; host identity does not establish a scheduling model"}


def locate(name, explicit=None):
    if explicit:
        candidates = [Path(explicit).expanduser()]
    else:
        found = shutil.which(name)
        candidates = ([Path(found)] if found else []) + [Path(d) / name for d in CANDIDATE_DIRS]
    checked = []
    for path in candidates:
        checked.append(str(path))
        try:
            resolved = regular_file(path)
            if not os.access(resolved, os.X_OK):
                continue
            return resolved, checked
        except (OSError, ValueError):
            continue
    return None, checked


def inspect_tool(name, explicit=None):
    path, checked = locate(name, explicit)
    info = {"name": name, "status": "missing", "path": None,
            "sha256": None, "checked_candidates": checked,
            "recognized_target_capability": {"status": "unknown", "targets": []}}
    if path is None:
        return info
    info.update(path=str(path), sha256=sha256(path))
    version = run([path, "--version"], PROBE_TIMEOUT)
    info["version_probe"] = version
    if version["status"] != "ok" or not version["stdout"].strip():
        info["status"] = "version_failure"
        return info
    info["status"] = "available"
    help_result = run([path, "--help"], PROBE_TIMEOUT)
    info["help_probe"] = help_result
    # LLVM tools commonly list Registered Targets in --version. Report only
    # evidence actually printed by this executable, never infer from its name.
    combined = version["stdout"] + "\n" + help_result["stdout"]
    registered = re.search(r"Registered Targets:\s*\n((?:[ \t]+[^\n]+\n?)*)", combined)
    targets = []
    if registered:
        targets = re.findall(r"^\s*([A-Za-z0-9_+-]+)\s+-\s+", registered[1], re.M)
    info["recognized_target_capability"] = {
        "status": "reported" if targets else "unknown", "targets": targets,
        "evidence": "version/help text only; target listing does not prove CPU model fidelity"}
    return info


def bounded_int(value):
    try:
        number = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError("expected an integer") from None
    if not 1 <= number <= MAX_ITERATIONS:
        raise argparse.ArgumentTypeError(f"must be in 1..{MAX_ITERATIONS}")
    return number


def token(value):
    if not TOKEN.fullmatch(value):
        raise argparse.ArgumentTypeError("expected a simple LLVM target/model name (max 128 characters)")
    return value


def timeout_value(value):
    try:
        number = float(value)
    except ValueError:
        raise argparse.ArgumentTypeError("expected seconds") from None
    if not 0.05 <= number <= 60:
        raise argparse.ArgumentTypeError("timeout must be in 0.05..60 seconds")
    return number


def analyze(args, report):
    source = regular_file(args.assembly)
    if source.stat().st_size > MAX_SOURCE_BYTES:
        raise ValueError(f"assembly exceeds {MAX_SOURCE_BYTES} bytes")
    report.update(source={"path": str(source), "sha256": sha256(source)},
                  iterations=args.iterations,
                  model={"cpu": args.cpu, "triple": args.triple, "arch": args.arch,
                         "selection": "user-declared",
                         "accuracy": "unverified",
                         "uncertainty": "LLVM acceptance does not establish host or Apple M5 fidelity; no calibration performed"},
                  metrics={})
    tool = inspect_tool("llvm-mca", args.llvm_mca)
    report["tool"] = tool
    if tool["status"] != "available":
        report.update(status="unsupported", error=f"llvm-mca {tool['status']}")
        return
    argv = [tool["path"], f"-mcpu={args.cpu}", f"-iterations={args.iterations}"]
    if args.triple:
        argv.append(f"-mtriple={args.triple}")
    if args.arch:
        argv.append(f"-march={args.arch}")
    argv.append(str(source))
    execution = run(argv, args.timeout)
    report["execution"] = execution
    # Treat diagnostics conservatively, even when LLVM returns success after
    # ignoring an unknown CPU or skipping an unsupported instruction.
    diagnostic = re.search(r"\b(error|warning|unsupported|unrecognized|not recognized|ignoring|no scheduling)\b",
                           execution["stdout"], re.I)
    if execution["status"] != "ok" or execution["stderr"].strip() or diagnostic:
        report.update(status="analysis_failed", error="LLVM execution failed or emitted diagnostics; no metrics accepted")
        return
    # Multiple code regions have multiple summaries. Preserve raw output rather
    # than merging or summing their costs. Require a recognizable summary.
    summary = re.findall(r"^Total Cycles:\s*(\d+)\s*$", execution["stdout"], re.M)
    if not summary:
        report.update(status="analysis_failed", error="no recognized MCA summary; no metrics accepted")
        return
    if len(summary) == 1:
        report["metrics"]["total_cycles"] = {"value": int(summary[0]), "unit": "modeled cycles"}
    report.update(status="analyzed", interpretation="static LLVM scheduling-model analysis only; no measured profit or correctness claim")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    discover = commands.add_parser("discover", help="version/help probes only; never execute a benchmark")
    mca = commands.add_parser("mca", help="analyze assembly under an explicit, unverified CPU model")
    for command in (discover, mca):
        command.add_argument("--output", required=True, help="fresh JSON file in an existing directory; never overwritten")
        command.add_argument("--llvm-mca", help="explicit executable path (no fallback if missing)")
    discover.add_argument("--llvm-exegesis", help="explicit executable path for discovery only")
    mca.add_argument("--assembly", required=True)
    mca.add_argument("--cpu", required=True, type=token)
    mca.add_argument("--triple", type=token)
    mca.add_argument("--arch", type=token)
    mca.add_argument("--iterations", type=bounded_int, default=100)
    mca.add_argument("--timeout", type=timeout_value, default=30)
    args = parser.parse_args(argv)
    report = {"schema": 1, "command": args.command, "status": "error", "native_host": host()}
    # Reserve the destination before probes/analysis; exclusive creation also
    # rejects existing symlinks and prevents accidental overwrites on retries.
    try:
        output = Path(args.output).expanduser().absolute()
        with output.open("x", encoding="utf-8") as stream:
            try:
                if args.command == "discover":
                    report.update(status="discovered", tools={
                        "llvm-mca": inspect_tool("llvm-mca", args.llvm_mca),
                        "llvm-exegesis": inspect_tool("llvm-exegesis", args.llvm_exegesis)},
                        native_benchmark_executed=False)
                else:
                    analyze(args, report)
            except (OSError, ValueError) as exc:
                report.update(status="error", error=str(exc))
            json.dump(report, stream, indent=2, allow_nan=False)
            stream.write("\n")
    except OSError as exc:
        print(f"cannot create/write output: {exc}", file=sys.stderr)
        return 2
    return 0 if report["status"] in ("discovered", "analyzed") else 1


if __name__ == "__main__":
    sys.exit(main())
