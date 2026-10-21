#!/usr/bin/env python3
"""Static routes and native correctness; invoke under suite/fast_opt_locked.py.

Build Keel in a checkout-exclusive CARGO_TARGET_DIR first. This script never
benchmarks and does not self-lock: its enclosing lock covers every subprocess.
The same invocation is the prepared Linux x86 native frontend oracle.
"""
import argparse
import hashlib
import html
import json
from pathlib import Path
import platform
import struct
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
TARGETS = ("aarch64-apple-darwin", "x86_64-unknown-linux-gnu")


def text_section(path):
    """Read only executable text, excluding object symbol/debug metadata."""
    data = path.read_bytes()
    if data[:4] == b"\x7fELF":
        assert data[4:6] == b"\x02\x01", "expected ELF64 little endian"
        offset = struct.unpack_from("<Q", data, 40)[0]
        size, count, names = struct.unpack_from("<HHH", data, 58)
        string_header = offset + names * size
        string_offset, string_size = struct.unpack_from("<QQ", data, string_header + 24)
        strings = data[string_offset:string_offset + string_size]
        for i in range(count):
            section = offset + i * size
            name = struct.unpack_from("<I", data, section)[0]
            if strings[name:].split(b"\0", 1)[0] == b".text":
                start, length = struct.unpack_from("<QQ", data, section + 24)
                return data[start:start + length]
    else:
        assert data[:4] == b"\xcf\xfa\xed\xfe", "expected Mach-O64 little endian"
        commands = struct.unpack_from("<I", data, 16)[0]
        at = 32
        for _ in range(commands):
            command, size = struct.unpack_from("<II", data, at)
            if command == 0x19:
                sections = struct.unpack_from("<I", data, at + 64)[0]
                for i in range(sections):
                    section = at + 72 + i * 80
                    if data[section:section + 16].split(b"\0", 1)[0] == b"__text":
                        length, start = struct.unpack_from("<QI", data, section + 40)
                        return data[start:start + length]
            at += size
    raise AssertionError(f"no text section: {path}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keel", required=True, type=Path)
    parser.add_argument("--clang", default="clang")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--no-native", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    keel = str(args.keel.resolve())
    commands = []
    report = {"status": "running", "host": platform.platform(), "timings": False,
              "compiler_sha256": hashlib.sha256(Path(keel).read_bytes()).hexdigest(),
              "routes": [], "native": []}

    def run(argv):
        argv = [str(x) for x in argv]
        result = subprocess.run(argv, capture_output=True, text=True, cwd=ROOT)
        commands.append({"argv": argv, "returncode": result.returncode,
                         "stdout": result.stdout, "stderr": result.stderr})
        (output / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        if result.returncode:
            raise RuntimeError(f"command failed: {argv}: {result.stderr}")
        return result.stdout

    native = {("arm64", "Darwin"): TARGETS[0],
              ("aarch64", "Darwin"): TARGETS[0],
              ("x86_64", "Linux"): TARGETS[1]}.get((platform.machine(), platform.system()))
    try:
        for target in TARGETS:
            front = output / f"{target}-frontend.keel"
            run([keel, "cc", "--verify-each", "--opt-tier", "full", "--target", target,
                 "--emit-ir", ROOT / "suite/runtime-kernels/kernels.c", "-o", front])
            source = front.read_text()
            clamp = source.split("func @rk_clamped(", 1)[1].split("\nfunc @", 1)[0]
            assert "vselect" in clamp and clamp.count("swiden_low.i64x2") == 2, clamp
            assert "load.i32x4" not in clamp, "must read exactly two source elements per chunk"
            optimized = output / f"{target}-fixture.keel"
            run([keel, "opt", "--verify-each", "--target", target, "-p", "loop-reduce,dce",
                 HERE / "loops.keel", "-o", optimized])
            for fixture, path in [("frontend", front), ("fixture", optimized)]:
                for selection in ("none", "all"):
                    for allocation in ("fast", "full"):
                        stem = output / f"{target}-{fixture}-{selection}-{allocation}"
                        obj, asm, textual = (Path(str(stem) + ext) for ext in (".o", ".s", "-text.o"))
                        flags = [keel, "compile", "--verify-each", "--target", target,
                                 "--isel", selection, "--regalloc", allocation]
                        run([*flags, "--emit-object", path, "-o", obj])
                        run([*flags, path, "-o", asm])
                        run([keel, "asm", "--target", target, asm, "-o", textual])
                        assert text_section(obj) == text_section(textual), stem
                        report["routes"].append({"target": target, "fixture": fixture,
                            "selection": selection, "allocation": allocation,
                            "text_identical": True, "object": str(obj)})
                        if target == native and fixture == "frontend" and not args.no_native:
                            binary = Path(str(stem) + "-oracle")
                            run([args.clang, "-O2", HERE / "frontend_oracle.c", obj, "-o", binary])
                            result = json.loads(run([binary]))
                            result.update(selection=selection, allocation=allocation)
                            report["native"].append(result)
        if not args.no_native:
            assert report["native"], "native oracle requires macOS ARM or Linux x86"
        report["status"] = "passed"
    except Exception as error:
        report.update(status="failed", error=str(error))
        raise
    finally:
        (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        (output / "report.html").write_text("<!doctype html><meta charset=utf-8>"
            "<title>Packed clamp correctness</title><pre>" +
            html.escape(json.dumps(report, indent=2)) + "</pre>\n")
    print(json.dumps({"status": report["status"], "routes": len(report["routes"]),
                      "native_configurations": len(report["native"]), "evidence": str(output)}))


if __name__ == "__main__":
    main()
