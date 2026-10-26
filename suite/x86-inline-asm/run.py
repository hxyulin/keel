#!/usr/bin/env python3
"""Execute exported Keel scalar inline-asm objects against GCC/Clang references."""
import argparse
import json
from pathlib import Path
import platform
import subprocess


def command(argv, timeout=60):
    result = subprocess.run([str(a) for a in argv], capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"{argv}: exit {result.returncode}\n{result.stdout}{result.stderr}")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--objects", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--compiler", action="append", required=True)
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("requires native Linux x86-64")
    objects = sorted(args.objects.resolve().glob("*.o"))
    if not objects:
        parser.error("no exported objects")
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    oracle = Path(__file__).with_name("oracle.c")
    report = {"objects": len(objects), "positive": [], "negative": []}
    for index, compiler in enumerate(args.compiler):
        reference = work / f"reference-{index}.o"
        command([compiler, "-O2", "-DNAME(x)=ref_##x", "-c", args.source.resolve(), "-o", reference])
        for obj in objects:
            exe = work / f"{index}-{obj.stem}"
            command([compiler, "-O2", "-no-pie", oracle, reference, obj, "-o", exe])
            result = command([exe])
            if result.stdout != "PASS\n" or result.stderr:
                raise RuntimeError(f"unexpected transcript for {obj}: {result.stdout!r} {result.stderr!r}")
            report["positive"].append({"compiler": compiler, "object": obj.name})
        original = args.source.read_text()
        for name, broken in [
            ("wrong-add", original.replace("return a;", "return a ^ 1;", 1)),
            ("wrong-clobber-result", original.replace("return out ^ (a +", "return 1 ^ out ^ (a +", 1)),
            ("wrong-deferred-symbol", original.replace("deferred_outer(asm_deferred_anchor + 7)", "deferred_outer(asm_deferred_anchor + 8)", 1)),
            ("wrong-deferred-number", original.replace("deferred_add(x, 17 + 6)", "deferred_add(x, 17 + 7)", 1)),
            ("missing-deferred-symbol", original.replace("leaq %c1(%%rip),%0", "movq $7,%0", 1)),
            ("missing-memory-write", original.replace('asm("addq %1,%0" : "+m"(*p)', 'asm("" : "+m"(*p)', 1)),
            ("wrong-memory-address", original.replace('"+m"(*p++)', '"+m"(*++p)', 1)),
            ("wrong-memory-read", original.replace('asm("movq %1,%0" : "=r"(value) : "m"(*p))', 'asm("movq $0,%0" : "=r"(value) : "m"(*p))', 1)),
            ("wrong-q-output", original.replace('return out + 3;', 'return out + 4;', 1)),
            ("wrong-q-byte-result", original.replace('movb %b1,%0; addb %b2,%0', 'movb %b1,%0; subb %b2,%0', 1)),
            ("wrong-address-symbol", original.replace('"i"(asm_deferred_anchor + 13)', '"i"(asm_deferred_anchor + 14)', 1)),
            ("wrong-negated-offset", original.replace('[delta] "n"(37)', '[delta] "n"(38)', 1)),
            ("wrong-choice-immediate", original.replace('choice_add(a, 17 + 6)', 'choice_add(a, 17 + 7)', 1)),
            ("wrong-flag-condition", original.replace('"=@cce"(f4)', '"=@ccne"(f4)', 1)),
            ("missing-flag-memory-write", original.replace('subq %[rhs],%[value]; movq %[value],%[mem]', 'subq %[rhs],%[value]', 1)),
        ]:
            assert broken != original
            source = work / f"{index}-{name}.c"
            source.write_text(broken)
            obj = source.with_suffix(".o")
            command([compiler, "-O2", "-c", source, "-o", obj])
            exe = source.with_suffix(".exe")
            command([compiler, "-O2", "-no-pie", oracle, reference, obj, "-o", exe])
            result = subprocess.run([str(exe)], capture_output=True, text=True, timeout=10)
            if result.returncode != 1 or not result.stdout.startswith("FAIL ") or result.stderr:
                raise RuntimeError(f"broken control did not fail normally: {result}")
            report["negative"].append({"compiler": compiler, "control": name, "stdout": result.stdout.strip()})
    (work / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{len(report['positive'])} native positives; {len(report['negative'])} broken controls rejected")


if __name__ == "__main__":
    main()
