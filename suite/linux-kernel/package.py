#!/usr/bin/env python3
"""Package a linked Linux v6.12 core using the pinned external boot stages."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def defines(symbols, names, prefix, suffix):
    values = {}
    for line in symbols.splitlines():
        row = line.split()
        if len(row) == 3 and row[2] in names:
            if row[2] in values:
                raise ValueError(f"Duplicate packaging symbol {row[2]}")
            values[row[2]] = int(row[0], 16)
    missing = set(names) - values.keys()
    if missing:
        raise ValueError(f"Missing packaging symbols: {sorted(missing)}")
    return "".join(f"#define {prefix}{name} {suffix.format(value=value)}\n"
                   for name, value in values.items())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-build", type=Path, required=True)
    parser.add_argument("--vmlinux", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    reference = args.reference_build.resolve()
    image = args.vmlinux.resolve()
    out = args.out.resolve()
    config_pin = Path(__file__).with_name("resolved.config")
    if sha(reference / ".config") != sha(config_pin):
        raise SystemExit("Reference boot stages differ from the pinned kernel config")
    out.mkdir(parents=True, exist_ok=False)
    boot = out / "arch/x86/boot"
    shutil.copytree(reference / "arch/x86/boot", boot)
    (out / "include").symlink_to(reference / "include", target_is_directory=True)
    (out / "arch/x86/include").symlink_to(reference / "arch/x86/include", target_is_directory=True)
    shutil.copy2(image, out / "vmlinux")
    report = {"schema": 1, "input_sha256": sha(image),
              "reference_config_sha256": sha(reference / ".config"),
              "scope": "external x86 boot/setup/decompression stages; no core relinking",
              "stages": []}
    environment = dict(os.environ, SOURCE_DATE_EPOCH="1731801600", KBUILD_BUILD_USER="keel",
                       KBUILD_BUILD_HOST="reference", KBUILD_BUILD_VERSION="1",
                       KBUILD_BUILD_TIMESTAMP="Sun Nov 17 00:00:00 UTC 2024")

    def run(label, command, output=None, input_bytes=None):
        start = time.monotonic()
        result = subprocess.run(command, cwd=out, env=environment,
                                capture_output=True, input=input_bytes)
        log = out / f"{label}.log"
        log.write_bytes(result.stdout + result.stderr)
        report["stages"].append({"name": label, "command": command,
                                 "returncode": result.returncode,
                                 "seconds": round(time.monotonic() - start, 3),
                                 "log_sha256": sha(log)})
        (out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        if result.returncode:
            raise SystemExit(f"{label} failed; see {log}")
        if output:
            output.write_bytes(result.stdout)
        return result.stdout.decode() if output is None else None

    def saved(path, label):
        text = (reference / path).read_text().splitlines()[0]
        command = text.split(" := ", 1)[1].strip()
        run(label, shlex.split(command))

    run("sorttable", [str(reference / "scripts/sorttable"), "vmlinux"])
    report["sorted_vmlinux_sha256"] = sha(out / "vmlinux")
    symbols = run("nm", ["nm", "vmlinux"])
    (boot / "voffset.h").write_text(defines(symbols,
        ["_text", "__start_rodata", "__bss_start", "_end"], "VO_", "_AC(0x{value:x},UL)"))
    saved("arch/x86/boot/compressed/.vmlinux.bin.cmd", "strip")
    binary = (boot / "compressed/vmlinux.bin").read_bytes()
    run("gzip", ["gzip", "-n", "-f", "-9"], boot / "compressed/vmlinux.bin.gz", binary)
    run("piggy-source", ["arch/x86/boot/compressed/mkpiggy",
                        "arch/x86/boot/compressed/vmlinux.bin.gz"], boot / "compressed/piggy.S")
    for name in ["misc", "piggy"]:
        saved(f"arch/x86/boot/compressed/.{name}.o.cmd", name)
    saved("arch/x86/boot/compressed/.vmlinux.cmd", "compressed-link")
    symbols = run("compressed-nm", ["nm", "arch/x86/boot/compressed/vmlinux"])
    pattern = re.compile(r"(startup_32|efi.._stub_entry|efi(32)?_pe_entry|input_data|kernel_info|_end|_ehead|_text|_e?data|z_.*)$")
    names = [row[2] for line in symbols.splitlines()
             if len(row := line.split()) == 3 and pattern.fullmatch(row[2])]
    if not {"startup_32", "input_data", "_end"} <= set(names):
        raise SystemExit("Compressed boot image lacks required entry/data/end symbols")
    (boot / "zoffset.h").write_text(defines(symbols, names, "ZO_", "0x{value:x}"))
    for path, label in [("arch/x86/boot/.vmlinux.bin.cmd", "compressed-binary"),
                        ("arch/x86/boot/.header.o.cmd", "header"),
                        ("arch/x86/boot/.setup.elf.cmd", "setup-link"),
                        ("arch/x86/boot/.setup.bin.cmd", "setup-binary"),
                        ("arch/x86/boot/.bzImage.cmd", "bzimage")]:
        saved(path, label)
    report["bzimage_sha256"] = sha(boot / "bzImage")
    report["bzimage_bytes"] = (boot / "bzImage").stat().st_size
    (out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "stages"}, indent=2))


if __name__ == "__main__":
    main()
