#!/usr/bin/env python3
"""Separate direct preprocessed C -> object timing on frozen MacHost suite inputs.

Run only after native_baseline.py completes. Reuses its application correctness
gates and verifies Keel object text against the previously assembled objects.
No preprocessing, external assembler, linking, runtime, or broader tests.
"""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys
import time


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--baseline", type=Path, required=True)
    p.add_argument("--work", type=Path, required=True)
    args = p.parse_args()
    previous = json.loads(args.baseline.read_text())
    assert previous.get("complete") and previous.get("correctness_complete")
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    root = Path(previous["configs"]["keel"][0]).parents[2]
    sys.path.insert(0, str(root / "suite"))
    from profile_app_loops import text_bytes
    keel = previous["configs"]["keel"][0]
    configs = {"keel": [keel, "cc", "--emit-object"],
               "keel-O": [keel, "cc", "-O", "--emit-object"],
               "clang-O0": ["/usr/bin/clang", "-w", "-c", "-O0", "-ffp-contract=off"],
               "clang-O2": ["/usr/bin/clang", "-w", "-c", "-O2", "-ffp-contract=off"]}
    report = {"boundary": "serial preprocessed C to object; no preprocessing, separate assembler, linking or runtime",
              "configs": configs, "warmups": 1, "rounds": 3,
              "order_method": "Three distinct positions per config over three measured rounds; final order reverses first; application start rotates. Three rounds cannot balance all four positions exactly.",
              "baseline_report": str(args.baseline.resolve()), "baseline_report_sha256": sha(args.baseline),
              "script_sha256": sha(__file__), "parity_helper_sha256": sha(root / "suite/profile_app_loops.py"),
              "compiler_sha256": previous["compiler_sha256"], "apps": {}, "errors": [],
              "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    commands = (work / "commands.jsonl").open("w", buffering=1)

    def save():
        (work / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    try:
        for path, digest in report["compiler_sha256"].items():
            assert sha(path) == digest
        for app_index, (app, prior) in enumerate(previous["apps"].items()):
            sources = list(map(Path, prior["sources_sha256"]))
            assert all(sha(s) == prior["sources_sha256"][str(s)] for s in sources)
            detail = {"source_count": len(sources), "input_bytes": prior["input_bytes"],
                      "sources_sha256": prior["sources_sha256"], "orders": [],
                      "configs": {n: {"warmups_ms": [], "samples_ms": [], "per_source_ms": [], "text_parity": {}} for n in configs}}
            report["apps"][app] = detail
            names = list(configs)
            names = names[app_index:] + names[:app_index]
            permutations = [[2, 0, 3, 1], [0, 1, 2, 3], [1, 3, 0, 2], [3, 2, 1, 0]]
            for r, permutation in zip(range(-1, 3), permutations):
                order = [names[i] for i in permutation]
                detail["orders"].append({"round": r, "order": order})
                for name in order:
                    directory = work / app / name
                    directory.mkdir(parents=True, exist_ok=True)
                    timings = []
                    for source in sources:
                        obj = directory / (source.stem + ".o")
                        command = [*configs[name], str(source), "-o", str(obj)]
                        start = time.perf_counter_ns()
                        result = subprocess.run(command, capture_output=True, timeout=180)
                        elapsed = (time.perf_counter_ns() - start) / 1e6
                        commands.write(json.dumps({"command": command, "app": app, "config": name, "round": r,
                                                   "wall_ms": elapsed, "returncode": result.returncode,
                                                   "stdout": result.stdout.decode(errors="replace"),
                                                   "stderr": result.stderr.decode(errors="replace")}) + "\n")
                        assert result.returncode == 0, result.stderr.decode(errors="replace")
                        timings.append(elapsed)
                    row = detail["configs"][name]
                    row["warmups_ms" if r < 0 else "samples_ms"].append(sum(timings))
                    row["per_source_ms"].append({"round": r, "sources_ms": dict(zip(map(str, sources), timings))})
                    # Text checks are outside every timed subprocess.
                    if r == -1:
                        for source in sources:
                            obj = directory / (source.stem + ".o")
                            reference = Path(prior["builds"][name]["binary"]).parent / obj.name
                            equal = text_bytes(obj) == text_bytes(reference)
                            row["text_parity"][source.name] = equal
                            if name.startswith("keel"):
                                assert equal, (app, name, source.name, "integrated/assembled text mismatch")
                    print("object", app, r, name, round(sum(timings), 3), flush=True)
                    save()
            for name, row in detail["configs"].items():
                row["median_ms"] = statistics.median(row["samples_ms"])
                row["preprocessed_MiB_per_second"] = detail["input_bytes"] / 1048576 / (row["median_ms"] / 1000)
                row["object_sha256"] = {str(o): sha(o) for o in (work / app / name).glob("*.o")}
            assert all(sha(s) == prior["sources_sha256"][str(s)] for s in sources)
        for path, digest in report["compiler_sha256"].items():
            assert sha(path) == digest
        report["source_count"] = sum(a["source_count"] for a in report["apps"].values())
        assert report["source_count"] == 44
        report["completed_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        report["complete"] = True
    except BaseException as exc:
        report["errors"].append(repr(exc))
        raise
    finally:
        save()
        commands.close()


if __name__ == "__main__":
    main()
