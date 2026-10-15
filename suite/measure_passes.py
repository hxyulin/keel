"""Paired application measurements for explicit optimizer pipelines.

Run `cargo suite test` first to populate identical preprocessed sources. Use
immutable release compiler copies and leave this machine idle during timing.
This checks benchmark stdout; --check-tests also runs Lua and SQLite correctness gates.
"""

import argparse
import concurrent.futures
from contextlib import nullcontext
import hashlib
import json
import math
import pathlib
import platform
import random
import re
import shutil
import statistics
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
SUITES = [
    ("lua", "lua", ["-lm"]),
    ("quickjs", "qjs", ["-lm", "-lpthread"]),
    ("sqlite", "sqlite3", ["-lm", "-lpthread"]),
]


def run(command, stdin=None):
    result = subprocess.run(command, input=stdin, capture_output=True, check=False)
    if result.returncode:
        raise RuntimeError(
            f"{command!r}: exit {result.returncode}\n{result.stderr.decode(errors='replace')}"
        )
    return result.stdout


def optimizer_arguments(passes):
    # An empty -p is one empty pass name to the CLI, not an empty pipeline.
    return ["-p", passes] if passes else ["--opt-tier", "none"]


def summary(samples, rng):
    ratios = [a / b for a, b in zip(samples["after"], samples["before"])]
    boots = sorted(
        statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(2000)
    )
    return {
        "samples_ms": samples,
        "before_ms": statistics.median(samples["before"]),
        "after_ms": statistics.median(samples["after"]),
        "after_over_before": statistics.median(ratios),
        "ratio_95_interval": [boots[49], boots[1949]],
    }


