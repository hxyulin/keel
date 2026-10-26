#!/usr/bin/env python3
"""Compare 256 packed/pragma record layouts and initializers against Clang."""
import argparse
import importlib.util
import json
from pathlib import Path
import random
import subprocess


def data_symbols(inspector, path):
    obj = inspector.parse(path)
    result = {}
    for name, _bind, _kind, _visibility, location, size in obj["symbols"]:
        if name.startswith(("value", "layout")):
            section, offset = location
            data = obj["sections"][section]["data"][offset:offset + size]
            result[name] = (size, data)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keel", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--clang", default="clang")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    spec = importlib.util.spec_from_file_location(
        "packed_objinspect", root / "suite/compiler_asm_compat/objinspect.py")
    inspector = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(inspector)
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    rng = random.Random(0x8617a539)
    lines = []
    for n in range(256):
        mode = n % 5
        if mode:
            lines.append(f"#pragma pack(push, {1 << (mode - 1)})")
        attribute = "__attribute__((packed)) " if mode == 0 else ""
        lines.append(f"struct {attribute}S{n} {{")
        values = []
        for j in range(9):
            ty, bits = rng.choice([
                ("unsigned char", 8), ("unsigned short", 16),
                ("unsigned int", 32), ("unsigned long", 64)])
            width = rng.randint(1, bits)
            if j == 3 and n % 3 == 0:
                lines.append("unsigned int :0;")
            if j == 5 and n % 3 == 1:
                lines.append("unsigned char pad;")
                values.append("173")
            lines.append(f"{ty} f{j}:{width};")
            values.append(f"{rng.getrandbits(width)}UL")
        lines.append("};")
        if mode:
            lines.append("#pragma pack(pop)")
        lines.append(f"struct S{n} value{n} = {{" + ",".join(values) + "};")
        lines.append(f"unsigned long layout{n}[2] = {{sizeof(struct S{n}), _Alignof(struct S{n})}};")
    source = work / "matrix.c"
    source.write_text("\n".join(lines) + "\n")
    report = {"seed": "0x8617a539", "records_per_target": 256, "matched_data_symbols": {}}
    for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"]:
        actual = work / f"{target}-keel.o"
        reference = work / f"{target}-clang.o"
        subprocess.run([
            str(args.keel.resolve()), "cc", "--target", target, "--emit-object",
            str(source), "-o", str(actual)], check=True, capture_output=True)
        subprocess.run([
            args.clang, "-w", "-target", target, "-c", str(source), "-o",
            str(reference)], check=True, capture_output=True)
        got = data_symbols(inspector, actual)
        want = data_symbols(inspector, reference)
        assert got.keys() == want.keys(), (target, got.keys() ^ want.keys())
        for name in got:
            assert got[name] == want[name], (target, name, got[name], want[name])
        report["matched_data_symbols"][target] = len(got)
    (work / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
