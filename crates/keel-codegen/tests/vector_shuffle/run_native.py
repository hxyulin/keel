#!/usr/bin/env python3
"""Run exported ELF+C oracles on Linux x86_64; write native-results.json.

Run python3 ARTIFACT_DIR/run_native.py in a KEEL_SHUFFLE_EVIDENCE export.
No remote access, Cargo, or Keel compiler is needed by this runner.
"""
import argparse
import hashlib
import html
import json
import pathlib
import platform
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cc", default="clang")
    parser.add_argument("--timeout", type=float, default=120, help="seconds per compile or native run")
    parser.add_argument("--dir", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parent)
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "amd64"):
        raise SystemExit("native oracle requires Linux x86_64")
    root = args.dir.resolve()
    rows = []
    for obj in sorted(root.glob("*.o")):
        c = obj.with_suffix(".c")
        if not c.exists():
            continue
        exe = obj.with_suffix(".native")
        build = [args.cc, "-O2", "-march=x86-64", "-msse2", "-mno-avx", "-Wall", "-Wextra", "-Werror", str(c), str(obj), "-o", str(exe)]
        row = {"case": obj.stem, "object_sha256": hashlib.sha256(obj.read_bytes()).hexdigest(),
               "c_sha256": hashlib.sha256(c.read_bytes()).hexdigest(), "build_argv": build}
        try:
            result = subprocess.run(build, capture_output=True, text=True, timeout=args.timeout)
            row.update(build_returncode=result.returncode, build_stdout=result.stdout, build_stderr=result.stderr)
            if result.returncode == 0:
                result = subprocess.run([str(exe)], capture_output=True, text=True, timeout=args.timeout)
                row.update(run_returncode=result.returncode, run_stdout=result.stdout, run_stderr=result.stderr)
        except subprocess.TimeoutExpired as error:
            row.update(timeout_seconds=args.timeout, timeout_argv=error.cmd)
        rows.append(row)
    passed = bool(rows) and all(r.get("run_returncode") == 0 for r in rows)
    report = {"unix_time": time.time(), "host": platform.platform(), "passed": passed, "oracles": rows}
    (root / "native-results.json").write_text(json.dumps(report, indent=2) + "\n")
    (root / "native-results.html").write_text(
        "<!doctype html><meta charset=utf-8><title>Shuffle native correctness</title>"
        "<h1>Shuffle native correctness</h1><pre>" + html.escape(json.dumps(report, indent=2)) + "</pre>\n")
    print(json.dumps({"passed": passed, "oracles": len(rows), "report": str(root / "native-results.json")}))
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
