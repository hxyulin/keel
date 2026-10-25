#!/usr/bin/env python3
"""Compare two Keel linkers on identical generated symbol-heavy inputs (macOS)."""
import argparse
import hashlib
import json
import os
import platform
import random
import re
import statistics
import subprocess
import time
from pathlib import Path


def run(command, **kwargs):
    return subprocess.run(list(map(str, command)), capture_output=True, **kwargs)


def checked(command):
    result = run(command, timeout=120)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace"))
    return result


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs(directory, count, elf):
    # Long common prefixes model mangled names. Shards form an archive demand
    # chain through data relocations; each also references every local function.
    prefix = "_ZN4keel24collections_benchmark17shared_namespace20symbol_heavy_workload"
    def name(i):
        return f"{prefix}{i:08d}Ev"

    objects = []
    for shard, start in enumerate(range(0, count, 2048)):
        end = min(start + 2048, count)
        lines = [".text"]
        for i in range(start, end):
            lines.extend([f".globl {name(i)}", f"{name(i)}:", "ret"])
        lines.append(".data")
        lines.extend(f".quad {name(i)}" for i in range(start, end))
        if end < count:
            lines.append(f".quad {name(end)}")
        source = directory / f"shard-{shard}.s"
        source.write_text("\n".join(lines) + "\n")
        obj = source.with_suffix(".o")
        checked(["clang", "-target", "x86_64-unknown-linux-gnu" if elf else
                 "arm64-apple-macosx11.0", "-c", source, "-o", obj])
        objects.append(obj)
    source = directory / "entry.s"
    source.write_text((".text\n.globl _start\n_start:\nmov $60, %eax\n"
                       "xor %edi, %edi\nsyscall\n" if elf else
                       ".text\n.globl _main\n_main:\nmov w0, #0\nret\n") +
                      f".data\n.quad {name(0)}\n")
    entry = source.with_suffix(".o")
    checked(["clang", "-target", "x86_64-unknown-linux-gnu" if elf else
             "arm64-apple-macosx11.0", "-c", source, "-o", entry])
    archive = directory / "members.a"
    # Apple's ar silently omits ELF objects. Write a regular archive without
    # an index: both linkers build their own provider information from members.
    with archive.open("wb") as stream:
        stream.write(b"!<arch>\n")
        for obj in reversed(objects):
            data = obj.read_bytes()
            stream.write((f"{obj.name + '/':<16}{0:<12}{0:<6}{0:<6}"
                          f"{'100644':<8}{len(data):<10}`\n").encode())
            stream.write(data)
            if len(data) % 2:
                stream.write(b"\n")
    return entry, objects, archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--symbols", type=int, default=65536)
    parser.add_argument("--rounds", type=int, default=9)
    args = parser.parse_args()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("requires arm64 macOS and Apple's cross-target assembler")
    if args.symbols < 4096 or args.rounds < 3:
        parser.error("use at least 4096 symbols and 3 rounds")
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    binaries = {"std": args.baseline.resolve(), "bbtree": args.candidate.resolve()}
    sdk = checked(["xcrun", "--show-sdk-path"]).stdout.decode().strip()
    report = {"platform": platform.platform(), "cpu": checked([
        "sysctl", "-n", "machdep.cpu.brand_string"]).stdout.decode().strip(),
        "source_commit": checked(["git", "rev-parse", "HEAD"]).stdout.decode().strip(),
        "source_diff_sha256": hashlib.sha256(checked(["git", "diff"]).stdout).hexdigest(),
        "harness_sha256": digest(Path(__file__)), "load_before": os.getloadavg(),
        "binaries": {k: {"path": str(v), "sha256": digest(v)} for k, v in binaries.items()},
        "symbols": args.symbols, "warmups": 2, "rounds": args.rounds,
        "seed": 20261025, "cases": {}}
    rng = random.Random(report["seed"])
    for elf, count in [(False, 64), (False, args.symbols), (True, args.symbols)]:
        label = f"{'elf' if elf else 'macho'}-{count}"
        directory = args.out / label
        directory.mkdir()
        entry, objects, archive = inputs(directory, count, elf)
        flags = (["--target", "x86_64-unknown-linux-gnu", "--static"] if elf else
                 ["--sysroot", sdk, "-mmacosx-version-min=11.0"])
        for archived in ([False] if count == 64 else [False, True]):
            case = label + ("-archive" if archived else "-direct")
            output = directory / "program"
            command = ["link", *flags, entry, *([archive] if archived else objects), "-o", output]
            samples = {k: [] for k in binaries}
            hashes, memory, orders = {}, {}, []
            for key, binary in binaries.items():
                measured = checked(["/usr/bin/time", "-l", binary, *command])
                memory[key] = int(re.search(rb"(\d+)\s+maximum resident set size", measured.stderr)[1])
                hashes[key] = digest(output)
                if not elf:
                    checked(["codesign", "--verify", "--strict", output])
                    checked([output])
            if len(set(hashes.values())) != 1:
                raise RuntimeError(f"{case}: output bytes differ")
            # Check failure status and exact diagnostics using the same paths.
            failures = {}
            for kind, bad in [("duplicate", [entry, entry, *objects]),
                              ("undefined", [entry])]:
                results = [run([binary, "link", *flags, *bad, "-o", output], timeout=120)
                           for binary in binaries.values()]
                if (not results[0].returncode or
                    (results[0].returncode, results[0].stdout, results[0].stderr) !=
                    (results[1].returncode, results[1].stdout, results[1].stderr)):
                    raise RuntimeError(f"{case}: {kind} diagnostics differ or succeeded")
                failures[kind] = results[0].stderr.decode()
            for iteration in range(args.rounds + 2):
                order = list(binaries)
                rng.shuffle(order)
                orders.append(order)
                for key in order:
                    start = time.perf_counter_ns()
                    checked([binaries[key], *command])
                    elapsed = (time.perf_counter_ns() - start) / 1e6
                    if iteration >= 2:
                        samples[key].append(elapsed)
            if digest(output) != hashes["std"]:
                raise RuntimeError(f"{case}: output changed during timing")
            medians = {k: statistics.median(v) for k, v in samples.items()}
            report["cases"][case] = {"command": list(map(str, command)),
                "inputs": {str(p): digest(p) for p in [entry, *objects, archive]},
                "output_sha256": hashes, "failure_diagnostics": failures,
                "samples_ms": samples, "median_ms": medians,
                "candidate_change_percent": 100 * (medians["bbtree"] / medians["std"] - 1),
                "peak_rss_bytes_single_run": memory, "orders_including_warmups": orders}
            (args.out / "raw.json").write_text(json.dumps(report, indent=2) + "\n")
            print(case, json.dumps(medians), flush=True)
    report["load_after"] = os.getloadavg()
    for key, binary in binaries.items():
        if digest(binary) != report["binaries"][key]["sha256"]:
            raise RuntimeError(f"{key}: binary changed during timing")
    (args.out / "raw.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
