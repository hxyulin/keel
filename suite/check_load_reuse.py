#!/usr/bin/env python3
"""Run under fast_opt_locked.py; requires a freshly built worktree-local keel.

No timings. Native ARM/x86 oracle + Fast/Full kernel driver equality to Clang;
ARM and x86 static assembly. Every child command shares the outer lock.
"""
import argparse
import json
import pathlib
import platform
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--keel", required=True, type=pathlib.Path)
parser.add_argument("--output", required=True, type=pathlib.Path)
args = parser.parse_args()
root = pathlib.Path(__file__).resolve().parents[1]
keel = str(args.keel.resolve())
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=True)
commands = []


def run(*argv, stdout=None):
    argv = [str(a) for a in argv]
    result = subprocess.run(argv, text=True, capture_output=True)
    commands.append(dict(argv=argv, returncode=result.returncode, stderr=result.stderr))
    (out / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
    if stdout:
        stdout.write_text(result.stdout)
    if result.returncode:
        raise RuntimeError(f"{argv}: {result.stderr}")
    return result.stdout


def body(text, name):
    return text.split(f"func @{name}(", 1)[1].split("\n}", 1)[0]


host = (platform.system(), platform.machine())
if host == ("Darwin", "arm64"):
    native_target = "aarch64-apple-darwin"
elif host == ("Linux", "x86_64"):
    native_target = "x86_64-unknown-linux-gnu"
else:
    raise SystemExit(f"unsupported native oracle host: {host}")
fixture = root / "crates/keel-codegen/tests/fixtures/load_reuse.keel"
driver = root / "crates/keel-codegen/tests/fixtures/load_reuse.c"
kernels = root / "crates/keel-cranelift/kernels"
kernel_driver = kernels / "driver.c"
if host[0] == "Linux":
    # Evidence-only timing-helper adaptation; no kernel C algorithm changes.
    source = kernel_driver.read_text()
    original = "static u64 now(void) { return clock_gettime_nsec_np(CLOCK_UPTIME_RAW); }"
    replacement = "static u64 now(void) { struct timespec ts; clock_gettime(CLOCK_MONOTONIC, &ts); return (u64)ts.tv_sec * 1000000000ULL + (u64)ts.tv_nsec; }"
    assert source.count(original) == 1
    kernel_driver = out / "driver-linux.c"
    kernel_driver.write_text(source.replace(original, replacement))
run("clang", "-O2", "-ffp-contract=off", kernel_driver, kernels / "kernels.c", "-o", out / "clang-check")
reference = run(out / "clang-check", "check", stdout=out / "clang-check.txt")
assert len(reference.splitlines()) == 795
run("clang", "-O0", "-ffp-contract=off", kernel_driver, kernels / "kernels.c", "-o", out / "clang-O0-check")
assert run(out / "clang-O0-check", "check", stdout=out / "clang-O0-check.txt") == reference
for tier in ["fast", "full"]:
    base = out / f"{tier}.keel"
    optimized = out / f"{tier}-reuse.keel"
    run(keel, "cc", "--opt-tier", tier, "--emit-ir", kernels / "kernels.c", "-o", base)
    run(keel, "opt", "-p", ("gvn,load-reuse" if tier == "fast" else "load-reuse"), base, "-o", optimized)
    before, after = body(base.read_text(), "k_isort"), body(optimized.read_text(), "k_isort")
    # Fast has independent ptradd expressions; explicit GVN exposes exact SSA.
    # Full already includes load-reuse; the explicit repeat must retain its
    # two-load result. Fast still needs GVN followed by the opt-in pass.
    before_loads = 2 if tier == "full" else 3
    assert before.count("load.i32") == before_loads
    assert after.count("load.i32") == 2
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]:
        assembly = out / f"{tier}-{target}.s"
        run(keel, "compile", "--isel", "all", "--regalloc", tier, "--target", target,
            "--opt-tier", "none", optimized, "-o", assembly)
        asm_body = assembly.read_text().split("k_isort_begin:", 1)[1].split("k_isort_end:", 1)[0]
        assert asm_body.count(" = load.i32 ") == 2, (tier, target)
        (out / f"isort-{tier}-{target}.s").write_text(asm_body)
        if tier == "full" and target == "aarch64-apple-darwin":
            assert len(re.findall(r"^\s*ldr\s+w[0-9]+,", asm_body, re.MULTILINE)) == 2
        run(keel, "compile", "--isel", "all", "--regalloc", tier, "--target", target,
            "--opt-tier", "none", "--emit-object", optimized, "-o", out / f"{tier}-{target}.o")
    run("clang", "-O2", kernel_driver, out / f"{tier}-{native_target}.o", "-o", out / f"{tier}-check")
    actual = run(out / f"{tier}-check", "check", stdout=out / f"{tier}-check.txt")
    assert actual == reference, tier
    fixture_base, fixture_opt = out / f"oracle-{tier}.keel", out / f"oracle-{tier}-reuse.keel"
    run(keel, "opt", "--opt-tier", tier, fixture, "-o", fixture_base)
    run(keel, "opt", "-p", "load-reuse", fixture_base, "-o", fixture_opt)
    assert body(fixture_opt.read_text(), "reuse").count("load.i64") == 1
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]:
        run(keel, "compile", "--isel", "all", "--regalloc", tier, "--target", target,
            "--opt-tier", "none", "--emit-object", fixture_opt, "-o", out / f"oracle-{tier}-{target}.o")
    for level in ["-O0", "-O2"]:
        executable = out / f"oracle-{tier}-{level}"
        run("clang", level, driver, out / f"oracle-{tier}-{native_target}.o", "-o", executable)
        print(run(executable, stdout=out / f"oracle-{tier}-{level}.txt").strip())
    print(f"{tier}: isort {before_loads} -> 2 loads; 795 kernel output lines equal Clang O2; both targets emitted")
