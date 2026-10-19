#!/usr/bin/env python3
"""Bounded native application baseline; reuse keel-suite's prepared inputs.

Run from a frozen source copy after keel-suite prepares Lua/QuickJS/SQLite.
All builds and correctness gates precede timing. No remote/config mutations.
Compilation measures serial preprocessed C -> assembly, excluding preprocessing,
assembly and linking. Runtime measures CLI wall time, including process startup.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import time

APPS = {"lua": ("lua", ["fib", "sort", "string", "float"]),
        "quickjs": ("qjs", ["fib", "sort", "string"]),
        "sqlite": ("sqlite3", ["queries"])}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--work", type=Path, required=True)
    p.add_argument("--rounds", type=int, default=8)
    p.add_argument("--compile-rounds", type=int, default=3)
    p.add_argument("--gcc", help="optional verified real GCC, never Apple's alias")
    args = p.parse_args()
    root, work = args.root.resolve(), args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    log = (work / "commands.jsonl").open("w", buffering=1)
    report = {"host": platform.platform(), "hostname": platform.node(),
              "script_sha256": sha(__file__), "runtime_rounds": args.rounds,
              "runtime_warmups": 2, "compile_rounds": args.compile_rounds,
              "compile_warmups": 1, "apps": {}, "checks": [],
              "method": __doc__, "errors": [], "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}

    def save():
        (work / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    def run(command, stdin=None, cwd=None, timeout=180, label=""):
        command = list(map(str, command))
        started = time.perf_counter_ns()
        result = subprocess.run(command, input=stdin, cwd=cwd, capture_output=True, timeout=timeout)
        elapsed = (time.perf_counter_ns() - started) / 1e6
        log.write(json.dumps({"command": command, "cwd": str(cwd) if cwd else None,
                              "label": label, "wall_ms": elapsed,
                              "stdin_sha256": hashlib.sha256(stdin).hexdigest() if stdin is not None else None,
                              "returncode": result.returncode,
                              "stdout": result.stdout.decode(errors="replace"),
                              "stderr": result.stderr.decode(errors="replace")}) + "\n")
        if result.returncode:
            raise RuntimeError("{}: {}\n{}".format(command, result.returncode, result.stderr.decode(errors="replace")))
        return result.stdout, elapsed

    configs = {"keel": [str(root / "target/release/keel"), "cc"],
               "keel-O": [str(root / "target/release/keel"), "cc", "-O"],
               "clang-O0": ["/usr/bin/clang", "-w", "-S", "-O0", "-ffp-contract=off"],
               "clang-O2": ["/usr/bin/clang", "-w", "-S", "-O2", "-ffp-contract=off"]}
    if args.gcc:
        version = run([args.gcc, "--version"])[0].decode()
        if "clang" in version.lower() or "gcc" not in version.lower():
            raise ValueError("--gcc must identify an independent GNU GCC")
        report["gcc_version"] = version
        for level in (0, 2):
            configs["gcc-O" + str(level)] = [args.gcc, "-w", "-S", "-O" + str(level), "-ffp-contract=off"]
    report["configs"] = configs
    report["compiler_sha256"] = {str(Path(c[0]).resolve()): sha(Path(c[0]).resolve()) for c in configs.values()}
    report["clang_version"] = run(["/usr/bin/clang", "--version"])[0].decode()
    report["cpu"] = run(["sysctl", "-n", "machdep.cpu.brand_string"])[0].decode().strip()
    report["processes_before"] = run(["ps", "-Ao", "pid,pcpu,comm", "-r"])[0].decode()

    def order(round_index):
        names = list(configs)
        # Alternate complete forward/reverse orders; rotate each pair's start.
        shift = (round_index // 2) % len(names)
        names = names[shift:] + names[:shift]
        return names if round_index % 2 == 0 else list(reversed(names))

    def compile_one(app, name, source, tag):
        assembly = work / app / name / (source.stem + ".s")
        return run([*configs[name], source, "-o", assembly], label=tag)[1]

    def invocation(app, binary, bench):
        fixture = root / "suite" / app / ("bench.sql" if app == "sqlite" else "bench.lua" if app == "lua" else "bench.js")
        if app == "sqlite":
            return [binary, ":memory:"], fixture.read_bytes()
        return [binary, *(["--stack-size", "4194304"] if app == "quickjs" else []), fixture, bench], None

    try:
        # Freeze every build before any repeated timing.
        for app, (binary_name, benches) in APPS.items():
            sources = sorted((root / "target/suite" / app / "pre").glob("*.i"))
            assert sources, app
            detail = {"sources_sha256": {str(s): sha(s) for s in sources},
                      "input_bytes": sum(s.stat().st_size for s in sources),
                      "source_count": len(sources), "builds": {}, "compile": {}, "runtime": {}}
            report["apps"][app] = detail
            for name in configs:
                directory = work / app / name
                directory.mkdir(parents=True)
                objects, compile_ms, assembly_ms = [], [], []
                for source in sources:
                    compile_ms.append(compile_one(app, name, source, "build-to-assembly"))
                    obj = directory / (source.stem + ".o")
                    objects.append(obj)
                    assembly_ms.append(run(["/usr/bin/clang", "-c", directory / (source.stem + ".s"), "-o", obj], label="assemble")[1])
                binary = directory / binary_name
                _, link_ms = run(["/usr/bin/clang", *objects, "-o", binary, "-lm", "-lpthread"], label="link")
                detail["builds"][name] = {"binary": str(binary), "sha256": sha(binary), "compile_ms": compile_ms,
                                           "assembly_ms": assembly_ms, "link_ms": link_ms}
                print("built", app, name, flush=True)
                save()

        # Suite tests (not only matching benchmark checksums).
        for app, detail in report["apps"].items():
            expected = None
            for name in ["clang-O2", *[n for n in configs if n != "clang-O2"]]:
                directory = work / app / name
                binary = detail["builds"][name]["binary"]
                if app == "lua":
                    tests = directory / "tests"
                    shutil.copytree(root / "target/suite/lua/lua-5.4.7-tests", tests)
                    run([binary, "-e", "_U=true", "all.lua"], cwd=tests, label="correctness-lua", timeout=240)
                    tests_passed = ["Lua portable suite"]
                elif app == "quickjs":
                    tests = directory / "tests"
                    shutil.copytree(root / "target/suite/quickjs/quickjs-2024-01-13/tests", tests)
                    tests_passed = []
                    for test, extra in [("closure", []), ("language", []), ("builtin", []), ("loop", []), ("std", []), ("worker", []), ("op_overloading", ["--bignum"]), ("bignum", ["--bignum"]), ("qjscalc", ["--qjscalc"])]:
                        run([binary, *extra, tests / ("test_" + test + ".js")], cwd=directory, label="correctness-quickjs")
                        tests_passed.append(test)
                else:
                    answer = run([binary, ":memory:"], stdin=(root / "suite/sqlite/queries.sql").read_bytes(), label="correctness-sqlite")[0]
                    if expected is None:
                        expected = answer
                    assert answer == expected, (app, name, "query output mismatch")
                    tests_passed = ["queries.sql matches Clang O2 byte-for-byte"]
                report["checks"].append({"app": app, "config": name, "passed": tests_passed})
                print("checked", app, name, flush=True)
                save()
            # Validate every benchmark output before the timing campaign.
            for bench in APPS[app][1]:
                command, stdin = invocation(app, detail["builds"]["clang-O2"]["binary"], bench)
                expected = run(command, stdin=stdin, label="benchmark-output-reference")[0]
                outputs = {}
                for name in configs:
                    command, stdin = invocation(app, detail["builds"][name]["binary"], bench)
                    output = run(command, stdin=stdin, label="benchmark-output-check")[0]
                    assert output == expected, (app, bench, name, "benchmark output mismatch")
                    outputs[name] = output.decode()
                detail["runtime"][bench] = {"outputs": outputs, "samples_ms": {name: [] for name in configs}, "warmups_ms": {name: [] for name in configs}}
            save()
        report["correctness_complete"] = True
        save()

        for app, detail in report["apps"].items():
            sources = list(map(Path, detail["sources_sha256"]))
            detail["compile"] = {name: {"samples_ms": [], "warmups_ms": []} for name in configs}
            for r in range(-1, args.compile_rounds):
                for name in order(max(r, 0)):
                    times = [compile_one(app, name, s, "compile-warmup" if r < 0 else "compile-sample") for s in sources]
                    detail["compile"][name]["warmups_ms" if r < 0 else "samples_ms"].append(sum(times))
                    print("compile", app, r, name, round(sum(times), 3), flush=True)
                    save()

        # Copy each executable to one pathname outside the timed interval.
        runtime_binary = work / "runtime-current"
        for app, detail in report["apps"].items():
            for bench, row in detail["runtime"].items():
                for r in range(-2, args.rounds):
                    for name in order(max(r, 0)):
                        build = detail["builds"][name]
                        shutil.copyfile(build["binary"], runtime_binary)
                        runtime_binary.chmod(0o755)
                        assert sha(runtime_binary) == build["sha256"]
                        command, stdin = invocation(app, runtime_binary, bench)
                        output, elapsed = run(command, stdin=stdin, label="runtime-warmup" if r < 0 else "runtime-sample")
                        assert output.decode() == row["outputs"][name], (app, bench, name)
                        row["warmups_ms" if r < 0 else "samples_ms"][name].append(elapsed)
                    save()
                row["median_ms"] = {n: statistics.median(s) for n, s in row["samples_ms"].items()}
                print("runtime", app, bench, row["median_ms"], flush=True)
        for app, detail in report["apps"].items():
            for name, row in detail["compile"].items():
                row["median_ms"] = statistics.median(row["samples_ms"])
                row["preprocessed_MiB_per_second"] = detail["input_bytes"] / (1024 ** 2) / (row["median_ms"] / 1000)
            assert all(sha(s) == digest for s, digest in detail["sources_sha256"].items())
            assert all(sha(b["binary"]) == b["sha256"] for b in detail["builds"].values())
        assert all(sha(path) == digest for path, digest in report["compiler_sha256"].items())
        report["processes_after"] = run(["ps", "-Ao", "pid,pcpu,comm", "-r"])[0].decode()
        report["completed_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        report["complete"] = True
    except BaseException as exc:
        report["errors"].append(repr(exc))
        raise
    finally:
        save()
        log.close()


if __name__ == "__main__":
    main()
