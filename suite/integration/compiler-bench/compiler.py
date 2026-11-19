#!/usr/bin/env python3
"""Correctness-gated compiler compile-time benchmark.

Requires prebuilt release binaries. Outputs only to a new directory. C driver
startup and object writes are included; IR backend timings reuse the existing
in-process harness, whose rotating order is recorded separately.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
CONFIGS = ["keel-baseline", "keel-full", "clif-none", "clif-speed"]
KERNELS = "logic fnv1a crc32 sieve matmul dot saxpy divmix gcd isort qsort interp classify bits narrow fconv collatz copy dispatch names stackobj".split()


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def time_kernels(run, out, rounds, warmups, seed):
    configurations = ["clang-O2", "raw-keel-full", "raw-clif-speed", "O-keel-full", "O-clif-speed"]
    images = {name: out / f"check-{name}" for name in configurations}
    control = out / "runtime-control"
    shutil.copy2(images["O-keel-full"], control)
    images["control"] = control
    rng = random.Random(seed)
    workloads = {}
    for kernel in KERNELS:
        def execute(name, reps):
            actual, ns, checksum = run([images[name], "time", kernel, reps]).decode().split()
            if actual != kernel:
                raise RuntimeError(f"unexpected kernel {actual!r}")
            return int(ns), checksum

        calibrations = [execute(name, 32)[0] for name in configurations]
        reps = max(1, min(200_000, round(32 * 20_000_000 / max(1, max(calibrations)))))
        expected = execute("clang-O2", reps)[1]
        samples = {name: [] for name in images}
        orders = []
        for index in range(warmups + rounds):
            order = list(images)
            rng.shuffle(order)
            orders.append(order)
            for name in order:
                ns, checksum = execute(name, reps)
                if checksum != expected:
                    raise RuntimeError(f"{kernel} checksum mismatch in {name}")
                if index >= warmups:
                    samples[name].append(ns)
        workloads[kernel] = {"reps": reps, "checksum": expected, "samples_ns": samples,
                             "orders_including_warmups": orders}
    return workloads


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--keel", type=Path, required=True)
    p.add_argument("--bench", type=Path, required=True)
    p.add_argument("--clang", default="clang")
    p.add_argument("--correctness-only", action="store_true",
                   help="run support/object/output gates and stop before all timing rounds")
    p.add_argument("--runtime", action="store_true", help="also time the in-process kernel workloads")
    p.add_argument("--warmups", type=int, default=2)
    p.add_argument("--rounds", type=int, default=7)
    p.add_argument("--seed", type=int, default=20261024)
    a = p.parse_args()
    if a.warmups < 1 or a.rounds < 3:
        p.error("require at least one warmup and three rounds")
    if a.runtime and a.correctness_only:
        p.error("--runtime and --correctness-only are mutually exclusive")
    host = (platform.system(), platform.machine())
    if host == ("Darwin", "arm64"):
        target_keel = "aarch64-apple-darwin"
        target_clang = "arm64-apple-macosx11.0.0"
        cpu_cmd = ["sysctl", "-n", "machdep.cpu.brand_string"]
        hardware_cmd = ["sysctl", "hw.model", "hw.memsize", "hw.ncpu"]
        os_cmd = ["sw_vers"]
    elif host == ("Linux", "x86_64"):
        target_keel = target_clang = "x86_64-unknown-linux-gnu"
        cpu_cmd = ["uname", "-m"]
        hardware_cmd = ["lscpu"]
        os_cmd = ["uname", "-sr"]
    else:
        p.error(f"unsupported benchmark host: {host}")
    a.out = a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=False)
    a.keel, a.bench = a.keel.resolve(), a.bench.resolve()
    clang = str(Path(shutil.which(a.clang) or a.clang).resolve())
    backend_command = [a.bench, "--target", target_clang]
    commands = []
    report = {"schema": 1, "commands": commands, "correctness_passed": False}

    def save():
        (a.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    def run(argv):
        argv = list(map(str, argv))
        start = time.perf_counter_ns()
        result = subprocess.run(argv, cwd=ROOT, capture_output=True, timeout=300)
        elapsed = time.perf_counter_ns() - start
        commands.append({"argv": argv, "returncode": result.returncode,
                         "wall_ns": elapsed})
        if result.returncode:
            save()
            raise RuntimeError(f"{argv}: {result.stderr.decode(errors='replace')}")
        return result.stdout

    try:
        source = ROOT / "crates/keel-cranelift/kernels/kernels.c"
        driver = source.with_name("driver.c")
        report["environment"] = {
            "head": run(["git", "rev-parse", "HEAD"]).decode().strip(),
            "tree": run(["git", "rev-parse", "HEAD^{tree}"]).decode().strip(),
            "compiler_source_status": run(["git", "status", "--porcelain", "--", "crates", "Cargo.toml", "Cargo.lock"]).decode(),
            "host": platform.uname()._asdict(),
            "cpu": run(cpu_cmd).decode().strip(),
            "hardware": run(hardware_cmd).decode(),
            "os": run(os_cmd).decode(),
            "clang": run([clang, "--version"]).decode(),
            "clang_path": clang,
            "load_start": os.getloadavg(),
            "binaries_sha256": {str(b): sha(b) for b in [a.keel, a.bench, clang]},
            "inputs_sha256": {str(b.relative_to(ROOT)): sha(b) for b in [source, driver]},
            "locks_sha256": {str(b.relative_to(ROOT)): sha(b) for b in [ROOT / "Cargo.lock", ROOT / "crates/keel-cranelift/Cargo.lock"]},
        }
        report["protocol"] = {"warmups": a.warmups, "rounds": a.rounds, "seed": a.seed,
            "c_order": "seeded shuffled configurations per round",
            "ir_order": "existing in-process harness rotates configurations each round; not randomized",
            "c_boundary": "subprocess wall time: raw C read, preprocessing, frontend (including Keel frontend-IR verification), configured optimization, native code generation, integrated assembly, object write; no linking",
            "ir_boundary": "sum of backend stage timers on identical parsed verified Keel IR; excludes parse/verify, IR optimization, process startup, filesystem writes, linking; CLIF verifier off during timing, on for gated objects; FNV64 verifier-on/off fingerprints checked per sample by existing harness",
            "features": "native objects include unwind services; Cranelift unwind_info=false; unequal services",
            "target_keel": target_keel, "target_clang": target_clang,
            "scope": "one maintained kernels translation unit; 21 named kernel checks plus helpers; no application or runtime performance claim",
            "host_control": "no enforced quiet-host gate, core pinning, frequency control or scheduling isolation; load snapshots only"}
        driver_obj = a.out / "driver.o"
        run([clang, "-target", target_clang, "-O2", "-ffp-contract=off", "-c", driver, "-o", driver_obj])
        c_commands = {}
        for name, flags in [("keel-raw", []), ("keel-O", ["-O"])]:
            c_commands[name] = [a.keel, "cc", *flags, "--target", target_keel, "--emit-object", source, "-o", a.out / f"{name}.o"]
        for name, flag in [("clang-O0", "-O0"), ("clang-O2", "-O2")]:
            c_commands[name] = [clang, "-target", target_clang, flag, "-ffp-contract=off", "-c", source, "-o", a.out / f"{name}.o"]
        objects = {}
        for name, cmd in c_commands.items():
            run(cmd)
            objects[name] = a.out / f"{name}.o"
        irs = {}
        report["support"] = {}
        for variant, flags in [("raw", []), ("O", ["-O"])]:
            ir = a.out / f"kernels.{variant}.keel"
            irs[variant] = ir
            run([a.keel, "cc", *flags, "--target", target_keel, "--emit-ir", source, "-o", ir])
            support = [json.loads(line) for line in run([*backend_command, "support", ir]).splitlines()]
            report["support"][variant] = support
            if any(row["error"] is not None for row in support):
                raise RuntimeError(f"unsupported functions in {variant}; no timing")
            for line in run([*backend_command, "objects", "--out-dir", a.out, "--label", variant, ir]).splitlines():
                row = json.loads(line)
                objects[f"{variant}-{row['config']}"] = Path(row["path"])
        outputs = {}
        for name, obj in objects.items():
            exe = a.out / f"check-{name}"
            run([clang, "-target", target_clang, driver_obj, obj, "-o", exe])
            outputs[name] = run([exe, "check"])
            (a.out / f"check-{name}.txt").write_bytes(outputs[name])
        oracle = outputs["clang-O0"]
        report["correctness"] = {name: {"agrees_with_clang_O0": value == oracle,
            "lines": value.count(b"\n"), "sha256": hashlib.sha256(value).hexdigest()}
            for name, value in outputs.items()}
        if oracle.count(b"\n") != 795:
            raise RuntimeError("maintained driver must produce 795 lines; no timing")
        if any(value != oracle for value in outputs.values()):
            raise RuntimeError("correctness mismatch; no timing")
        report["correctness_passed"] = True
        report["ir_sha256"] = {v: sha(ir) for v, ir in irs.items()}
        gated_hashes = {name: sha(obj) for name, obj in objects.items()}
        report["objects_sha256"] = gated_hashes
        if a.correctness_only:
            report["correctness_only"] = True
            report["load_end"] = os.getloadavg()
            save()
            print(a.out / "report.json")
            return
        samples = {name: [] for name in c_commands}
        orders = []
        rng = random.Random(a.seed)
        for round_index in range(a.warmups + a.rounds):
            order = list(c_commands)
            rng.shuffle(order)
            orders.append(order)
            for name in order:
                run(c_commands[name])
                if sha(objects[name]) != gated_hashes[name]:
                    raise RuntimeError(f"timed {name} object differs from gated object")
                if round_index >= a.warmups:
                    samples[name].append(commands[-1]["wall_ns"])
        report["c_to_object"] = {name: {"samples_ns": values,
            "median_ms": statistics.median(values) / 1e6,
            "object_bytes": objects[name].stat().st_size,
            "argv": list(map(str, c_commands[name]))} for name, values in samples.items()}
        report["c_orders_including_warmups"] = orders
        backend = a.out / "ir-time.json"
        run([*backend_command, "time", "--warmup", a.warmups, "--reps", a.rounds,
             "--out", backend, "--emit-dir", a.out,
             *[f"{variant}={ir}" for variant, ir in irs.items()]])
        report["ir_to_object"] = json.loads(backend.read_text())
        for variant in irs:
            for config in CONFIGS:
                name = f"{variant}-{config}"
                if sha(a.out / f"{variant}.{config}.o") != gated_hashes[name]:
                    raise RuntimeError(f"timed {name} object differs from gated object")
        report["objects_sha256"] = gated_hashes
        if a.runtime:
            report["protocol"]["runtime_boundary"] = "monotonic clock inside the driver, fixed input and repetitions per kernel, checksum checked on every run"
            report["kernel_runtime"] = time_kernels(run, a.out, a.rounds, a.warmups, a.seed)
        report["load_end"] = os.getloadavg()
        report["completed"] = True
        save()
        print(a.out / "report.json")
    except Exception as error:
        report["error"] = str(error)
        save()
        raise


if __name__ == "__main__":
    main()
