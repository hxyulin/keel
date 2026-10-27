#!/usr/bin/env python3
"""Apply the reviewed FPU link workaround to one pinned optimized IR input.

This experiment is not a general interprocedural optimization. Its assembly
macros were reviewed manually; see docs/linux-kernel-link-fixes.md.
"""
import argparse
import hashlib
from pathlib import Path
import re
import subprocess
import tempfile

INPUT_SHA256 = "0a74ece5da962e5ceae52251ce84ae668a2a12c160867c9b441d146bebf20cf6"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keel", required=True)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    raw = args.input.read_bytes()
    if hashlib.sha256(raw).hexdigest() != INPUT_SHA256:
        raise SystemExit("Input differs from the reviewed optimized IR; do not apply this workaround.")
    if args.out.exists():
        raise SystemExit("Output already exists.")
    text = raw.decode()
    # Counts include assembly strings and data initializers, not just calls.
    assert text.count("save_fsave_header") == 1
    assert text.count("__fpu_restore_sig") == 2
    parts = text.split("\nfunc ")
    calls = [part for part in parts if "call @__fpu_restore_sig(" in part]
    assert len(calls) == 1
    call = re.search(r"call @__fpu_restore_sig\((v\d+), (v\d+), (v\d+)\)", calls[0])
    assert call is not None
    assert re.search(rf"^\s+{call[3]} = iconst.i8 0$", calls[0], re.MULTILINE)
    saved = [part for part in parts if part.startswith("@save_fsave_header(")]
    restore = [part for part in parts if part.startswith("@__fpu_restore_sig(")]
    assert len(saved) == len(restore) == 1
    for part in [saved[0], restore[0]]:
        header = part.splitlines()[0]
        assert " local {" in header
        assert not re.search(r"\b(used|retain|weak|constructor|destructor)\b", header)
    body = restore[0]
    entry = re.search(r"\nblock0\(v0: ptr, v1: ptr, (v\d+): i8\):\n", body)
    assert entry is not None
    parameter = entry[1]
    # Never substitute inside assembly strings or other quoted metadata.
    assert all(not re.search(rf"\b{parameter}\b", quoted[0])
               for quoted in re.finditer(r'"(?:\\.|[^"\\])*"', body))
    fresh = "v" + str(max(map(int, re.findall(r"\bv(\d+)\b", body))) + 1)
    body = (body[:entry.end()] + f"    {fresh} = iconst.i8 0\n"
            + re.sub(rf"\b{parameter}\b", fresh, body[entry.end():]))
    parts = [body if part == restore[0] else part for part in parts if part != saved[0]]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="keel-fpu-link-") as directory:
        changed = Path(directory) / "manual.keel"
        changed.write_text("\nfunc ".join(parts))
        # Parsing and optimizer verification check the resulting IR contract.
        subprocess.run([args.keel, "opt", "-p", "simplify-cfg,instcombine,dce",
                        str(changed), "-o", str(args.out)], check=True)


if __name__ == "__main__":
    main()
