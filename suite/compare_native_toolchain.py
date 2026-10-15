#!/usr/bin/env python3
"""Same Keel assembly: compare assemblers, linkers, sizes and application runtime.

Prepared inputs are read only. Outputs, samples and hashes go under --out.
On macOS, isolate assembly with Apple's linker, then isolate linking with the
same Keel objects. On Linux both assembler outputs use the system linker.
"""

import argparse
import hashlib
import json
import platform
import random
import statistics
import struct
import subprocess
import time
from pathlib import Path


def invoke(argv, **kwargs):
    result = subprocess.run(
        [str(x) for x in argv], capture_output=True, timeout=300, check=False, **kwargs
    )
    if result.returncode:
        raise RuntimeError(
            f"{argv}: {result.returncode}\n{result.stderr.decode(errors='replace')}"
        )
    return result.stdout


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def text_section(path):
    data = path.read_bytes()
    if data[:4] == b"\x7fELF":
        offset = struct.unpack_from("<Q", data, 40)[0]
        size, count, strings = struct.unpack_from("<HHH", data, 58)
        headers = [
            struct.unpack_from("<IIQQQQIIQQ", data, offset + i * size)
            for i in range(count)
        ]
        h = headers[strings]
        names = data[h[4] : h[4] + h[5]]
        for h in headers:
            name = names[h[0] :].split(b"\0")[0]
            if name == b".text":
                return data[h[4] : h[4] + h[5]]
    else:
        assert struct.unpack_from("<I", data)[0] == 0xFEEDFACF
        at = 32
        for _ in range(struct.unpack_from("<I", data, 16)[0]):
            cmd, size = struct.unpack_from("<II", data, at)
            if cmd == 0x19:
                for i in range(struct.unpack_from("<I", data, at + 64)[0]):
                    s = at + 72 + i * 80
                    if data[s : s + 16].split(b"\0")[0] == b"__text":
                        length, start = struct.unpack_from("<QI", data, s + 40)
                        return data[start : start + length]
            at += size
    raise ValueError(f"no text section: {path}")