def geomean(rows):
    return math.exp(statistics.mean(math.log(row["after_over_before"]) for row in rows))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=pathlib.Path, required=True)
    parser.add_argument("--suites", nargs="+", choices=["lua", "quickjs", "sqlite"], default=["lua", "quickjs", "sqlite"])
    parser.add_argument("--prepared", type=pathlib.Path, default=ROOT / "target/suite", help="read-only prepared suite tree")
    parser.add_argument("--work-dir", type=pathlib.Path, help="retain build artifacts in a new directory")
    parser.add_argument("--check-tests", action="store_true", help="run Lua's portable tests and SQLite queries before timing")
    parser.add_argument("--vector-counts", action="store_true", help="count vectorized functions in printed IR before timing")
    parser.add_argument("--baseline-compiler", type=pathlib.Path)
    parser.add_argument("--baseline-passes", required=True)
    parser.add_argument("--compiler-arg", action="append", default=[], help="extra argument for candidate compiler (repeatable)")
    parser.add_argument("--baseline-arg", action="append", default=[], help="extra argument for baseline compiler (repeatable)")
    parser.add_argument("--assembler", choices=["clang", "keel"], default="clang")
    parser.add_argument(
        "--variant",
        action="append",
        required=True,
        help="NAME=comma,separated,passes (repeatable)",
    )
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--pairs", type=int, default=7)
    parser.add_argument("--warmups", type=int, default=2)
    parser.add_argument("--compile-pairs", type=int, default=5)
    args = parser.parse_args()
    if min(args.pairs, args.compile_pairs) < 3 or args.warmups < 1:
        parser.error("use at least three paired samples and one warmup")
    if args.assembler == "keel" and (platform.system() != "Darwin" or platform.machine() != "arm64"):
        parser.error("Keel object assembly currently requires Apple arm64")
    compiler = str(args.compiler.resolve())
    baseline = str((args.baseline_compiler or args.compiler).resolve())
    configs = {"baseline": (baseline, args.baseline_passes)}
    for variant in args.variant:
        name, sep, passes = variant.partition("=")
        if (
            not sep
            or not name
            or name in configs
            or any(c not in "abcdefghijklmnopqrstuvwxyz0123456789-_" for c in name)
        ):
            parser.error(f"invalid/duplicate variant: {variant!r}")
        configs[name] = (compiler, passes)
    variants = list(configs)[1:]
    rng = random.Random(42)
    suites = [entry for entry in SUITES if entry[0] in args.suites]
    sources = {
        suite: sorted((args.prepared.resolve() / suite / "pre").glob("*.i"))
        for suite, _, _ in suites
    }
    if any(not files for files in sources.values()):
        parser.error("missing preprocessed sources; run cargo suite test first")
    report = {
        "commit": run(["git", "-C", str(ROOT), "rev-parse", "HEAD"]).decode().strip(),
        "host": platform.platform(),
        "measurement_script_sha256": hashlib.sha256(
            pathlib.Path(__file__).read_bytes()
        ).hexdigest(),
        "clang_version": run(["clang", "--version"]).decode().strip(),
        "cpu": run(["sysctl", "-n", "machdep.cpu.brand_string"]).decode().strip()
        if platform.system() == "Darwin"
        else platform.processor(),
        "method": "Paired runs alternate baseline/candidate order, with seeded shuffled variant order in each round. Every benchmark stdout must match its baseline. Medians of paired ratios; 95% bootstrap intervals (2000 samples). Compilation serial, assembly/linking excluded; all builds finish before timing.",
        "runtime_pairs": args.pairs,
        "warmups": args.warmups,
        "compile_pairs": args.compile_pairs,
        "compiler_sha256": {
            name: hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest()
            for name, (binary, _) in configs.items()
        },
        "assembler": args.assembler,
        "compiler_arguments": {name: args.baseline_arg if name == "baseline" else args.compiler_arg for name in configs},
        "pipelines": {
            name: passes.split(",") if passes else []
            for name, (_, passes) in configs.items()
        },
        "sources_sha256": {
            str(source): hashlib.sha256(
                source.read_bytes()
            ).hexdigest()
            for files in sources.values()
            for source in files
        },
        "variants": {
            name: {"runtime": [], "compile": [], "code": []} for name in variants
        },
    }
    if args.work_dir:
        if args.work_dir.resolve() == args.prepared.resolve() or args.prepared.resolve() in args.work_dir.resolve().parents:
            parser.error("work directory must be outside the prepared inputs")
        args.work_dir.mkdir(parents=True, exist_ok=False)
    context = nullcontext(str(args.work_dir.resolve())) if args.work_dir else tempfile.TemporaryDirectory(prefix="keel-pass-matrix-")
    with context as temporary:
        work = pathlib.Path(temporary)
        report["work_dir"] = str(work)
        report["checks"] = []
        report["vector_ir"] = []
        for suite, binary, libs in suites:
            sizes = {}
            for name, (driver, passes) in configs.items():
                out = work / name / suite
                out.mkdir(parents=True)

                extra = args.baseline_arg if name == "baseline" else args.compiler_arg

                def build(source, out=out, driver=driver, passes=passes, extra=extra):
                    asm = out / (source.stem + ".s")
                    obj = asm.with_suffix(".o")
                    run([driver, "cc", *extra, *optimizer_arguments(passes), str(source), "-o", str(asm)])
                    if args.assembler == "keel":
                        run([driver, "asm", str(asm), "-o", str(obj)])
                    else:
                        run(["clang", "-c", str(asm), "-o", str(obj)])
                    return obj

                with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                    objects = list(pool.map(build, sources[suite]))
                run(["clang", *map(str, objects), "-o", str(out / binary), *libs])
                size = (
                    run(["size", "-m", str(out / binary)]).decode()
                    if platform.system() == "Darwin"
                    else run(["size", str(out / binary)]).decode()
                )
                sizes[name] = (
                    int(
                        next(
                            line for line in size.splitlines() if "__text" in line
                        ).split()[-1]
                    )
                    if platform.system() == "Darwin"
                    else int(size.splitlines()[1].split()[0])
                )
                if args.vector_counts:
                    functions = []
                    for source in sources[suite]:
                        text = run([driver, "cc", *extra, *optimizer_arguments(passes), "--emit-ir", str(source)]).decode()
                        function = None
                        for line in text.splitlines():
                            if line.startswith("func @"):
                                function = line.split("(")[0][6:]
                            if function and re.search(r"\b(?:i8x16|i16x8|i32x4|i64x2|f32x4|f64x2)\b", line):
                                tag = f"{source.name}:{function}"
                                if tag not in functions:
                                    functions.append(tag)
                    report["vector_ir"].append({"suite": suite, "config": name, "functions": functions, "count": len(functions)})
                if args.check_tests and suite == "lua":
                    fixture = out / "tests"
                    shutil.copytree(args.prepared.resolve() / suite / "lua-5.4.7-tests", fixture)
                    result = subprocess.run([str(out / binary), "-e", "_U=true", "all.lua"], cwd=fixture, capture_output=True, timeout=300)
                    (out / "correctness.log").write_bytes(result.stdout + result.stderr)
                    if result.returncode:
                        raise RuntimeError(f"Lua correctness failed: {name}; see {out / 'correctness.log'}")
                    report["checks"].append({"suite":suite,"config":name,"test":"Lua portable suite","passed":True})
                elif args.check_tests and suite == "sqlite":
                    answer = run([str(out / binary), ":memory:"], (ROOT / "suite/sqlite/queries.sql").read_bytes())
                    (out / "correctness.log").write_bytes(answer)
                    if name != "baseline" and answer != (work / "baseline" / suite / "correctness.log").read_bytes():
                        raise RuntimeError(f"SQLite correctness mismatch: {name}")
                    report["checks"].append({"suite":suite,"config":name,"test":"SQLite queries match baseline","passed":True})
                print(f"built {suite}/{name}", flush=True)
            for name in variants:
                changed = sum(
                    (work / "baseline" / suite / (s.stem + ".s")).read_bytes()
                    != (work / name / suite / (s.stem + ".s")).read_bytes()
                    for s in sources[suite]
                )
                report["variants"][name]["code"].append(
                    {
                        "suite": suite,
                        "before": sizes["baseline"],
                        "after": sizes[name],
                        "assembly_files_changed": changed,
                        "files": len(sources[suite]),
                        "executable_sha256": {config: hashlib.sha256((work / config / suite / binary).read_bytes()).hexdigest() for config in ["baseline", name]},
                    }
                )

        for suite, binary, _ in suites:
            benches = (
                ["fib", "sort", "string", "float"]
                if suite == "lua"
                else ["fib", "sort", "string"]
                if suite == "quickjs"
                else ["queries"]
            )
            for bench in benches:
                samples = {name: {"before": [], "after": []} for name in variants}
                for iteration in range(args.warmups + args.pairs):
                    order = variants.copy()
                    rng.shuffle(order)
                    for name in order:
                        outputs = {}
                        for version, config in (
                            [("before", "baseline"), ("after", name)]
                            if iteration % 2 == 0
                            else [("after", name), ("before", "baseline")]
                        ):
                            exe = str(work / config / suite / binary)
                            if suite == "lua":
                                command = [
                                    exe,
                                    str(ROOT / "suite/lua/bench.lua"),
                                    bench,
                                ]
                                stdin = None
                            elif suite == "quickjs":
                                command = [
                                    exe,
                                    "--stack-size",
                                    "4194304",
                                    str(ROOT / "suite/quickjs/bench.js"),
                                    bench,
                                ]
                                stdin = None
                            else:
                                command = [exe, ":memory:"]
                                stdin = (ROOT / "suite/sqlite/bench.sql").read_bytes()
                            start = time.perf_counter()
                            outputs[version] = run(command, stdin)
                            elapsed = (time.perf_counter() - start) * 1000
                            if iteration >= args.warmups:
                                samples[name][version].append(elapsed)
                        if outputs["before"] != outputs["after"]:
                            raise RuntimeError(
                                f"stdout mismatch: {name}/{suite}/{bench}"
                            )
                for name in variants:
                    row = {
                        "suite": suite,
                        "bench": bench,
                        **summary(samples[name], rng),
                    }
                    report["variants"][name]["runtime"].append(row)
                    print(
                        f"{name} {suite}/{bench}: {row['after_over_before']:.4f}",
                        flush=True,
                    )

        for suite, _, _ in suites:
            samples = {name: {"before": [], "after": []} for name in variants}
            for iteration in range(1 + args.compile_pairs):
                order = variants.copy()
                rng.shuffle(order)
                for name in order:
                    for version, config in (
                        [("before", "baseline"), ("after", name)]
                        if iteration % 2 == 0
                        else [("after", name), ("before", "baseline")]
                    ):
                        driver, passes = configs[config]
                        start = time.perf_counter()
                        for source in sources[suite]:
                            run(
                                [
                                    driver,
                                    "cc",
                                    *(args.baseline_arg if config == "baseline" else args.compiler_arg),
                                    *optimizer_arguments(passes),
                                    str(source),
                                    "-o",
                                    "/dev/null",
                                ]
                            )
                        if iteration:
                            samples[name][version].append(
                                (time.perf_counter() - start) * 1000
                            )
            for name in variants:
                row = {
                    "suite": suite,
                    "files": len(sources[suite]),
                    **summary(samples[name], rng),
                }
                report["variants"][name]["compile"].append(row)
                print(
                    f"{name} compile/{suite}: {row['after_over_before']:.4f}",
                    flush=True,
                )

    for variant in report["variants"].values():
        variant["runtime_geomean"] = geomean(variant["runtime"])
        variant["compile_geomean"] = geomean(variant["compile"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"saved {args.output}", flush=True)


if __name__ == "__main__":
    main()
