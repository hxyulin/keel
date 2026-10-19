#!/usr/bin/env python3
"""Summarize native_baseline.py evidence without re-running benchmarks."""
import argparse
import json
import math
from pathlib import Path
import random
import statistics


def geo(values):
    return math.exp(statistics.mean(math.log(x) for x in values))


def paired(a, b):
    ratios = [x / y for x, y in zip(a, b)]
    rng = random.Random(20261019)
    boot = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(4000))
    return {"paired_median_ratio": statistics.median(ratios),
            "paired_bootstrap_95": [boot[99], boot[3899]],
            "paired_ratios": ratios, "ratio_of_medians": statistics.median(a) / statistics.median(b)}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--evidence", type=Path, required=True)
    p.add_argument("--summary", type=Path, required=True)
    args = p.parse_args()
    evidence = args.evidence.resolve()
    report = json.loads((evidence / "measurement/report.json").read_text())
    source = json.loads((evidence / "source-identity.json").read_text())
    assert report.get("complete") and report.get("correctness_complete"), report.get("errors")
    stats = {"runtime": {}, "compile": {}, "aggregate": {}}
    lines = ["# MacHost baseline — 2026-10-19", "",
             "Measured on Apple M5 Max, 18 cores, 128 GiB RAM; macOS 27.0.1 (26A434), Apple Clang 21.0.0, Rust 1.99.0.", "",
             "GCC unavailable: `/usr/bin/gcc --version` identifies Apple Clang. Versioned GCC executables and conventional Homebrew/Nix profile/store paths were checked; no independent GCC found. No packages or system settings were changed.", "",
             "Source revision: `{}`. Initial status: `{}`. Tracked diff SHA-256: `{}`. Dirty-state SHA-256: `{}`. Frozen source-tree SHA-256: `{}`.".format(source["revision"], source["status"].strip(), source["diff_sha256"], source["dirty_state_sha256"], source["source_tree_sha256"]),
             "The only pre-existing dirty state was an untracked generated output directory (`target-pattern-opt-rotate-20261017/`, about 1 GiB); it was recorded and excluded from the compiler source snapshot. All 983 tracked working files were copied and hash-verified. The new harness is independently hashed in the report.", "",
             "Scratch directory: `{}`. This execution environment itself identified as MacHost; commands were nevertheless launched through SSH into this isolated scratch tree. Existing remote checkouts and configuration were untouched.".format(source["scratch"]), "",
             "Release build: `cargo build --release --locked -j 4 -p keel -p keel-suite`, completed in 19.73 s. Existing SHA-256-checked suite archives were transferred, then the stock suite freshly prepared the inputs with Clang. Lua 5.4.7, QuickJS 2024-01-13 with BigNum and SQLite 3.46.1 were rebuilt for all four configurations.", "",
             "Configurations: Keel default; Keel `-O`; Clang `-O0 -ffp-contract=off`; Clang `-O2 -ffp-contract=off`. No native targeting, LTO, PGO or fast-math. All configurations compile identical preprocessed inputs and use the same Apple Clang assembler/linker and system libraries.", "",
             "Correctness: all four configurations passed Lua's portable suite, all nine QuickJS tests and SQLite query-output comparison against Clang O2. All eight benchmark outputs matched Clang O2 before timing and on every warmup and measured execution. Source and binary hashes were verified again after timing.", "",
             "Runtime is CLI process wall time including startup: two warmups, eight rounds per configuration/workload. Each forward/reverse pair uses the same rotated start. Binaries are copied/hash-checked to one common executable pathname outside timing. Compilation is serial preprocessed C to assembly, excluding preprocessing, external assembly and linking: one warmup and three rounds per application/configuration. Assembly and link times from initial builds are retained separately. Timers include subprocess/I/O overhead; these are warm-cache results, not internal-stage or cold-start measurements.", "",
             "## Runtime medians (milliseconds)", "",
             "| Workload | Keel | Keel -O | Clang O0 | Clang O2 | Keel/Clang O0 | Keel -O/Clang O2 (paired 95% interval) |",
             "|---|---:|---:|---:|---:|---:|---:|"]
    for app, detail in report["apps"].items():
        for bench, row in detail["runtime"].items():
            samples = row["samples_ms"]
            assert all(len(s) == report["runtime_rounds"] for s in samples.values())
            medians = {n: statistics.median(s) for n, s in samples.items()}
            key = app + "/" + bench
            unopt = paired(samples["keel"], samples["clang-O0"])
            opt = paired(samples["keel-O"], samples["clang-O2"])
            stats["runtime"][key] = {"unoptimized": unopt, "optimized": opt}
            lines.append("| {} | {:.3f} | {:.3f} | {:.3f} | {:.3f} | {:.3f}x | {:.3f}x [{:.3f}, {:.3f}] |".format(key, medians["keel"], medians["keel-O"], medians["clang-O0"], medians["clang-O2"], unopt["paired_median_ratio"], opt["paired_median_ratio"], *opt["paired_bootstrap_95"]))
    lines += ["", "Ratios are Keel time / Clang time, so values below one are faster. Intervals use 4,000 seeded resamples of same-round ratios; eight rounds describe this short campaign, not general workload performance.", "", "## C-to-assembly compile throughput", "",
              "Serial C-to-assembly batch medians; all translation units are recompiled each sample. Units are milliseconds and preprocessed MiB/s.", "",
              "| Application / units | Keel ms (MiB/s) | Keel -O ms (MiB/s) | Clang O0 ms (MiB/s) | Clang O2 ms (MiB/s) |",
              "|---|---:|---:|---:|---:|"]
    for app, detail in report["apps"].items():
        cells = []
        for name in ["keel", "keel-O", "clang-O0", "clang-O2"]:
            row = detail["compile"][name]
            assert len(row["samples_ms"]) == report["compile_rounds"]
            median = statistics.median(row["samples_ms"])
            cells.append("{:.2f} ({:.2f})".format(median, detail["input_bytes"] / 1048576 / (median / 1000)))
        lines.append("| {} / {} | {} |".format(app, detail["source_count"], " | ".join(cells)))
        stats["compile"][app] = {"unoptimized": paired(detail["compile"]["keel"]["samples_ms"], detail["compile"]["clang-O0"]["samples_ms"]), "optimized": paired(detail["compile"]["keel-O"]["samples_ms"], detail["compile"]["clang-O2"]["samples_ms"])}
    for mode in ["unoptimized", "optimized"]:
        stats["aggregate"][mode] = {"runtime_equal_workload_geomean": geo(r[mode]["paired_median_ratio"] for r in stats["runtime"].values()), "compile_equal_application_geomean": geo(r[mode]["paired_median_ratio"] for r in stats["compile"].values())}
    object_report = json.loads((evidence / "object-measurement/report.json").read_text())
    assert object_report.get("complete") and object_report["source_count"] == 44
    stats["direct_object"] = {}
    lines += ["", "## Direct C-to-object compile throughput (separate phase)", "",
              "Measured after the complete runtime/assembly campaign, using the same 44 preprocessed files and frozen release compiler. Keel flags: `cc --emit-object` and `cc -O --emit-object`; Clang flags: `-w -c -O0 -ffp-contract=off` and `-w -c -O2 -ffp-contract=off`. These commands include each compiler's integrated object generation. No external assembly, preprocessing or linking is included.", "",
              "One warmup and three serial measured rounds per application/configuration; rotated start per application, three distinct temporal positions per configuration, final order reversing the first. Three rounds cannot balance all four temporal positions exactly. Keel's integrated object text matched every previously externally assembled object at both optimization levels (88 checks). These are useful compile-to-object comparisons, not full-build measurements.", "",
              "| Application / units | Keel ms (MiB/s) | Keel -O ms (MiB/s) | Clang O0 ms (MiB/s) | Clang O2 ms (MiB/s) | Keel/Clang O0 | Keel -O/Clang O2 |",
              "|---|---:|---:|---:|---:|---:|---:|"]
    parity_count = 0
    for app, detail in object_report["apps"].items():
        assert detail["sources_sha256"] == report["apps"][app]["sources_sha256"]
        cells = []
        for name in ["keel", "keel-O", "clang-O0", "clang-O2"]:
            row = detail["configs"][name]
            assert len(row["samples_ms"]) == 3 and len(row["warmups_ms"]) == 1
            median = statistics.median(row["samples_ms"])
            cells.append("{:.2f} ({:.2f})".format(median, detail["input_bytes"] / 1048576 / (median / 1000)))
            if name.startswith("keel"):
                assert all(row["text_parity"].values())
                parity_count += len(row["text_parity"])
        rows = detail["configs"]
        stats["direct_object"][app] = {"unoptimized": paired(rows["keel"]["samples_ms"], rows["clang-O0"]["samples_ms"]), "optimized": paired(rows["keel-O"]["samples_ms"], rows["clang-O2"]["samples_ms"])}
        lines.append("| {} / {} | {} | {:.3f}x | {:.3f}x |".format(app, detail["source_count"], " | ".join(cells), stats["direct_object"][app]["unoptimized"]["paired_median_ratio"], stats["direct_object"][app]["optimized"]["paired_median_ratio"]))
    assert parity_count == 88
    for mode in ["unoptimized", "optimized"]:
        stats["aggregate"][mode]["direct_object_equal_application_geomean"] = geo(r[mode]["paired_median_ratio"] for r in stats["direct_object"].values())
    lines += ["", "Equal-workload geometric mean runtime ratios: default Keel / Clang O0 **{:.3f}x**; Keel -O / Clang O2 **{:.3f}x**. Equal-application geometric mean C-to-assembly compile-time ratios: **{:.3f}x** and **{:.3f}x**, respectively.".format(stats["aggregate"]["unoptimized"]["runtime_equal_workload_geomean"], stats["aggregate"]["optimized"]["runtime_equal_workload_geomean"], stats["aggregate"]["unoptimized"]["compile_equal_application_geomean"], stats["aggregate"]["optimized"]["compile_equal_application_geomean"]), "",
              "Direct C-to-object equal-application geometric mean time ratios are **{:.3f}x** (Keel / Clang O0) and **{:.3f}x** (Keel -O / Clang O2). None of these ratios measures preprocessing, linking, parallel full builds or resulting program runtime.".format(stats["aggregate"]["unoptimized"]["direct_object_equal_application_geomean"], stats["aggregate"]["optimized"]["direct_object_equal_application_geomean"]), "",
              "Limitations: host was not reserved; normal desktop/services remained active. No core affinity or power-policy changes. Process/thermal snapshots are retained. Three compile samples support only a short descriptive comparison; application, architecture, compiler-version and system-library effects limit generalization. There is no independent GCC result.", "",
              "## Commands and raw evidence", "",
              "Exact commands, stdout/stderr, return codes, stdin hashes and wall times: `{}/measurement/commands.jsonl`. Raw samples, flags, source/compiler/executable hashes, checks and process snapshots: `{}/measurement/report.json`. Derived paired ratios: `{}/statistics.json`. Host discovery: `host-tools.txt`. Release/preparation/measurement logs and source/input archives are retained beside them.".format(evidence, evidence, evidence), "",
              "Direct-object commands/per-file raw timings: `object-measurement/commands.jsonl`; batch samples, orders, exact flags, object hashes and text-parity checks: `object-measurement/report.json`. Frozen release compiler and exact prepared inputs: `keel-release-frozen` and `prepared-inputs.tar.gz`.", "",
              "Reproduce in a fresh scratch tree after extracting `source.tar.gz` and `suite-inputs.tar.gz` and copying the two harness files into `suite/`:", "", "```sh", "cargo build --release --locked -j 4 -p keel -p keel-suite", "target/release/keel-suite --jobs 2 test lua quickjs sqlite -c clang-O2 --release", "python3 suite/native_baseline.py --root \"$PWD\" --work /tmp/machost-new-measurement --rounds 8 --compile-rounds 3", "python3 suite/native_object.py --baseline /tmp/machost-new-measurement/report.json --work /tmp/machost-new-object-measurement", "```", "",
              "The harness is provided separately beside the source archive because the source snapshot preceded its creation. No commits were made. Only MacHost was benchmarked."]
    (evidence / "statistics.json").write_text(json.dumps(stats, indent=2) + "\n")
    args.summary.write_text("\n".join(lines) + "\n")
    print(json.dumps(stats["aggregate"], indent=2))


if __name__ == "__main__":
    main()
