#!/usr/bin/env python3
"""Exact A64 encoding inventory and assembly costs, with optional LLVM CPU estimates.

No CPU timing is inferred from encoding bits. Analyze small straight-line regions,
not whole functions/translation units, when requesting llvm-mca estimates.
"""

import argparse
import hashlib
import json
import math
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
ISA = ROOT / "crates/keel-asm/isa/a64.json"
MCA_DOC = "https://llvm.org/docs/CommandGuide/llvm-mca.html"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def catalog():
    data = json.loads(ISA.read_text())
    records = []
    for inst in data["instructions"]:
        encoding = inst["encoding"]
        if encoding["width"] != 32:
            raise ValueError("expected fixed-width A64 encodings")
        mask = base = 0
        for field in encoding["values"]:
            start = field["range"]["start"]
            bits = field["value"]["value"].strip("'")
            for offset, bit in enumerate(reversed(bits)):
                if bit != "x":
                    mask |= 1 << (start + offset)
                    base |= int(bit) << (start + offset)
        records.append({
            "encoding": inst["name"], "bytes": 4, "mask": mask, "base": base,
            "latency_cycles": None, "reciprocal_throughput_cycles": None,
            "micro_ops": None, "cpu": None,
        })
    return {
        "schema_version": 1, "architecture": "aarch64",
        "provenance": {"source_url": data["source_url"],
                       "archive_sha256": data["archive_sha256"],
                       "isa_sha256": digest(ISA)},
        "timing_coverage": 0, "encoding_count": len(records), "encodings": records,
    }


def run(command):
    proc = subprocess.run(command, text=True, capture_output=True, check=False)
    if proc.returncode:
        raise ValueError(f"{command[0]} exited {proc.returncode}: {proc.stderr.strip()}")
    return proc


def nonnegative(value, name):
    if isinstance(value, bool) or not isinstance(value, (float, int)):
        raise ValueError(f"invalid {name}: {value!r}")
    if not math.isfinite(value) or value < 0:
        raise ValueError(f"invalid {name}: {value!r}")
    return value


def normalize_mca(raw, cpu, expected_instructions):
    """Keep CPU identity and missing/unsupported model data explicit."""
    if raw["TargetInfo"]["CPUName"] != cpu:
        raise ValueError("llvm-mca CPU differs from requested CPU; refusing fallback")
    if raw["SimulationParameters"]["-mcpu"] != cpu:
        raise ValueError("llvm-mca simulation CPU differs from requested CPU")
    regions = raw["CodeRegions"]
    if len(regions) != 1:
        raise ValueError("analyze exactly one llvm-mca region at a time")
    region = regions[0]
    instructions = region["Instructions"]
    info = region["InstructionInfoView"]["InstructionList"]
    if len(instructions) != expected_instructions or len(info) != expected_instructions:
        raise ValueError("llvm-mca and Keel disagree on instruction count")
    if sorted(row["Instruction"] for row in info) != list(range(len(instructions))):
        raise ValueError("llvm-mca instruction indices are incomplete or duplicated")
    rows = []
    for row in sorted(info, key=lambda row: row["Instruction"]):
        rows.append({
            "assembly": instructions[row["Instruction"]],
            "latency_cycles": nonnegative(row["Latency"], "latency"),
            "reciprocal_throughput_cycles": nonnegative(row["RThroughput"], "throughput"),
            "micro_ops": nonnegative(row["NumMicroOpcodes"], "micro-ops"),
            "may_load": row["mayLoad"], "may_store": row["mayStore"],
            "unmodeled_side_effects": row["hasUnmodeledSideEffects"],
        })
    summary = region["SummaryView"]
    iterations = nonnegative(summary["Iterations"], "iterations")
    if not iterations:
        raise ValueError("llvm-mca returned zero iterations")
    cycles = nonnegative(summary["TotalCycles"], "total cycles")
    # A model can report zero throughput for missing scheduling data.
    complete = all(row["reciprocal_throughput_cycles"] > 0
                   and not row["unmodeled_side_effects"] for row in rows)
    return {
        "cpu": cpu, "source": MCA_DOC, "kind": "llvm-scheduling-model",
        "iterations": iterations,
        "estimated_cycles_per_iteration": cycles / iterations if complete else None,
        "block_reciprocal_throughput_cycles": summary.get("BlockRThroughput"),
        "instructions": rows,
        "resource_pressure": region.get("ResourcePressureView"),
        "resources": raw["TargetInfo"].get("Resources", []),
        "complete": complete,
    }


