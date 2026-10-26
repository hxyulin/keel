#!/usr/bin/env python3
"""Run exported file-scope-asm objects and ordinary-failure controls on Linux x86-64."""
import argparse
import json
from pathlib import Path
import platform
import subprocess


def command(argv):
    result = subprocess.run([str(a) for a in argv], capture_output=True, text=True, timeout=60)
    if result.returncode:
        raise RuntimeError(f"{argv}: exit {result.returncode}\n{result.stdout}{result.stderr}")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--objects", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--compiler", default="gcc")
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("requires native Linux x86-64")
    objects = sorted(args.objects.resolve().glob("*.o"))
    if not objects:
        parser.error("no exported objects")
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    oracle = Path(__file__).with_name("oracle.c")
    reference = work / "reference.o"
    command([args.compiler, "-O2", "-c", args.source.resolve(), "-o", reference])
    report = {"objects": len(objects), "positive": [], "negative": []}
    for name, obj in [("reference", reference), *((p.stem, p) for p in objects)]:
        exe = work / name
        command([args.compiler, "-O2", "-no-pie", oracle, obj, "-o", exe])
        result = command([exe])
        if result.stdout != "PASS\n" or result.stderr:
            raise RuntimeError(f"unexpected transcript for {name}: {result}")
        report["positive"].append(name)
    original = args.source.read_text()
    for name, old, new in [
        ("wrong-asm-function", "xorq $37", "xorq $38"),
        ("wrong-alias", ".set alias,target", ".set alias,kept_fn"),
        ("wrong-record-pointer", ".quad target,datum,kept_fn", ".quad asm_leaf,datum,kept_fn"),
        ("wrong-relative-target", ".long target-.", ".long datum-."),
        ("wrong-assembly-data", ".byte 3,5,7,11", ".byte 3,5,7,12"),
        ("wrong-kept-object", "kept __attribute__((used)) = 41", "kept __attribute__((used)) = 42"),
    ]:
        assert original.count(old) == 1, old
        source = work / f"{name}.c"
        source.write_text(original.replace(old, new, 1))
        obj = source.with_suffix(".o")
        command([args.compiler, "-O2", "-c", source, "-o", obj])
        exe = source.with_suffix(".exe")
        command([args.compiler, "-O2", "-no-pie", oracle, obj, "-o", exe])
        result = subprocess.run([str(exe)], capture_output=True, text=True, timeout=10)
        if result.returncode != 1 or not result.stdout.startswith("FAIL ") or result.stderr:
            raise RuntimeError(f"broken control did not fail normally: {result}")
        report["negative"].append({"control": name, "stdout": result.stdout.strip()})
    (work / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{len(objects)} generated objects and reference passed; {len(report['negative'])} broken controls rejected")


if __name__ == "__main__":
    main()