def paired_summary(samples, reference):
    result = {}
    rng = random.Random(42)
    for name, values in samples.items():
        ratios = [a / b for a, b in zip(values, samples[reference])]
        bootstrap = sorted(
            statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(2000)
        )
        result[name] = {
            "median_ms": statistics.median(values),
            "samples_ms": values,
            "median_paired_ratio": statistics.median(ratios),
            "paired_ratio_bootstrap_95": [bootstrap[50], bootstrap[1949]],
        }
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--keel", type=Path, required=True)
    p.add_argument("--prepared", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--pairs", type=int, default=5)
    p.add_argument("--assembly-pairs", type=int, default=5)
    a = p.parse_args()
    if a.pairs < 3 or a.assembly_pairs < 3:
        p.error("use at least three paired samples")
    root = Path(__file__).resolve().parents[1]
    a.keel, a.prepared, a.out = a.keel.resolve(), a.prepared.resolve(), a.out.resolve()
    if a.out == a.prepared or a.out.is_relative_to(a.prepared):
        p.error("output must be outside prepared inputs")
    a.out.mkdir(parents=True, exist_ok=True)
    mac = platform.system() == "Darwin"
    target = "aarch64-apple-darwin" if mac else "x86_64-unknown-linux-gnu"
    report = {
        "commit": invoke(["git", "rev-parse", "HEAD"], cwd=root).decode().strip(),
        "platform": platform.platform(),
        "host": platform.node(),
        "target": target,
        "keel_sha256": sha(a.keel),
        "clang": invoke(["clang", "--version"]).decode().splitlines()[0],
        "scope": "Warm serial subprocess wall time. Same Keel-generated assembly; same system linker for assembler comparison. macOS linker comparison uses identical Keel objects. Runtime includes process startup. No Clang C-codegen comparison.",
        "modes": {},
    }
    rng = random.Random(42)

    def save():
        (a.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    def measure(commands, pairs, *, stdin=None):
        samples = {name: [] for name in commands}
        expected = None
        # One untimed warmup per variant also checks output identity.
        for command in commands.values():
            output = invoke(command, input=stdin)
            if expected is None:
                expected = output
            assert output == expected, f"output mismatch: {command}"
        for _ in range(pairs):
            order = list(commands)
            rng.shuffle(order)
            for name in order:
                start = time.perf_counter_ns()
                output = invoke(commands[name], input=stdin)
                samples[name].append((time.perf_counter_ns() - start) / 1e6)
                assert output == expected, f"output mismatch: {commands[name]}"
        return samples

    for mode, flags in [("baseline", []), ("opt", ["-O"])]:
        entry = {"flags": flags, "inputs": [], "assembly": {}, "suites": {}}
        report["modes"][mode] = entry
        batches = {"keel": [], "clang_driver": [], "clang_cc1as": []}
        if mac:
            batches["clang_default"] = []
        objects = {}
        for suite in ["lua", "quickjs", "sqlite"]:
            out = a.out / mode / suite
            out.mkdir(parents=True, exist_ok=True)
            inputs = sorted((a.prepared / suite / "pre").glob("*.i"))
            if not inputs:
                raise RuntimeError(f"missing inputs: {suite}")
            objects[suite] = {"keel": [], "clang": []}
            for source in inputs:
                asm = out / (source.stem + ".s")
                invoke([a.keel, "cc", *flags, source, "-o", asm])
                ko, co, direct = [
                    out / (source.stem + suffix)
                    for suffix in [".keel.o", ".clang.o", ".direct.o"]
                ]
                commands = {
                    "keel": [a.keel, "asm", "--target", target, asm, "-o", ko],
                    "clang_driver": ["clang", "-x", "assembler", "-c", asm, "-o", co],
                    "clang_cc1as": [
                        "clang",
                        "-cc1as",
                        "-triple",
                        target,
                        "-filetype",
                        "obj",
                        "-target-cpu",
                        "apple-m1" if mac else "x86-64",
                        "-mrelocation-model",
                        "pic",
                        asm,
                        "-o",
                        direct,
                    ],
                }
                if mac:
                    commands["clang_default"] = [
                        "clang",
                        "-c",
                        asm,
                        "-o",
                        out / (source.stem + ".default.o"),
                    ]
                for name, command in commands.items():
                    invoke(command)
                    batches[name].append(command)
                kt, ct, dt = [text_section(x) for x in [ko, co, direct]]
                assert ct == dt, f"Clang direct/driver mismatch: {asm}"
                entry["inputs"].append(
                    {
                        "suite": suite,
                        "source": str(source),
                        "source_sha256": sha(source),
                        "asm": str(asm),
                        "asm_sha256": sha(asm),
                        "asm_bytes": asm.stat().st_size,
                        "text_identical": kt == ct,
                        "keel_text_bytes": len(kt),
                        "clang_text_bytes": len(ct),
                        "keel_object_bytes": ko.stat().st_size,
                        "clang_object_bytes": co.stat().st_size,
                    }
                )
                objects[suite]["keel"].append(ko)
                objects[suite]["clang"].append(co)
        assert len(entry["inputs"]) == 44
        print(f"{mode}: generated and assembled 44 identical sources", flush=True)
        samples = {name: [] for name in batches}
        # Objects are warm from verification above. No C compilation in timing.
        for _ in range(a.assembly_pairs):
            order = list(batches)
            rng.shuffle(order)
            for name in order:
                start = time.perf_counter_ns()
                for command in batches[name]:
                    invoke(command)
                samples[name].append((time.perf_counter_ns() - start) / 1e6)
        entry["assembly"] = paired_summary(samples, "clang_driver")
        save()
        for suite, binary in [
            ("lua", "lua"),
            ("quickjs", "qjs"),
            ("sqlite", "sqlite3"),
        ]:
            out = a.out / mode / suite
            variants = {
                "keel_asm_system_link": objects[suite]["keel"],
                "clang_asm_system_link": objects[suite]["clang"],
            }
            if mac:
                variants["keel_asm_keel_link"] = objects[suite]["keel"]
            link_commands, executables = {}, {}
            for variant, paths in variants.items():
                exe = out / (binary + "-" + variant)
                command = (
                    [a.keel, "link"] if variant.endswith("keel_link") else ["clang"]
                )
                command += (
                    [*paths, "-lm", "-lpthread"]
                    + ([] if mac else ["-ldl"])
                    + ["-o", exe]
                )
                invoke(command)
                link_commands[variant], executables[variant] = command, exe
            link_samples = {name: [] for name in variants}
            for _ in range(a.assembly_pairs):
                order = list(variants)
                rng.shuffle(order)
                for name in order:
                    start = time.perf_counter_ns()
                    invoke(link_commands[name])
                    link_samples[name].append((time.perf_counter_ns() - start) / 1e6)
            results = {
                "link": paired_summary(link_samples, "keel_asm_system_link"),
                "executables": {
                    name: {
                        "path": str(exe),
                        "sha256": sha(exe),
                        "bytes": exe.stat().st_size,
                        "text_bytes": len(text_section(exe)),
                    }
                    for name, exe in executables.items()
                },
                "benchmarks": {},
            }
            entry["suites"][suite] = results
            benches = [("fib", None), ("sort", None), ("string", None)]
            if suite == "lua":
                benches += [("float", None)]
            if suite == "sqlite":
                benches = [("queries", (root / "suite/sqlite/bench.sql").read_bytes())]
            for bench, stdin in benches:
                if suite == "lua":
                    args = [root / "suite/lua/bench.lua", bench]
                elif suite == "quickjs":
                    args = [
                        "--stack-size",
                        "4194304",
                        root / "suite/quickjs/bench.js",
                        bench,
                    ]
                else:
                    args = [":memory:"]
                commands = {name: [exe, *args] for name, exe in executables.items()}
                values = measure(commands, a.pairs, stdin=stdin)
                results["benchmarks"][bench] = paired_summary(
                    values, "clang_asm_system_link"
                )
                print(
                    f"PASS {mode}/{suite}/{bench}: "
                    + ", ".join(
                        f"{name}={statistics.median(v):.2f}ms"
                        for name, v in values.items()
                    ),
                    flush=True,
                )
                save()
            # This includes process startup, dyld/loader work and a trivial main.
            args = ["-e", ""] if suite != "sqlite" else [":memory:"]
            commands = {name: [exe, *args] for name, exe in executables.items()}
            results["startup"] = paired_summary(
                measure(commands, 15, stdin=b""), "clang_asm_system_link"
            )
            save()
    print(f"Completed: {a.out / 'report.json'}", flush=True)


if __name__ == "__main__":
    main()
