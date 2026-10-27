#!/usr/bin/env python3
"""Compare two pinned kernels in sequential, alternating disposable KVM guests."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import statistics
import subprocess
import time

from reference import initramfs

HERE = Path(__file__).resolve().parent
COUNTS = {"getpid": 5000000, "open_read_close": 300000,
          "page_fault_mprotect": 1310720, "fork_cow_wait": 5000, "pipe_pingpong": 200000}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def measurements(transcript, returncode):
    lines = transcript.replace(b"\r\n", b"\n").splitlines()
    ready = [int(m[1]) for line in lines if (m := re.fullmatch(rb"BENCH_READY (\d+)", line))]
    values = {}
    for line in lines:
        match = re.fullmatch(rb"BENCH ([a-z_]+) (\d+) (\d+)", line)
        if match:
            name, ns, count = match[1].decode(), int(match[2]), int(match[3])
            if name in values or name not in COUNTS or COUNTS[name] != count or ns <= 0:
                raise ValueError("Unexpected or duplicate benchmark result")
            values[name] = ns
    if (returncode != 33 or len(ready) != 1 or set(values) != set(COUNTS)
            or lines.count(b"KEEL_BENCH_OK") != 1 or b"KEEL_BENCH_FAIL" in lines):
        raise ValueError("Guest did not complete every checked workload")
    return {"guest_boot_ns": ready[0], **values}


def boot(command, log):
    start = time.perf_counter_ns()
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    transcript = bytearray()
    ready_ns = None
    timeout = False
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        while selector.get_map():
            if time.perf_counter_ns() - start > 60000000000:
                timeout = True
                process.kill()
                break
            for key, _ in selector.select(timeout=0.1):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                transcript.extend(chunk)
                if ready_ns is None and re.search(rb"BENCH_READY \d+\r?\n", transcript):
                    ready_ns = time.perf_counter_ns() - start
    process.wait()
    process.stdout.close()
    log.write_bytes(transcript)
    result = {"returncode": process.returncode, "timeout": timeout,
              "host_startup_ns": ready_ns, "log_sha256": sha(log)}
    if timeout or ready_ns is None:
        raise RuntimeError(f"Guest failed to start or timed out; see {log}")
    result["values"] = measurements(bytes(transcript), process.returncode)
    result["values"]["host_startup_ns"] = ready_ns
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gcc-kernel", type=Path, required=True)
    parser.add_argument("--keel-kernel", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    parser.add_argument("--cc", default="gcc")
    parser.add_argument("--cpu", type=int, required=True, help="Host logical CPU for the whole VM")
    parser.add_argument("--repeats", type=int, default=9)
    args = parser.parse_args()
    if args.repeats < 3:
        parser.error("At least three measured pairs are required")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    kernels = {"gcc": args.gcc_kernel.resolve(), "keel": args.keel_kernel.resolve()}
    source = HERE / "bench.c"
    compile_command = [args.cc, "-O2", "-march=x86-64", "-mno-sse", "-mno-sse2",
                       "-mno-red-zone", "-static", "-nostdlib", "-ffreestanding",
                       "-fno-pie", "-no-pie", "-fno-stack-protector",
                       "-fno-asynchronous-unwind-tables", "-Wl,--build-id=none",
                       "-Wl,-e,_start", str(source), "-o", str(out / "init")]
    subprocess.run(compile_command, check=True, capture_output=True)
    (out / "initramfs.cpio.gz").write_bytes(initramfs((out / "init").read_bytes()))
    cpuinfo = Path("/proc/cpuinfo").read_text()
    model = re.search(r"^model name\s*:\s*(.+)$", cpuinfo, re.MULTILINE)[1]
    report = {"schema": 1, "acceleration": "KVM", "guest_cpu": "qemu64",
              "guest_vcpus": 1, "guest_ram_mib": 128, "host_cpu_model": model,
              "host_logical_cpu": args.cpu, "host_kernel": os.uname().release,
              "governor": Path(f"/sys/devices/system/cpu/cpu{args.cpu}/cpufreq/scaling_governor").read_text().strip(),
              "host_load_start": os.getloadavg(), "repeats": args.repeats,
              "qemu": subprocess.check_output([args.qemu, "--version"], text=True).splitlines()[0],
              "userspace_cc": subprocess.check_output([args.cc, "--version"], text=True).splitlines()[0],
              "kernel_sha256": {name: sha(path) for name, path in kernels.items()},
              "source_sha256": sha(source), "init_sha256": sha(out / "init"),
              "initramfs_sha256": sha(out / "initramfs.cpio.gz"), "counts": COUNTS,
              "warmups": [], "runs": []}
    base = ["taskset", "-c", str(args.cpu), args.qemu, "-accel", "kvm", "-machine", "pc",
            "-cpu", "qemu64", "-m", "128M", "-smp", "1", "-display", "none",
            "-serial", "stdio", "-monitor", "none", "-no-reboot", "-nic", "none",
            "-device", "isa-debug-exit,iobase=0xf4,iosize=4",
            "-initrd", str(out / "initramfs.cpio.gz"), "-append",
            "console=ttyS0,115200 rdinit=/init panic=-1 quiet loglevel=3"]
    report["qemu_arguments"] = base[4:]
    for pair in range(-1, args.repeats):
        for name in (["gcc", "keel"] if pair % 2 else ["keel", "gcc"]):
            prefix = "warmup" if pair < 0 else f"pair-{pair:02}"
            result = boot(base + ["-kernel", str(kernels[name])], out / f"{prefix}-{name}.log")
            result.update(kernel=name, pair=pair)
            report["warmups" if pair < 0 else "runs"].append(result)
            (out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
            print(f"{prefix} {name}: {result['values']}", flush=True)
    report["summary"] = {}
    for metric in report["runs"][0]["values"]:
        values = {name: [r["values"][metric] for r in report["runs"] if r["kernel"] == name]
                  for name in kernels}
        medians = {name: statistics.median(v) for name, v in values.items()}
        ratios = []
        for pair in range(args.repeats):
            paired = {r["kernel"]: r["values"][metric] for r in report["runs"] if r["pair"] == pair}
            ratios.append(paired["keel"] / paired["gcc"])
        report["summary"][metric] = {
            "median_ns": medians, "min_ns": {k: min(v) for k, v in values.items()},
            "max_ns": {k: max(v) for k, v in values.items()},
            "median_paired_keel_over_gcc": statistics.median(ratios),
            "paired_ratios": ratios}
    report["host_load_end"] = os.getloadavg()
    (out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report["summary"], indent=2))


if __name__ == "__main__":
    main()
