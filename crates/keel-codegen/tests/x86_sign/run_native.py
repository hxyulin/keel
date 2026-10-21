#!/usr/bin/env python3
"""Run exported ELF+C oracles on Linux x86_64; write native-results.json.

Serialize the entire invocation through the investigation lock:
python3 suite/fast_opt_locked.py target/x86-sign64-20261021/lock.jsonl \
    python3 ARTIFACT_DIR/run_native.py
No remote access, Cargo, or Keel compiler is needed by this runner.
"""
import argparse
import hashlib
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
    root = args.dir.resolve()
    rows = []
    objects = sorted(root.glob("*.o"))
    expected = json.loads((root / "expected-count.json").read_text())
    if len(objects) != expected or expected < 1:
        raise SystemExit(f"expected {expected} objects, found {len(objects)}")
    missing = [str(obj.with_suffix(".c")) for obj in objects if not obj.with_suffix(".c").is_file()]
    if missing:
        raise SystemExit("missing matching C oracles: " + ", ".join(missing))
    manifest = json.loads((root / "manifest.json").read_text())
    if manifest.get("schema") != "keel.sign-select-oracles.v1":
        raise SystemExit("unknown oracle manifest schema")
    stems = manifest["stems"]
    if len(stems) != expected or len(set(stems)) != expected or set(stems) != {p.stem for p in objects}:
        raise SystemExit("oracle stems differ from manifest")
    expected_files = {f"{stem}.{ext}" for stem in stems for ext in ("o", "c", "s")}
    if set(manifest["sha256"]) != expected_files:
        raise SystemExit("oracle file set differs from manifest")
    for ext in ("o", "c", "s"):
        if {p.name for p in root.glob(f"*.{ext}")} != {f"{stem}.{ext}" for stem in stems}:
            raise SystemExit("extra or missing oracle files")
    for name, digest in manifest["sha256"].items():
        if hashlib.sha256((root / name).read_bytes()).hexdigest() != digest:
            raise SystemExit(f"oracle hash mismatch: {name}")
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "amd64"):
        raise SystemExit("native oracle requires Linux x86_64")
    for obj in objects:
        c = obj.with_suffix(".c")
        if not c.exists():
            raise SystemExit(f"missing matching C oracle: {c}")
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
    print(json.dumps({"passed": passed, "oracles": len(rows), "report": str(root / "native-results.json")}))
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
