#!/usr/bin/env python3
"""Native ELF application correctness using read-only, already prepared inputs.

All outputs and runtime fixtures go under --work-dir. Never run tests in the
parent's fixture tree: Lua/QuickJS tests create temporary files.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import time


def run(argv, *, cwd, log, stdin=None, timeout=300):
    start = time.monotonic()
    result = subprocess.run([str(a) for a in argv], cwd=cwd, input=stdin,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            timeout=timeout)
    log.write_bytes(result.stdout)
    if result.returncode:
        raise RuntimeError(f"{argv}: exit {result.returncode}; see {log}")
    return {"command": [str(a) for a in argv], "seconds": time.monotonic() - start,
            "log": str(log)}, result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--prepared", type=Path, required=True,
                        help="prepared target/suite directory, read only")
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--configs", default="baseline,opt,debug,opt-select")
    args = parser.parse_args()
    compiler = args.compiler.resolve()
    prepared = args.prepared.resolve()
    work = args.work_dir.resolve()
    if work == prepared or work.is_relative_to(prepared):
        parser.error("work directory must be outside the prepared tree")
    work.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[1]
    configs = {"baseline": [], "opt": ["-O"], "debug": ["-g"],
               "opt-select": ["-O", "--isel", "all"]}
    report = {"compiler": str(compiler), "prepared": str(prepared), "runs": []}
    for suite, binary, libs, source_dir in [
        ("lua", "lua", ["-lm", "-ldl"], "lua-5.4.7-tests"),
        ("quickjs", "qjs", ["-lm", "-lpthread", "-ldl"], "quickjs-2024-01-13"),
        ("sqlite", "sqlite3", ["-lm", "-lpthread", "-ldl"], None),
    ]:
        inputs = sorted((prepared / suite / "pre").glob("*.i"))
        if not inputs:
            raise RuntimeError(f"no prepared inputs for {suite}")
        fixtures = work / suite / "fixtures"
        if source_dir:
            shutil.copytree(prepared / suite / source_dir, fixtures, dirs_exist_ok=True)
        else:
            fixtures.mkdir(parents=True, exist_ok=True)
        for config in args.configs.split(","):
            flags = configs[config]
            out = work / suite / config
            out.mkdir(parents=True, exist_ok=True)
            entry = {"suite": suite, "config": config, "compilation": [], "tests": []}
            objects = []
            for path in inputs:
                obj = out / path.with_suffix(".o").name
                record, _ = run([compiler, "cc", *flags, "--emit-object", path, "-o", obj],
                                cwd=out, log=out / f"{path.stem}.compile.log")
                entry["compilation"].append(record)
                objects.append(obj)
            exe = out / binary
            record, _ = run(["clang", *objects, *libs, "-o", exe], cwd=out,
                            log=out / "link.log")
            entry["link"] = record
            if suite == "lua":
                commands = [[exe, "-e", "_U=true", "all.lua"]]
            elif suite == "quickjs":
                commands = [[exe, f"tests/test_{name}.js"] for name in
                            ["closure", "language", "builtin", "loop", "std", "worker"]]
                commands += [[exe, "--bignum", f"tests/test_{name}.js"] for name in
                             ["op_overloading", "bignum"]]
                commands += [[exe, "--qjscalc", "tests/test_qjscalc.js"]]
            else:
                commands = [[exe, ":memory:"]]
            stdin = (root / "suite/sqlite/queries.sql").read_bytes() if suite == "sqlite" else None
            for i, command in enumerate(commands):
                record, output = run(command, cwd=fixtures, log=out / f"test-{i}.log", stdin=stdin)
                entry["tests"].append(record)
                if suite == "sqlite":
                    reference = out / "reference"
                    run(["clang", "-w", "-O2", *inputs, *libs, "-o", reference],
                        cwd=out, log=out / "reference-build.log")
                    _, expected = run([reference, ":memory:"], cwd=fixtures,
                                      log=out / "reference-test.log", stdin=stdin)
                    if output != expected:
                        raise RuntimeError(f"SQLite differs from Clang: {out}")
            report["runs"].append(entry)
            (work / "report.json").write_text(json.dumps(report, indent=2) + "\n")
            print(f"PASS {suite} {config}: {len(inputs)} objects, {len(commands)} tests", flush=True)


if __name__ == "__main__":
    main()
