import argparse
import collections
import json
import struct
import subprocess
from pathlib import Path

p = argparse.ArgumentParser(
    description="Audit GNU-linked native/Clang assembler outputs from compare_native_toolchain.py"
)
p.add_argument("--root", type=Path, required=True)
root = p.parse_args().root.resolve()
report = {}
for mode in ["baseline", "opt"]:
    report[mode] = {}
    for suite, binary in [("lua", "lua"), ("quickjs", "qjs"), ("sqlite", "sqlite3")]:
        report[mode][suite] = {}
        for variant in ["keel_asm_system_link", "clang_asm_system_link"]:
            path = root / mode / suite / (binary + "-" + variant)
            data = path.read_bytes()
            at = struct.unpack_from("<Q", data, 40)[0]
            size, count, si = struct.unpack_from("<HHH", data, 58)
            headers = [
                struct.unpack_from("<IIQQQQIIQQ", data, at + i * size)
                for i in range(count)
            ]
            h = headers[si]
            names = data[h[4] : h[4] + h[5]]
            sections = {
                names[h[0] :].split(b"\0")[0].decode(): {
                    "bytes": h[5],
                    "allocated": bool(h[2] & 2),
                    "type": h[1],
                }
                for h in headers
            }
            symbol_counts = collections.Counter()
            for h in headers:
                if h[1] != 2:
                    continue
                sh = headers[h[6]]
                strings = data[sh[4] : sh[4] + sh[5]]
                for offset in range(h[4], h[4] + h[5], h[9]):
                    index = struct.unpack_from("<I", data, offset)[0]
                    name = strings[index : strings.find(b"\0", index)]
                    symbol_counts["total"] += 1
                    if name.startswith(b".L"):
                        symbol_counts["dot_L"] += 1
            dis = subprocess.check_output(
                ["objdump", "-d", "-j", ".text", str(path)], text=True
            )
            branches = collections.Counter()
            for line in dis.splitlines():
                fields = line.split("\t")
                if len(fields) < 3:
                    continue
                mnemonic = fields[-1].split()
                if not mnemonic or not mnemonic[0].startswith("j"):
                    continue
                raw = fields[1].split()
                if raw[0] == "eb" or 0x70 <= int(raw[0], 16) <= 0x7F:
                    branches["short"] += 1
                elif raw[0] == "e9" or (
                    raw[0] == "0f" and len(raw) > 1 and 0x80 <= int(raw[1], 16) <= 0x8F
                ):
                    branches["near"] += 1
                else:
                    branches["other"] += 1
            stripped = path.with_name(path.name + "-stripped")
            subprocess.run(["strip", "-s", "-o", str(stripped), str(path)], check=True)
            report[mode][suite][variant] = {
                "sections": sections,
                "symbols": dict(symbol_counts),
                "branches": dict(branches),
                "stripped_bytes": stripped.stat().st_size,
            }
(root / "audit.json").write_text(json.dumps(report, indent=2) + "\n")
print("ELF size, symbol, branch and strip audit complete")
