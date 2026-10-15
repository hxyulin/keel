#!/usr/bin/env python3
"""Add static IR counts to an existing paired pipeline report using its exact inputs."""
import argparse
import collections
import hashlib
import json
import re
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def count(text):
    counts = collections.Counter()
    in_block = False
    function_lines = []
    for line in text.splitlines():
        if line.startswith("block"):
            counts["blocks"] += 1
            in_block = True
        elif line == "}":
            literals = set()
            for instruction in function_lines:
                match = re.match(r"\s*(v[0-9]+) = iconst\.", instruction)
                if match:
                    literals.add(match.group(1))
            for instruction in function_lines:
                expression = instruction.strip().split(" = ")[-1]
                opcode = expression.split()[0]
                if opcode.startswith("icmp.") or opcode in ["iadd", "imul", "and", "or", "xor", "umulhi", "smulhi"]:
                    operands = re.findall(r"\bv[0-9]+\b", expression)
                    if len(operands) == 2 and operands[0] in literals and operands[1] not in literals:
                        metric = "left_comparison_literals" if opcode.startswith("icmp.") else "left_integer_literals"
                        counts[metric] += 1
            function_lines = []
            in_block = False
        elif in_block and line.startswith("    "):
            function_lines.append(line)
            opcode = line.strip().split(" = ")[-1].split()[0].split(".")[0]
            counts["instructions"] += 1
            counts[opcode] += 1
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--baseline-compiler", type=Path)
    parser.add_argument("--extra", action="append", default=[], help="NAME=comma,separated,passes for static-only additional recipes")
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    compiler = args.compiler.resolve()
    baseline = (args.baseline_compiler or args.compiler).resolve()
    for name, expected_hash in report["compiler_sha256"].items():
        if expected_hash != digest(baseline if name == "baseline" else compiler):
            parser.error(f"immutable compiler hash mismatch: {name}")
    pipelines = dict(report["pipelines"])
    for extra in args.extra:
        name, _, passes = extra.partition("=")
        if not name or not passes or name in pipelines:
            parser.error("invalid or duplicate extra recipe")
        pipelines[name] = passes.split(",")
    totals = {name: {} for name in pipelines}
    files = []
    for source, expected_hash in report["sources_sha256"].items():
        path = ROOT / source
        if digest(path) != expected_hash:
            parser.error(f"source hash mismatch: {source}")
        suite = Path(source).parts[2]
        row = {"source": source, "variants": {}}
        for name, passes in pipelines.items():
            # The simplification sweep uses no extra target/selection flags.
            # Refuse silently dropping flags if this tool is reused elsewhere.
            if report.get("compiler_arguments", {}).get(name):
                parser.error("this census expects reports without extra compiler arguments")
            driver = baseline if name == "baseline" else compiler
            ir = subprocess.run([str(driver), "cc", "--emit-ir", "-p", ",".join(passes), str(path)],
                                text=True, capture_output=True, check=True).stdout
            counts = count(ir)
            totals[name].setdefault(suite, collections.Counter()).update(counts)
            row["variants"][name] = {"counts": dict(counts), "ir_sha256": hashlib.sha256(ir.encode()).hexdigest()}
        files.append(row)
    report["static_ir"] = {"method": "Count printed instructions within IR blocks after each exact recipe; static counts are not execution frequencies. Extra recipes are static-only, not runtime measurements.",
                           "script_sha256": digest(Path(__file__)), "pipelines": pipelines,
                           "totals": totals, "files": files}
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    for name, suites in totals.items():
        print(name, {suite: {key: counts[key] for key in ["instructions", "blocks", "icmp", "and", "or", "trunc", "uext", "sext", "left_integer_literals", "left_comparison_literals"]} for suite, counts in suites.items()}, flush=True)


if __name__ == "__main__":
    main()
