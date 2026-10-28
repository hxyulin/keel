#!/usr/bin/env python3
"""Boot a supplied bzImage and test VFS, memory, fork, COW, pipes and wait."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from reference import initramfs

HERE = Path(__file__).resolve().parent
MARKER = b"KEEL_LINUX64_RUNTIME_OK"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", type=Path, required=True, help="packaged Linux bzImage")
    parser.add_argument("--out", type=Path, required=True, help="new evidence directory")
    parser.add_argument("--cc", default="gcc", help="external reference compiler for userspace")
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    parser.add_argument("--init-source", type=Path, default=HERE / "runtime.c")
    args = parser.parse_args()
    kernel = args.kernel.resolve()
    source = args.init_source.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = {"kernel_sha256": sha(kernel), "init_source_sha256": sha(source),
              "compiler": subprocess.check_output([args.cc, "--version"], text=True).splitlines()[0],
              "qemu": subprocess.check_output([args.qemu, "--version"], text=True).splitlines()[0]}
    command = [args.cc, "-O2", "-static", "-nostdlib", "-ffreestanding", "-fno-pie",
               "-no-pie", "-fno-stack-protector", "-Wl,--build-id=none", "-Wl,-e,_start",
               str(source), "-o", str(out / "init")]
    compiled = subprocess.run(command, capture_output=True)
    (out / "compile.log").write_bytes(compiled.stdout + compiled.stderr)
    report["compile"] = {"command": command, "returncode": compiled.returncode}
    if compiled.returncode:
        (out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        raise SystemExit("Userspace fixture compilation failed; see compile.log")
    (out / "initramfs.cpio.gz").write_bytes(initramfs((out / "init").read_bytes()))
    report["init_sha256"] = sha(out / "init")
    report["initramfs_sha256"] = sha(out / "initramfs.cpio.gz")
    command = [args.qemu, "-accel", "tcg", "-machine", "pc", "-cpu", "qemu64",
               "-m", "128M", "-smp", "1", "-display", "none", "-serial", "stdio",
               "-monitor", "none", "-no-reboot", "-device",
               "isa-debug-exit,iobase=0xf4,iosize=4", "-kernel", str(kernel),
               "-initrd", str(out / "initramfs.cpio.gz"), "-append",
               "console=ttyS0,115200 rdinit=/init panic=-1"]
    try:
        result = subprocess.run(command, capture_output=True, timeout=60)
        transcript = result.stdout + result.stderr
        report["boot"] = {"returncode": result.returncode, "timeout": False}
    except subprocess.TimeoutExpired as error:
        transcript = (error.stdout or b"") + (error.stderr or b"")
        report["boot"] = {"returncode": None, "timeout": True}
    (out / "boot.log").write_bytes(transcript)
    lines = transcript.replace(b"\r\n", b"\n").splitlines()
    stages = [b"KEEL_RUNTIME_VFS_OK", b"KEEL_RUNTIME_MEMORY_OK", b"KEEL_RUNTIME_PROCESS_OK", MARKER]
    report["boot"]["passed"] = (
        report["boot"]["returncode"] == 33
        and all(lines.count(stage) == 1 for stage in stages)
        and b"KEEL_LINUX64_RUNTIME_FAIL" not in lines
        and [line for line in lines if line in stages] == stages
    )
    report["boot"]["log_sha256"] = sha(out / "boot.log")
    (out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report["boot"], indent=2))
    if not report["boot"]["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
