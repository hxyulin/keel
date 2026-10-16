#!/usr/bin/env python3
"""Local correctness validation; requires prebuilt Keel and exporter, no builds/installs.
Run only while holding the coordinator's native validation lease.
Artifacts, commands, hashes and tool versions go under ignored target/.
"""
import argparse
import hashlib
import json
import pathlib
import re
import resource
import shutil
import subprocess
import sys

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
if (a.out / "manifest.json").exists():
    parser.error("output already contains a manifest; choose a fresh --out to preserve prior evidence")
records = []
manifest = {"commands": records, "purpose": "correctness only; no performance measurements"}


def run(*cmd, expected_returncodes=(0,)):
    cmd = [str(c) for c in cmd]
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True)
    entry = {"argv": cmd, "returncode": p.returncode,
             "stdout": p.stdout.decode(errors="replace"), "stderr": p.stderr.decode(errors="replace")}
    records.append(entry)
    if p.returncode not in expected_returncodes:
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
    # /usr/bin/clang on macOS is a small launcher, not the actual compiler.
    # Pin both the invoked launcher and the compiler identified by --version.
    installed = re.search(r"^InstalledDir: (.+)$", manifest["tools"]["clang"], re.MULTILINE)
    if installed:
        actual_clang = pathlib.Path(installed[1]) / pathlib.Path(a.clang).name
        if actual_clang.is_file():
            manifest["binaries"][str(actual_clang.resolve())] = sha(actual_clang)
    manifest["tools"]["python"] = sys.version
    manifest["binaries"][str(pathlib.Path(sys.executable).resolve())] = sha(sys.executable)
    for tool in ["rustc", "cargo"]:
        launcher = shutil.which(tool)
        if launcher:
            manifest["binaries"][launcher] = sha(launcher)
        if shutil.which("rustup"):
            actual = pathlib.Path(run("rustup", "which", tool).decode().strip())
            manifest["binaries"][str(actual.resolve())] = sha(actual)
    source_paths = [p for p in (ROOT / "crates/keel-llvm").rglob("*")
                    if p.is_file() and p.suffix in {".rs", ".py", ".c", ".keel", ".toml", ".md"}]
    source_paths += [ROOT / "Cargo.lock"]
    manifest["source_sha256"] = {str(p.relative_to(ROOT)): sha(p) for p in sorted(source_paths)}
    profile_check = a.out / "clang-profile.ll"
    run(a.clang, "-target", "arm64-apple-macosx11.0.0", "-S", "-emit-llvm", "-O0",
        FIXTURES / "hot.c", "-o", profile_check)
    layout = 'target datalayout = "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32"'
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
    extended = FIXTURES / "extended.keel"
    extended_ll = export_ir(extended, "extended")
    extended_asm = a.out / "extended.s"
    run(a.keel, "compile", "--target", "aarch64-apple-darwin", extended, "-o", extended_asm)
    extended_expected = execute(extended_asm, FIXTURES / "extended_driver.c", "extended-keel")
    for level in ["0", "2"]:
        exe = a.out / ("extended-llvm-O" + level)
        run(a.clang, "-O" + level, "-Wno-override-module", extended_ll, FIXTURES / "extended_driver.c", "-o", exe)
        result = run(exe)
        (a.out / ("extended-llvm-O" + level + ".stdout")).write_bytes(result)
        if result != extended_expected:
            raise AssertionError("extended LLVM O" + level + " differs from Keel")
    wide_ll = export_ir(FIXTURES / "wide.keel", "wide")
    for level in ["0", "2"]:
        exe = a.out / ("wide-llvm-O" + level)
        run(a.clang, "-O" + level, "-Wno-override-module", wide_ll, FIXTURES / "wide_driver.c", "-o", exe)
        run(exe)
    manifest["wide_multiply_overflow_oracle"] = "Clang C __builtin_mul_overflow: Keel native backend rejects i128 multiply-overflow"
    # R1-1: custom ordinary strlen with visible side effects, in a separate
    # object. No LTO, C builtin folding or body-based inference may hide it.
    libc_ll = export_ir(FIXTURES / "libcall.keel", "libcall")
    provider = a.out / "libcall-provider.o"
    run(a.clang, "-O2", "-fno-builtin", "-c", FIXTURES / "libcall_provider.c", "-o", provider)
    libc_asm = a.out / "libcall.s"
    run(a.keel, "compile", "--target", "aarch64-apple-darwin", FIXTURES / "libcall.keel", "-o", libc_asm)
    libc_outputs = {}
    for source, kind in [(libc_asm, "keel"), (libc_ll, "llvm")]:
        for level in ["0", "2"]:
            exe = a.out / ("libcall-" + kind + "-O" + level)
            run(a.clang, "-O" + level, "-Wno-override-module", source,
                provider, FIXTURES / "libcall_driver.c", "-o", exe)
            libc_outputs[kind + "-O" + level] = run(exe).decode()
    if len(set(libc_outputs.values())) != 1:
        raise AssertionError("custom ordinary strlen side-effect mismatch")
    manifest["ordinary_libcall_outputs"] = libc_outputs
    optimized_libcall = a.out / "libcall.llvm-O2.ll"
    run(a.clang, "-O2", "-Wno-override-module", "-S", "-emit-llvm", "-x", "ir", libc_ll, "-o", optimized_libcall)
    optimized_text = optimized_libcall.read_text()
    # Clang prints decoded names and usually puts attributes into numbered groups.
    if len(re.findall(r"(?:tail )?call i64 @strlen\(", optimized_text)) != 2:
        raise AssertionError("ordinary strlen calls disappeared from optimized LLVM IR")
    declaration = next(line for line in optimized_text.splitlines() if line.startswith("declare i64 @strlen("))
    group = re.search(r"#(\d+)", declaration)
    declaration_attrs = declaration + (next(line for line in optimized_text.splitlines()
        if line.startswith("attributes #" + group[1] + " =")) if group else "")
    if "nobuiltin" not in declaration_attrs or re.search(r"memory\(|readonly|willreturn|nounwind", declaration_attrs):
        raise AssertionError("unexpected libc name-based declaration attributes: " + declaration_attrs)
    manifest["ordinary_libcall_optimized_declaration"] = declaration_attrs
    # Negative control recreates the pre-fix exporter output. On the installed
    # compiler it must demonstrate the forbidden name-based transformation.
    unprotected = a.out / "libcall-unprotected.ll"
    unprotected.write_text(libc_ll.read_text().replace(" nobuiltin", ""))
    control = a.out / "libcall-unprotected-O2"
    run(a.clang, "-O2", "-Wno-override-module", unprotected, provider, FIXTURES / "libcall_driver.c", "-o", control)
    run(control, expected_returncodes=(1, 2))
    manifest["ordinary_libcall_negative_control"] = "pre-fix LLVM O2 failed driver as expected (removed side effect or constant result changed)"
    control_ir = a.out / "libcall-unprotected-O2.ll"
    run(a.clang, "-O2", "-Wno-override-module", "-S", "-emit-llvm", unprotected, "-o", control_ir)
    manifest["ordinary_libcall_negative_control_optimized_ir"] = control_ir.read_text()
    trap_ll = export_ir(FIXTURES / "trap.keel", "trap")
    trap_asm = a.out / "trap.s"
    run(a.keel, "compile", "--target", "aarch64-apple-darwin", FIXTURES / "trap.keel", "-o", trap_asm)
    # Do not create crash/core artifacts. Restriction applies to this process
    # and inherited subprocesses, not the shell/user environment.
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    trap_results = {}
    for source, kind in [(trap_asm, "keel"), (trap_ll, "llvm")]:
        for level in ["0", "2"]:
            exe = a.out / ("trap-" + kind + "-O" + level)
            run(a.clang, "-O" + level, "-Wno-override-module", source, "-o", exe)
            run(exe, expected_returncodes=tuple(range(-127, 0)))
            trap_results[kind + "-O" + level] = records[-1]["returncode"]
    manifest["trap_abnormal_termination"] = trap_results

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
    abi_source = FIXTURES / "abi.c"
    abi_driver = FIXTURES / "abi_driver.c"
    abi_expected = execute(abi_source, abi_driver, "abi-clang")
    abi_ir = a.out / "abi.keel"
    run(a.keel, "cc", "--emit-ir", "--target", "aarch64-apple-darwin", abi_source, "-o", abi_ir)
    abi_ll = export_ir(abi_ir, "abi")
    abi_asm = a.out / "abi.s"
    run(a.keel, "compile", "--target", "aarch64-apple-darwin", abi_ir, "-o", abi_asm)
    if execute(abi_asm, abi_driver, "abi-keel") != abi_expected:
        raise AssertionError("narrow / stack argument Keel ABI differs from Clang")
    for level in ["0", "2"]:
        exe = a.out / ("abi-llvm-O" + level)
        run(a.clang, "-O" + level, "-Wno-override-module", abi_ll, abi_driver, "-o", exe)
        if run(exe) != abi_expected:
            raise AssertionError("narrow / stack argument LLVM ABI differs from Clang at O" + level)
    opt = shutil.which("opt")
    if opt:
        manifest["tools"]["opt"] = run(opt, "--version").decode()
        run(opt, "-passes=verify,default<O2>,verify", "-print-before-all", "-print-after-all", "-S",
            a.out / "hot-raw.ll", "-o", a.out / "hot-raw.llvm-opt.ll")
        # Full pass dump is retained in manifest command stderr.
    else:
        manifest["opt"] = "not installed on PATH; no LLVM pass-by-pass trace obtained"
    manifest["result"] = "PASS"
except BaseException as exc:
    manifest["result"] = "FAIL"
    manifest["failure"] = repr(exc)
    raise
finally:
    manifest["artifact_sha256"] = {str(p.relative_to(a.out)): sha(p)
        for p in sorted(a.out.iterdir()) if p.is_file() and p.name != "manifest.json"}
    test_paths = {p for pattern in ["export-*", "keel_llvm-*"]
                  for p in (ROOT / "target/debug/deps").glob(pattern)}
    manifest["test_binary_sha256"] = {str(p.relative_to(ROOT)): sha(p)
        for p in sorted(test_paths) if p.is_file() and not p.suffix}
    (a.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
