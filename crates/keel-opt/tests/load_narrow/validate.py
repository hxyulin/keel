#!/usr/bin/env python3
"""Explicit correctness-lease runner; never run as an automatic Cargo test.

Requires a built Keel CLI, Clang, and an AArch64 macOS native host. Each command
has retained argv, stdout, stderr, return status, and timestamps. Use a NEW output
directory for every attempt. This runs no benchmark or remote commands.
"""
import argparse
import datetime
import hashlib
import json
import pathlib
import platform
import re
import shutil
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def body(text, name):
    match = re.search(r"func @" + re.escape(name) + r"\([^\n]*\{\n(.*?)^\}",
                      text, re.S | re.M)
    assert match, name
    return match[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--keel", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    source = pathlib.Path(__file__).resolve().parent
    repo = source.parents[3]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    keel = args.keel.resolve()
    clang = pathlib.Path(shutil.which("clang")).resolve()
    records = []
    report = {"status": "RUNNING", "commands": records,
              "keel": str(keel), "keel_sha256": digest(keel),
              "clang": str(clang), "clang_sha256": digest(clang),
              "platform": platform.platform(), "inputs": {},
              "native_x86": "NOT RUN: no LinuxHost lease; objects only"}

    def run(argv):
        argv = [str(arg) for arg in argv]
        n = len(records)
        record = {"argv": argv, "start_utc": datetime.datetime.now(
            datetime.timezone.utc).isoformat()}
        records.append(record)
        try:
            result = subprocess.run(argv, cwd=repo, capture_output=True, timeout=120)
            record["returncode"] = result.returncode
            (out / f"{n:03}.stdout").write_bytes(result.stdout)
            (out / f"{n:03}.stderr").write_bytes(result.stderr)
            record["stdout_sha256"] = hashlib.sha256(result.stdout).hexdigest()
            record["stderr_sha256"] = hashlib.sha256(result.stderr).hexdigest()
            assert result.returncode == 0, (argv, result.returncode,
                                             result.stderr.decode(errors="replace"))
            return result.stdout.decode()
        finally:
            record["end_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()

    try:
        report["source_commit"] = run(["git", "rev-parse", "HEAD"]).strip()
        report["source_tree"] = run(["git", "rev-parse", "HEAD^{tree}"]).strip()
        report["git_status"] = run(["git", "status", "--porcelain"])
        assert not report["git_status"], "commit source before validation"
        report["clang_version"] = run([clang, "--version"])
        assert platform.system() == "Darwin" and platform.machine() == "arm64"
        inputs = [source / name for name in ["cases.keel", "oracle.c", "driver.c", "validate.py"]]
        bridge = repo / "suite/pattern-lab/compiler-cases/recurrences.c"
        inputs.append(bridge)
        for path in inputs:
            report["inputs"][str(path.relative_to(repo))] = digest(path)
            shutil.copyfile(path, out / path.name)
        cases = source / "cases.keel"
        positive = ["stack_o0", "stack_o1", "stack_o8", "stack_o16", "stack_write"]
        fallback = ["stack_mayfault", "stack_debug", "generic_load"]
        pipeline = run([keel, "passes"]).split("default pipeline (-O): ")[1].strip()
        pipeline = ",".join(pipeline.split(", "))
        raw = run([keel, "opt", cases, "--opt-tier", "none"])
        portable = run([keel, "opt", cases, "-p", "load-narrow"])
        assert raw == portable, "no target means unknown endian: must be unchanged"
        (out / "portable.ir").write_text(portable)
        # The oracle is compiled independently at O0, with no LTO or Keel source.
        oracle = out / "oracle.o"
        run([clang, "-O0", "-fno-lto", "-c", source / "oracle.c", "-o", oracle])
        matrix = []
        for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]:
            optimized = run([keel, "opt", cases, "--target", target, "-p", "load-narrow"])
            (out / (target + ".ir")).write_text(optimized)
            for name in positive:
                assert "load.i64" in body(raw, name)
                assert "load.i8" in body(optimized, name), name
                assert "load.i64" not in body(optimized, name), name
                assert "uext.i64" in body(optimized, name), name
            for name in fallback:
                assert body(raw, name) == body(optimized, name), name
            baseline = run([keel, "cc", bridge, "--emit-ir", "--target", target, "-O"])
            narrowed = run([keel, "cc", bridge, "--emit-ir", "--target", target,
                            "-p", pipeline + ",load-narrow"])
            assert baseline == narrowed, "actual C bridge must remain unchanged"
            assert "load.i64" in body(narrowed, "bridge_narrow_load")
            assert "load.i8" not in body(narrowed, "bridge_narrow_load")
            (out / (target + "-bridge.ir")).write_text(narrowed)
            for selector in ["baseline", "full"]:
                for allocator in ["fast", "full"]:
                    name = f"{target}-{selector}-{allocator}"
                    flags = ["--target", target, "-p", "load-narrow",
                             "--isel-tier", selector, "--regalloc", allocator]
                    if selector == "full" and allocator == "full":
                        flags.append("-g")
                    assembly = out / (name + ".s")
                    direct = out / (name + "-direct.o")
                    assembled = out / (name + "-assembled.o")
                    run([keel, "compile", cases, *flags, "-o", assembly])
                    text = assembly.read_text()
                    if target.startswith("aarch64"):
                        assert len(re.findall(r"\bldrb\s", text)) >= len(positive)
                    else:
                        assert len(re.findall(r"\bmovzx\s[^\n]*byte ptr", text)) >= len(positive)
                    run([keel, "compile", cases, *flags, "--emit-object", "-o", direct])
                    run([clang, "-target", target, "-c", assembly, "-o", assembled])
                    for obj in [direct, assembled]:
                        data = obj.read_bytes()
                        if target.startswith("aarch64"):
                            assert data[:4] == b"\xcf\xfa\xed\xfe"
                            assert int.from_bytes(data[4:8], "little") == 0x100000c
                        else:
                            assert data[:4] == b"\x7fELF"
                            assert int.from_bytes(data[18:20], "little") == 62
                    entry = {"target": target, "selection": selector, "allocation": allocator,
                             "debug_lines": "-g" in flags, "narrowed_functions": positive,
                             "fallback_functions_unchanged": fallback, "native": []}
                    matrix.append(entry)
                    if target.startswith("aarch64"):
                        bridge_obj = out / (name + "-bridge.o")
                        run([keel, "cc", bridge, "--target", target, "-p",
                             pipeline + ",load-narrow", "--isel-tier", selector,
                             "--regalloc", allocator, "--emit-object", "-o", bridge_obj])
                        for route, obj in [("direct", direct), ("assembled", assembled)]:
                            native = out / (name + "-" + route)
                            run([clang, "-O0", "-fno-lto", source / "driver.c", obj,
                                 bridge_obj, oracle, "-o", native])
                            result = run([native]).strip()
                            assert re.fullmatch(r"PASS comparisons=\d+ boundary_faults=2", result), result
                            entry["native"].append({"route": route, "path": str(native),
                                                    "sha256": digest(native), "result": result})
        report["matrix"] = matrix
        report["status"] = "PASS"
    except BaseException as error:
        report["status"] = "FAIL"
        report["error"] = repr(error)
        raise
    finally:
        report["keel_after_sha256"] = digest(keel)
        assert report["keel_after_sha256"] == report["keel_sha256"]
        report["artifact_sha256"] = {str(path.relative_to(out)): digest(path)
                                    for path in out.iterdir() if path.is_file()}
        (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "commands": len(records),
                      "matrix": report["matrix"]}, indent=2))


if __name__ == "__main__":
    main()
