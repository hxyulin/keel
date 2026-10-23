"""Run exported ELF oracles and compare clang instruction bytes; ROOT is in /tmp."""
import json
import os
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
results = []
for version in ["current", "baseline"]:
    directory = root / version
    for obj in sorted(directory.glob("*.o")):
        exe = directory / "native"
        subprocess.run(
            ["clang", "-O2", "-msse2", "-mno-avx", str(directory / "oracle.c"),
             str(obj), "-o", str(exe)], check=True, capture_output=True,
        )
        subprocess.run([str(exe)], check=True)
        reference = directory / "clang.o.tmp"
        subprocess.run(
            ["clang", "-c", str(obj.with_suffix(".s")), "-o", str(reference)],
            check=True, capture_output=True,
        )
        for source, name in [(obj, "keel.text"), (reference, "clang.text")]:
            subprocess.run(
                ["objcopy", "--dump-section", ".text=" + str(directory / name),
                 str(source)], check=True,
            )
        assert (directory / "keel.text").read_bytes() == (directory / "clang.text").read_bytes(), obj
        results.append(str(obj.relative_to(root)))
(root / "native-results.json").write_text(json.dumps(
    {"passed": results, "count": len(results), "host": os.uname().nodename}, indent=2,
))
print("Native and clang encoding passes:", len(results), flush=True)
