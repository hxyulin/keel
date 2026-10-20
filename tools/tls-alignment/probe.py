#!/usr/bin/env python3
"""Retain Darwin TLS alignment counterexamples; exit 1 if any address is misaligned.

Requires an already built Keel CLI and Apple Clang on native arm64 Darwin.
No retry loop, allocator interposition, compilation timing, or expectation relaxation.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source(objects):
    # Ordinary mixed sizes stay live alongside the over-aligned ints. They are
    # checked independently, so no compiler may infer pointer residues in driver.c.
    declarations = []
    names = []
    alignments = []
    values = []
    for index, (alignment, value) in enumerate(objects):
        name = f"tls_{index}"
        declarations.append(f"_Alignas({alignment}) _Thread_local int {name} = {value};")
        names.append(name)
        alignments.append(alignment)
        values.append(value)
    ordinary_char = 3 if any(values) else 0
    ordinary_long = 7 if any(values) else 0
    declarations += [
        f"_Thread_local char ordinary_char = {ordinary_char};",
        "_Thread_local short ordinary_short;",
        f"_Thread_local long ordinary_long = {ordinary_long};",
    ]
    # Exercise mixed-size storage without treating char/short as int pointers.
    ordinary_check = f"ordinary_char == {ordinary_char} && ordinary_short == 0 && ordinary_long == {ordinary_long}"
    declarations += [f"int tls_count(void) {{ return ({ordinary_check}) ? {len(names)} : -1; }}"]
    for function, ty, expressions in [
        ("tls_addr", "void *", [f"&{n}" for n in names]),
        ("tls_alignment", "int", list(map(str, alignments))),
        ("tls_initial", "int", list(map(str, values))),
    ]:
        cases = " ".join(f"case {i}: return {expr};" for i, expr in enumerate(expressions))
        declarations.append(f"{ty} {function}(int i) {{ switch(i) {{ {cases} }} return 0; }}")
    return "\n".join(declarations) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keel", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        parser.error("this probe requires native arm64 Darwin")
    out = args.out.resolve()
    if out.exists():
        parser.error("--out must be a fresh directory; retain earlier failures")
    out.mkdir(parents=True)
    keel = args.keel.resolve()
    driver = Path(__file__).with_name("driver.c")
    shutil.copyfile(driver, out / "driver.c")
    commands = []

    def run(argv, label, check=True):
        command = list(map(str, argv))
        result = subprocess.run(command, capture_output=True, timeout=60)
        (out / f"{label}.stdout").write_bytes(result.stdout)
        (out / f"{label}.stderr").write_bytes(result.stderr)
        commands.append({"argv": command, "cwd": os.getcwd(), "returncode": result.returncode,
                         "stdout": f"{label}.stdout", "stderr": f"{label}.stderr"})
        (out / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        if check and result.returncode:
            raise RuntimeError(f"{label}: exit {result.returncode}; see retained stderr")
        return result

    run(["clang", "--version"], "clang-version")
    run(["sw_vers"], "os-version")
    run(["uname", "-a"], "uname")
    identity = {"keel": str(keel), "keel_sha256": sha(keel),
                "driver_sha256": sha(driver), "script_sha256": sha(__file__)}
    (out / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    fixtures = {
        "initialized32": [(32, 17)], "zero32": [(32, 0)],
        "initialized64": [(64, 19)], "zero64": [(64, 0)],
        "mixed": [(32, 17), (64, 0), (128, 23), (32, 0), (64, 19), (128, 0)],
        "mixed-reverse": [(128, 0), (64, 19), (32, 0), (128, 23), (64, 0), (32, 17)],
        "ordinary16": [(4, 17), (8, 0), (16, 19)],
    }
    rows = []
    for fixture, objects in fixtures.items():
        src = out / f"{fixture}.c"
        src.write_text(source(objects))
        for optimize in [False, True]:
            mode = "gO" if optimize else "g"
            base = f"{fixture}-{mode}"
            flags = ["-g"] + (["-O"] if optimize else [])
            ir = out / f"{base}.keel"
            run([keel, "cc", *flags, "--emit-ir", src, "-o", ir], f"{base}-ir")
            for route in ["clang", "keel-text", "keel-structured"]:
                label = f"{base}-{route}"
                asm = out / f"{label}.s"
                obj = out / f"{label}.o"
                exe = out / label
                if route == "clang":
                    run(["clang", "-g", "-O2" if optimize else "-O0", "-S", src, "-o", asm], label + "-asm")
                    run(["clang", "-c", asm, "-o", obj], label + "-obj")
                elif route == "keel-text":
                    run([keel, "cc", *flags, src, "-o", asm], label + "-asm")
                    run(["clang", "-c", asm, "-o", obj], label + "-obj")
                else:
                    run([keel, "cc", *flags, "--emit-object", src, "-o", obj], label + "-obj")
                run(["otool", "-lrv", obj], label + "-object-layout")
                run(["nm", "-nm", obj], label + "-object-symbols", check=False)
                # The driver is always separate and unoptimized: it observes the
                # actual returned pointer, initial value, mutation and isolation.
                linked = run(["clang", "-g", "-O0", obj, out / "driver.c", "-o", exe], label + "-link", check=False)
                if linked.returncode:
                    rows.append({"fixture": fixture, "mode": mode, "route": route,
                                 "kind": "link-failure", "returncode": linked.returncode,
                                 "object_sha256": sha(obj), "stderr": linked.stderr.decode()})
                    (out / "results.json").write_text(json.dumps(rows, indent=2) + "\n")
                    continue
                run(["otool", "-l", exe], label + "-executable-layout")
                run(["nm", "-nm", exe], label + "-executable-symbols")
                result = run([exe], label + "-run", check=False)
                if result.returncode not in (0, 1):
                    raise RuntimeError(f"{label}: unexpected exit {result.returncode}")
                rows.append({"fixture": fixture, "mode": mode, "route": route,
                             "kind": "native", "returncode": result.returncode,
                             "stdout": result.stdout.decode(), "stderr": result.stderr.decode(),
                             "source_sha256": sha(src), "ir_sha256": sha(ir),
                             "object_sha256": sha(obj), "executable_sha256": sha(exe)})
                (out / "results.json").write_text(json.dumps(rows, indent=2) + "\n")
    failures = sum(row["kind"] == "native" and row["returncode"] != 0 for row in rows)
    link_failures = sum(row["kind"] == "link-failure" for row in rows)
    print(f"{len(rows)} configurations; {failures} alignment counterexamples; "
          f"{link_failures} link failures; retained in {out}")
    return bool(failures or link_failures)


if __name__ == "__main__":
    raise SystemExit(main())