def representative_word(instruction):
    """Choose one encoding sample, not a claim to cover all operand variants."""
    row = next(r for r in catalog()["encodings"] if r["encoding"] == instruction["name"])
    word = row["base"]
    registers = {"Rd": 0, "Rt": 0, "Rn": 1, "Rm": 2, "Ra": 3, "Rs": 2, "Rt2": 3}
    for field in instruction["encoding"]["values"]:
        name = field.get("name")
        value = registers.get(name, 0)
        if name == "option" and "regoff" in instruction["name"]:
            value = 3
        if name == "immh":
            value = 1
        if name == "CRm" and "barriers" in instruction["name"]:
            value = 15
        start, width = field["range"]["start"], field["range"]["width"]
        variable = (((1 << width) - 1) << start) & ~row["mask"]
        word = (word & ~variable) | ((value << start) & variable)
    return word


def canonical_syntax(assembly):
    # Darwin's printer puts vector arrangements on the mnemonic. Keel accepts
    # the architecture's operand notation; exact encoding round-trip checks it.
    match = re.match(r"([a-z0-9]+)\.([0-9]+[bhsd])\s+(.*)", assembly)
    if match:
        op, arrangement, operands = match.groups()
        return op + " " + re.sub(r"\bv([0-9]+)\b", r"v\1." + arrangement, operands)
    return assembly


def modeled_catalog(keel, cpu, mca, llvm_mc):
    """Round-trip a sample per encoding before querying the CPU model.

    Unsupported/reserved sample forms remain explicitly unknown. A sample's
    result never supplies a timing for every possible operand of an encoding.
    """
    result = catalog()
    isa = json.loads(ISA.read_text())
    mca_path, mc_path = shutil.which(str(mca)), shutil.which(str(llvm_mc))
    if not mca_path or not mc_path:
        raise ValueError("modeled catalog requires llvm-mca and llvm-mc")
    # Validate CPU once up front; an invalid model must not look like 534 gaps.
    with tempfile.TemporaryDirectory(prefix="keel-cost-catalog-") as temporary:
        source = Path(temporary) / "sample.s"
        source.write_text("add x0, x0, x1\n")
        probe = analyze(source, keel, cpu, mca_path, 1)
        if not probe["performance"]["complete"]:
            raise ValueError("CPU model cannot completely model the initial add probe")
        for inst, row in zip(isa["instructions"], result["encodings"]):
            word = representative_word(inst)
            encoded = " ".join(f"0x{b:02x}" for b in word.to_bytes(4, "little"))
            disassembly = subprocess.run(
                [mc_path, "--disassemble", "--triple=aarch64-apple-darwin", "--mattr=+lse"],
                input=encoded, text=True, capture_output=True, check=False,
            )
            lines = [line.strip() for line in disassembly.stdout.splitlines()
                     if line.strip() and not line.strip().startswith(".")]
            if disassembly.returncode or disassembly.stderr.strip() or len(lines) != 1:
                row["unknown_reason"] = "LLVM cannot disassemble representative encoding"
                continue
            assembly = canonical_syntax(lines[0])
            source.write_text(assembly + "\n")
            try:
                row["sample"] = {"assembly": assembly, "word": word}
                report = analyze(source, keel, cpu, mca_path, 1, expected_word=word)
                timing = report["performance"]
                if not timing["complete"]:
                    raise ValueError(timing.get("diagnostics") or "incomplete CPU scheduling model")
                row.update(timing["instructions"][0])
                row["cpu"] = cpu
                row["resource_pressure"] = timing["resource_pressure"]
            except (ValueError, KeyError, TypeError) as error:
                row["unknown_reason"] = str(error).replace(str(source), "<sample>")
    result["timing_coverage"] = sum(row["cpu"] is not None for row in result["encodings"])
    result["model"] = {"cpu": cpu, "kind": "llvm-scheduling-model", "source": MCA_DOC,
                       "llvm_mca_sha256": digest(mca_path), "llvm_mc_sha256": digest(mc_path),
                       "llvm_version": run([mca_path, "--version"]).stdout.strip(),
                       "assembler_sha256": digest(keel),
                       "resources": probe["performance"]["resources"],
                       "scope": "One round-tripped operand sample per encoding; timings are not hardware measurements or guarantees for other operands."}
    return result


