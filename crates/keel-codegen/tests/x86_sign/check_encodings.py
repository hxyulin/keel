#!/usr/bin/env python3
"""Cross-assemble every exported .s using Clang and compare sign recipe bytes.
Run this entire script under suite/fast_opt_locked.py; no native execution.
"""
import hashlib
import json
import pathlib
import struct
import subprocess


def text(data):
    if data[:6] != b"\x7fELF\x02\x01":
        raise ValueError("expected little-endian ELF64")
    off = struct.unpack_from("<Q", data, 40)[0]
    size, count, strings = struct.unpack_from("<HHH", data, 58)
    rows = [struct.unpack_from("<IIQQQQIIQQ", data, off + i*size) for i in range(count)]
    names = rows[strings]
    names = data[names[4]:names[4]+names[5]]
    for row in rows:
        name = names[row[0]:].split(b"\0", 1)[0]
        if name == b".text":
            return data[row[4]:row[4]+row[5]]
    raise ValueError("missing .text")


def main():
    root = pathlib.Path(__file__).resolve().parent
    manifest = json.loads((root / "manifest.json").read_text())
    out = root / "clang-encodings"
    out.mkdir(exist_ok=True)
    rows = []
    # Exact new FS0/1/2 recipe: shuffle, sign shift, XOR/AND/XOR.
    recipe = bytes.fromhex("66450f70eef566410f72e51f450f57f7450f54ee450f57ef")
    for stem in manifest["stems"]:
        src = root / (stem + ".s")
        obj = out / (stem + ".o")
        argv = ["clang", "-target", "x86_64-unknown-linux-gnu", "-c", str(src), "-o", str(obj)]
        result = subprocess.run(argv, capture_output=True, text=True, timeout=120)
        if result.returncode:
            raise SystemExit(result.stderr)
        ours = text((root / (stem + ".o")).read_bytes())
        clang = text(obj.read_bytes())
        n = ours.count(recipe)
        if (not n and not stem.startswith("controlled-")) or clang.count(recipe) != n:
            raise SystemExit(f"sign recipe mismatch: {stem}: Keel={n}, Clang={clang.count(recipe)}")
        rows.append({"case": stem, "recipe_count": n, "recipe_hex": recipe.hex(),
                     "clang_object_sha256": hashlib.sha256(obj.read_bytes()).hexdigest(),
                     "whole_text_equal": ours == clang, "argv": argv})
    (root / "clang-encodings.json").write_text(json.dumps({"passed": True, "cases": rows}, indent=2)+"\n")
    print(f"Clang sign recipe encodings match in {len(rows)} cases")


if __name__ == "__main__":
    main()
