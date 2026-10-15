#!/usr/bin/env python3
"""Repeat runtime measurements on existing same-source toolchain outputs.

Optional Linux CPU affinity is inherited by every child, without changing any
system configuration. Keep the original report as the assembly/size evidence.
"""

import argparse
import json
import os
import random
import subprocess
import time
from pathlib import Path

from compare_native_toolchain import paired_summary

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--report", type=Path, required=True)
p.add_argument("--out", type=Path, required=True)
p.add_argument("--cpu", type=int)
p.add_argument("--pairs", type=int, default=11)
a = p.parse_args()
if a.pairs < 3:
    p.error("use at least three paired samples")
if a.cpu is not None:
    if not hasattr(os, "sched_setaffinity"):
        p.error("CPU affinity requires Linux")
    os.sched_setaffinity(0, {a.cpu})
root = Path(__file__).resolve().parents[1]
d = json.loads(a.report.read_text())
report = {
    "cpu_affinity": sorted(os.sched_getaffinity(0))
    if hasattr(os, "sched_getaffinity")
    else None,
    "pairs": a.pairs,
    "warmups": 2,
    "source_report": str(a.report.resolve()),
    "modes": {},
}
rng = random.Random(73)
for mode, entry in d["modes"].items():
    report["modes"][mode] = {}
    for suite, data in entry["suites"].items():
        result = {"benchmarks": {}}
        report["modes"][mode][suite] = result
        for bench in data["benchmarks"]:
            stdin = None
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
                args, stdin = (
                    [":memory:"],
                    (root / "suite/sqlite/bench.sql").read_bytes(),
                )
            commands = {
                name: [info["path"], *args]
                for name, info in data["executables"].items()
            }
            samples = {name: [] for name in commands}
            expected = None
            for iteration in range(2 + a.pairs):
                order = list(commands)
                rng.shuffle(order)
                for name in order:
                    start = time.perf_counter_ns()
                    out = subprocess.run(
                        list(map(str, commands[name])),
                        input=stdin,
                        capture_output=True,
                        check=True,
                        timeout=120,
                    )
                    elapsed = (time.perf_counter_ns() - start) / 1e6
                    if expected is None:
                        expected = out.stdout
                    assert out.stdout == expected, commands[name]
                    if iteration >= 2:
                        samples[name].append(elapsed)
            result["benchmarks"][bench] = paired_summary(
                samples, "clang_asm_system_link"
            )
            a.out.write_text(json.dumps(report, indent=2) + "\n")
            ratio = result["benchmarks"][bench]["keel_asm_system_link"][
                "median_paired_ratio"
            ]
            print(f"{mode}/{suite}/{bench}: {ratio:.4f}", flush=True)