def analyze(source, keel, cpu=None, mca="llvm-mca", iterations=100, expected_word=None):
    source = Path(source).resolve()
    proc = run([str(keel), "asm-metrics", str(source)])
    exact = json.loads(proc.stdout)
    if exact["schema_version"] != 1 or exact["target"] != "aarch64-apple-darwin":
        raise ValueError("unsupported assembler metrics schema/target")
    if expected_word is not None:
        actual = exact["instructions"]
        if len(actual) != 1 or actual[0]["word"] != expected_word:
            raise ValueError("representative encoding does not round-trip through Keel")
    inventory = catalog()
    for inst in exact["instructions"]:
        # Masks identify candidate encoding forms, not semantic disassembly.
        # Preserve ambiguity; never select the first matching form as a cost key.
        inst["encoding_candidates"] = [row["encoding"] for row in inventory["encodings"]
                                        if inst["word"] & row["mask"] == row["base"]]
        inst["bytes"] = 4
    report = {
        "schema_version": 1, "source": str(source), "source_sha256": digest(source),
        "assembler_sha256": digest(keel), "isa_sha256": digest(ISA),
        "exact": exact, "performance": None,
        "limitations": ["Static counts are not execution frequencies.",
                        "ISA encodings do not specify CPU latency or throughput."],
    }
    if cpu:
        executable = shutil.which(str(mca))
        if not executable:
            raise ValueError(f"{mca} is unavailable; omit --cpu for exact size metrics")
        if exact["text_bytes"] != exact["instruction_bytes"]:
            raise ValueError("CPU analysis requires a region without embedded text data/padding")
        command = [executable, "-mtriple=aarch64-apple-darwin", f"-mcpu={cpu}",
                   "-json", "-instruction-info", "-resource-pressure",
                   "-noalias=false", f"-iterations={iterations}", str(source)]
        result = run(command)
        performance = normalize_mca(json.loads(result.stdout), cpu, len(exact["instructions"]))
        performance.update({"command": command, "tool_sha256": digest(executable),
                            "tool_version": run([executable, "--version"]).stdout.strip(),
                            "diagnostics": result.stderr.strip()})
        # Warnings such as modeled call latency must not become trusted scores.
        if result.stderr.strip():
            performance["complete"] = False
            performance["estimated_cycles_per_iteration"] = None
        report["performance"] = performance
        report["limitations"] += ["CPU estimates are not hardware measurements.",
                                  "Compare equivalent straight-line regions only; branch prediction, cache misses and whole-program behavior are not modeled."]
    return report


def compare(before, after, objective):
    for key in ["schema_version", "isa_sha256", "assembler_sha256"]:
        if before[key] != after[key]:
            raise ValueError(f"incompatible reports: {key}")
    if before["exact"]["target"] != after["exact"]["target"]:
        raise ValueError("incompatible targets")
    if objective == "size":
        a, b = before["exact"]["text_bytes"], after["exact"]["text_bytes"]
        unit = "bytes"
    else:
        p, q = before["performance"], after["performance"]
        if not p or not q or not p["complete"] or not q["complete"]:
            raise ValueError("speed comparison requires complete CPU models for both regions")
        for key in ["cpu", "tool_sha256", "iterations"]:
            if p[key] != q[key]:
                raise ValueError(f"incompatible CPU models: {key}")
        a, b = p["estimated_cycles_per_iteration"], q["estimated_cycles_per_iteration"]
        unit = "estimated cycles per iteration"
    return {"objective": objective, "unit": unit, "before": a, "after": b,
            "delta": b - a, "preferred": "after" if b < a else "before" if a < b else "tie",
            "requires_semantic_equivalence": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-o", "--output", type=Path)
    commands = parser.add_subparsers(dest="command", required=True)
    inventory = commands.add_parser("catalog")
    inventory.add_argument("--cpu")
    inventory.add_argument("--keel", type=Path, default=ROOT / "target/release/keel")
    inventory.add_argument("--llvm-mca", default="llvm-mca")
    inventory.add_argument("--llvm-mc", default="llvm-mc")
    analysis = commands.add_parser("analyze")
    analysis.add_argument("source", type=Path)
    analysis.add_argument("--keel", type=Path, default=ROOT / "target/release/keel")
    analysis.add_argument("--cpu")
    analysis.add_argument("--llvm-mca", default="llvm-mca")
    analysis.add_argument("--iterations", type=int, default=100)
    comparison = commands.add_parser("compare")
    comparison.add_argument("before", type=Path)
    comparison.add_argument("after", type=Path)
    comparison.add_argument("--objective", choices=["size", "speed"], required=True)
    args = parser.parse_args()
    try:
        if args.command == "catalog":
            result = (modeled_catalog(args.keel.resolve(), args.cpu, args.llvm_mca, args.llvm_mc)
                      if args.cpu else catalog())
        elif args.command == "analyze":
            if args.iterations < 1:
                raise ValueError("iterations must be positive")
            result = analyze(args.source, args.keel.resolve(), args.cpu, args.llvm_mca, args.iterations)
        else:
            result = compare(json.loads(args.before.read_text()), json.loads(args.after.read_text()), args.objective)
    except (ValueError, KeyError, OSError, TypeError) as error:
        parser.exit(1, f"instruction metrics: {error}\n")
    output = json.dumps(result, indent=2, allow_nan=False) + "\n"
    if args.output:
        args.output.write_text(output)
    else:
        sys.stdout.write(output)


if __name__ == "__main__":
    main()
