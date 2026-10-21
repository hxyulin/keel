#!/usr/bin/env python3
"""Seal the exact exported object/C/assembly set before parent transfer."""
import hashlib
import json
import pathlib
import struct


def seal(root):
    stems = json.loads((root / "expected-stems.json").read_text())
    count = json.loads((root / "expected-count.json").read_text())
    if not stems or len(stems) != count or len(set(stems)) != count:
        raise SystemExit("invalid expected stems/count")
    files = {}
    for ext in ("o", "c", "s"):
        expected = {f"{stem}.{ext}" for stem in stems}
        found = {p.name for p in root.glob(f"*.{ext}")}
        if found != expected:
            raise SystemExit(f"{ext} files differ from manifest: {sorted(found ^ expected)}")
        for name in sorted(expected):
            data = (root / name).read_bytes()
            if ext == "o" and (data[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<HHI", data, 16) != (1, 62, 1)):
                raise SystemExit(f"not ELF64 little-endian x86_64 relocatable: {name}")
            files[name] = hashlib.sha256(data).hexdigest()
    manifest = {"schema": "keel.sign-select-oracles.v1", "stems": stems, "sha256": files}
    (root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"sealed {len(stems)} ELF+C+assembly cases")


if __name__ == "__main__":
    seal(pathlib.Path(__file__).resolve().parent)
