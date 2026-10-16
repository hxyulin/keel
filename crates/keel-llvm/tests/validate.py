#!/usr/bin/env python3
"""Local correctness validation; requires prebuilt Keel and exporter, no builds/installs.
Run only while holding the coordinator's native validation lease.
Artifacts, commands, hashes and tool versions go under ignored target/.
"""
import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
FIXTURES = pathlib.Path(__file__).resolve().parent / "fixtures"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--keel", type=pathlib.Path, default=ROOT / "target/debug/keel")
parser.add_argument("--exporter", type=pathlib.Path, default=ROOT / "target/debug/keel-llvm")
parser.add_argument("--clang", default="clang")
parser.add_argument("--out", type=pathlib.Path, default=ROOT / "target/keel-llvm-validation")
a = parser.parse_args()
a.out = a.out.resolve()
a.out.mkdir(parents=True, exist_ok=True)
records = []
manifest = {"commands": records, "purpose": "correctness only; no performance measurements"}


def run(*cmd):
    cmd = [str(c) for c in cmd]
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True)
    entry = {"argv": cmd, "returncode": p.returncode,
             "stdout": p.stdout.decode(errors="replace"), "stderr": p.stderr.decode(errors="replace")}
    records.append(entry)
    if p.returncode:
        raise RuntimeError(json.dumps(entry, indent=2))
    return p.stdout


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def export_ir(src, name):
    ll = a.out / (name + ".ll")
    run(a.exporter, "--target", "arm64-apple-macosx11.0.0", src, "-o", ll)
    # -c parses/verifies the module and actually exercises LLVM code generation.
    run(a.clang, "-Wno-override-module", "-x", "ir", "-c", ll, "-o", a.out / (name + ".o"))
    return ll


def execute(source, driver, name, llvm=False):
    exe = a.out / name
    command = [a.clang, "-O2", "-Wno-override-module", source, driver, "-o", exe]
    run(*command)
    result = run(exe)
    (a.out / (name + ".stdout")).write_bytes(result)
    return result


try:
    for binary in [a.keel, a.exporter]:
        if not binary.is_file():
            raise RuntimeError(f"prebuilt binary missing: {binary}; build separately under lease")
    manifest["git_head"] = run("git", "rev-parse", "HEAD").decode().strip()
    manifest["git_tree"] = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    manifest["git_status"] = run("git", "status", "--porcelain").decode()
    manifest["tools"] = {"clang": run(a.clang, "--version").decode(),
                         "rustc": run("rustc", "--version").decode(),
                         "cargo": run("cargo", "--version").decode()}
    manifest["binaries"] = {str(p.resolve()): sha(p) for p in [a.keel, a.exporter]}
    clang_path = shutil.which(a.clang)
    if clang_path:
        manifest["binaries"][clang_path] = sha(clang_path)
    source_paths = list(FIXTURES.glob("*")) + list((ROOT / "crates/keel-llvm/src").glob("*.rs"))
    source_paths += [pathlib.Path(__file__), ROOT / "Cargo.lock", ROOT / "crates/keel-llvm/Cargo.toml"]
    manifest["source_sha256"] = {str(p.relative_to(ROOT)): sha(p) for p in sorted(source_paths)}
    profile_check = a.out / "clang-profile.ll"
    run(a.clang, "-target", "arm64-apple-macosx11.0.0", "-S", "-emit-llvm", "-O0",
        FIXTURES / "hot.c", "-o", profile_check)
    layout = 'target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32"'
    if layout not in profile_check.read_text():
        raise AssertionError("installed Clang data layout differs from exporter profile")
    core = FIXTURES / "core.keel"
    ll = export_ir(core, "core")
    asm = a.out / "core.s"
    run(a.keel, "compile", "--target", "aarch64-apple-darwin", core, "-o", asm)
    expected = execute(asm, FIXTURES / "driver.c", "core-keel")
    actual = execute(ll, FIXTURES / "driver.c", "core-llvm")
    if actual != expected:
        raise AssertionError("core native Keel / LLVM mismatch; inspect retained stdout")
    # Also compile and execute LLVM at O0 to separate export from optimizer bugs.
    core_o0 = a.out / "core-llvm-o0"
    run(a.clang, "-O0", "-Wno-override-module", ll, FIXTURES / "driver.c", "-o", core_o0)
    if run(core_o0) != expected:
        raise AssertionError("core LLVM O0 mismatch")
    hot = FIXTURES / "hot.c"
    baseline = execute(hot, FIXTURES / "hot_driver.c", "hot-clang")
    raw = a.out / "hot.raw.keel"
    optimized = a.out / "hot.opt.keel"
    run(a.keel, "cc", "--emit-ir", "--target", "aarch64-apple-darwin", hot, "-o", raw)
    run(a.keel, "opt", "-p", "inline,simplify-cfg,canonicalize,instcombine,call-contracts,simplify-cfg,gvn,stack-forward,canonicalize,instcombine,dce,gcm", "--target", "aarch64-apple-darwin", raw, "-o", optimized)
    for src, name in [(raw, "hot-raw"), (optimized, "hot-opt")]:
        ll = export_ir(src, name)
        if execute(ll, FIXTURES / "hot_driver.c", name + "-llvm") != baseline:
            raise AssertionError(name + " differs from Clang C baseline")
        asm = a.out / (name + ".s")
        run(a.keel, "compile", "--target", "aarch64-apple-darwin", src, "-o", asm)
        if execute(asm, FIXTURES / "hot_driver.c", name + "-keel") != baseline:
            raise AssertionError(name + " Keel differs from Clang C baseline")
    opt = shutil.which("opt")
    if opt:
        manifest["tools"]["opt"] = run(opt, "--version").decode()
        run(opt, "-passes=verify,default<O2>,verify", "-print-before-all", "-print-after-all", "-S",
            a.out / "hot-raw.ll", "-o", a.out / "hot-raw.llvm-opt.ll")
        # Full pass dump is retained in manifest command stderr.
    else:
        manifest["opt"] = "not installed on PATH; no LLVM pass-by-pass trace obtained"
    manifest["result"] = "PASS"
finally:
    (a.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
