#!/usr/bin/env python3
"""Run exported packed-bit-field objects against the independent Linux byte oracle."""
import argparse
import hashlib
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
    fixtures = Path(__file__).resolve().parents[2] / "crates/keel-c/tests/fixtures"
    source = fixtures / "packed_bitfields.c"
    oracle = fixtures / "packed_bitfields_oracle.c"
    reference = work / "reference.o"
    command([args.compiler, "-O2", "-c", source, "-o", reference])
    report = {"objects": len(objects), "positive": [], "negative": [], "sha256": {}}
    for name, obj in [("reference", reference), *((p.stem, p) for p in objects)]:
        exe = work / name
        command([args.compiler, "-O2", "-no-pie", oracle, obj, "-o", exe])
        result = command([exe])
        if result.stdout != "PASS\n" or result.stderr:
            raise RuntimeError(f"unexpected transcript for {name}: {result}")
        report["positive"].append(name)
        report["sha256"][name] = hashlib.sha256(obj.read_bytes()).hexdigest()
    original = source.read_text()
    for name, old, new, expected_exit in [
        ("wrong-signed-read", "return ((struct Tiny *)pointer)->value;", "return ((struct Tiny *)pointer)->value + 1;", 3),
        ("wrong-wide-write", "((struct Wide *)pointer)->value = value;", "((struct Wide *)pointer)->value = value + 1;", 4),
        ("clobbered-neighbor", "((struct Wide *)pointer)->value = value;", "((struct Wide *)pointer)->lead = 0; ((struct Wide *)pointer)->value = value;", 4),
        ("wrong-initializer", "0xfedcba9876543210UL", "0xfedcba9876543211UL", 7),
        ("wrong-compound-update", "p->sign += value;", "p->sign += value + 1;", 8),
    ]:
        assert old in original, old
        broken = work / f"{name}.c"
        broken.write_text(original.replace(old, new))
        obj = broken.with_suffix(".o")
        command([args.compiler, "-O2", "-c", broken, "-o", obj])
        exe = broken.with_suffix(".exe")
        command([args.compiler, "-O2", "-no-pie", oracle, obj, "-o", exe])
        result = subprocess.run([str(exe)], capture_output=True, text=True, timeout=10)
        if result.returncode != expected_exit or result.stdout or result.stderr:
            raise RuntimeError(f"broken control did not fail normally: {result}")
        report["negative"].append({"control": name, "exit": result.returncode})
    (work / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{len(objects)} generated objects and reference passed; {len(report['negative'])} broken controls rejected")


if __name__ == "__main__":
    main()
