"""Alternate pinned short-kernel samples; invoke through fast_opt_locked.py."""
import json
import os
import pathlib
import statistics
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
source = pathlib.Path(__file__).with_name("bench.c")
for version in ["current", "baseline"]:
    subprocess.run(
        ["clang", "-O2", "-msse2", "-mno-avx", str(source),
         str(root / version / "V1-Full-Full-normal.o"),
         "-o", str(root / version / "bench")], check=True,
    )
rows = []
checksums = {}
for repetition in range(9):
    order = ["baseline", "current"] if repetition % 2 == 0 else ["current", "baseline"]
    for version in order:
        before = os.getloadavg()
        output = subprocess.check_output(
            ["taskset", "-c", str(min(os.sched_getaffinity(0))),
             str(root / version / "bench")], text=True,
        )
        for line in output.splitlines():
            shape, _, checksum = line.split()
            assert checksums.setdefault(shape, checksum) == checksum
        rows.append(dict(repetition=repetition, version=version, load_before=before,
                         load_after=os.getloadavg(), output=output))
(root / "timings.json").write_text(json.dumps(rows, indent=2))
for version in ["baseline", "current"]:
    for shape in ["i64x2", "i32x4"]:
        samples = [float(line.split()[1]) for row in rows if row["version"] == version
                   for line in row["output"].splitlines() if line.startswith(shape)]
        print(version, shape, "median ns/call", statistics.median(samples),
              "range", min(samples), max(samples))
