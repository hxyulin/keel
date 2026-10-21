#!/usr/bin/env python3
"""Run exported mask objects locally; each compiler/executable takes the suite lock.

python3 crates/keel-codegen/tests/common/run_mask_oracle.py EVIDENCE_DIR
Export objects with KEEL_MASK_EVIDENCE and the vector_mask_proofs cargo test first.
On Linux x86, copy this script, the evidence directory and suite/fast_opt_locked.py.
"""
import json
import pathlib
import platform
import subprocess
import sys


def main():
    evidence = pathlib.Path(sys.argv[1]).resolve()
    arch = {("Darwin", "arm64"): "arm", ("Linux", "x86_64"): "x86"}.get(
        (platform.system(), platform.machine()))
    if not arch:
        raise SystemExit("supported native oracle hosts: macOS ARM64 or Linux x86_64")
    wrapper = pathlib.Path("suite/fast_opt_locked.py").resolve()
    log = pathlib.Path("target/clang-gap-round2-20261021/lock.jsonl").resolve()
    log.parent.mkdir(parents=True, exist_ok=True)
    records = []
    objects = sorted(evidence.glob(f"{arch}-*.o"))
    if len(objects) != 16:
        raise SystemExit(f"expected 16 {arch} objects (two object routes), got {len(objects)}")
    for obj in objects:
        exe = obj.with_suffix(".native")
        compiler = ["clang", "-O2", "-Wall", "-Wextra", "-Werror"]
        if arch == "x86":
            compiler += ["-msse2", "-mno-avx"]
        compiler += [str(evidence / "oracle.c"), str(obj), "-o", str(exe)]
        for command in [compiler, [str(exe)]]:
            result = subprocess.run([sys.executable, str(wrapper), str(log), *command],
                                    capture_output=True, text=True)
            records.append(dict(command=command, returncode=result.returncode,
                                stdout=result.stdout, stderr=result.stderr))
            (evidence / f"native-{arch}.json").write_text(json.dumps(records, indent=2) + "\n")
            if result.returncode:
                raise SystemExit(result.stderr or result.stdout or f"failed: {command}")
    print(f"{arch}: {len(objects)} native objects passed; {evidence / f'native-{arch}.json'}")


if __name__ == "__main__":
    main()
