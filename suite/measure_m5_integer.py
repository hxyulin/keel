#!/usr/bin/env python3
"""Measure integer multiply alternatives on Apple M5 using Keel's assembler.

Reports elapsed nanoseconds per operation, never inferred CPU cycles. Latency
chains and eight independent streams are separate experiments. No LLVM tools
are used to generate, assemble or analyze the kernels.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import random
import statistics
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
UNROLL = 64


def run(command):
    result = subprocess.run(command, text=True, capture_output=True, check=True)
    return result.stdout


def kernel(name, bits, multiplier, shift, streams, replacement):
    states = [1, 3, 4, 5, 6, 7, 8, 9][:streams]
    reg = lambda n: f"{'x' if bits == 64 else 'w'}{n}"
    lines = [".text", ".p2align 4", f".globl _{name}", f"_{name}:",
             f"mov {reg(2)}, #{multiplier}"]
    for index, state in enumerate(states[1:], 1):
        lines.append(f"add {reg(state)}, {reg(1)}, #{2 * index}")
    lines.append(f"L_{name}:")
    for i in range(UNROLL):
        r = reg(states[i % streams])
        lines.append(f"add {r}, {r}, {r}, lsl #{shift}" if replacement else f"mul {r}, {r}, {reg(2)}")
    lines += ["subs x0, x0, #1", f"b.ne L_{name}"]
    for state in states[1:]:
        lines.append(f"eor {reg(1)}, {reg(1)}, {reg(state)}")
    lines += [f"mov {reg(0)}, {reg(1)}", "ret"]
    return "\n".join(lines) + "\n"


def harness(kernels):
    declarations = "\n".join(f"extern uint64_t {k['name']}(uint64_t, uint64_t);" for k in kernels)
    entries = ",\n".join(f"{{{k['name']}, {k['bits']}, {k['multiplier']}, {k['streams']}}}" for k in kernels)
    return '''#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <pthread.h>
''' + declarations + '''
typedef uint64_t (*kernel_fn)(uint64_t, uint64_t);
struct kernel { kernel_fn fn; unsigned bits, multiplier, streams; };
static struct kernel kernels[] = {''' + entries + '''};
static uint64_t now(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC_RAW, &t)) exit(4);
    return (uint64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}
int main(int argc, char **argv) {
    if (argc != 4) return 1;
    unsigned index = strtoul(argv[1], 0, 10);
    uint64_t iterations = strtoull(argv[2], 0, 10), seed = strtoull(argv[3], 0, 10);
    if (index >= sizeof(kernels)/sizeof(kernels[0]) || !iterations) return 2;
    struct kernel k = kernels[index];
    int qos = pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
    if (qos) return 3;
    uint64_t begin = now();
    uint64_t result = k.fn(iterations, seed);
    uint64_t elapsed = now() - begin;
    // Independently check each kernel outside the timed region before sampling.
    if (iterations <= 2) {
        uint64_t expected = 0;
        for (unsigned state = 0; state < k.streams; ++state) {
            uint64_t value = seed + 2 * state;
            for (uint64_t j = 0; j < iterations * 64 / k.streams; ++j) value *= k.multiplier;
            if (k.bits == 32) value = (uint32_t)value;
            expected ^= value;
        }
        if (result != expected) return 5;
    }
    printf("%llu %llu\\n", (unsigned long long)elapsed, (unsigned long long)result);
    return 0;
}
'''


def measure(keel, output, pairs=9, iterations=1000000, warmups=2):
    cpu = run(["sysctl", "-n", "machdep.cpu.brand_string"]).strip()
    if platform.system() != "Darwin" or platform.machine() != "arm64" or not cpu.startswith("Apple M5"):
        raise ValueError(f"this calibration requires Apple M5 hardware, found {cpu}")
    kernels = []
    for bits in [32, 64]:
        for multiplier, shift in [(3, 1), (5, 2), (9, 3)]:
            for streams in [1, 8]:
                for replacement in [False, True]:
                    name = f"k_{bits}_{multiplier}_{streams}_{int(replacement)}"
                    kernels.append({"name": name, "bits": bits, "multiplier": multiplier,
                                    "streams": streams, "replacement": replacement,
                                    "source": kernel(name, bits, multiplier, shift, streams, replacement)})
    rng = random.Random(20261015)
    rows = []
    with tempfile.TemporaryDirectory(prefix="keel-m5-integer-") as temporary:
        work = Path(temporary)
        assembly = work / "kernels.s"
        assembly.write_text("".join(k["source"] for k in kernels))
        driver = work / "driver.c"
        driver.write_text(harness(kernels))
        run([str(keel), "asm", str(assembly), "-o", str(work / "kernels.o")])
        run(["clang", "-O2", str(driver), str(work / "kernels.o"), "-o", str(work / "measure")])
        for index in range(len(kernels)):
            for seed in [0, 1, 0x7fffffff, 0xffffffffffffffff]:
                run([str(work / "measure"), str(index), "2", str(seed)])
        for index in range(0, len(kernels), 2):
            samples = {"before": [], "after": []}
            for round_number in range(warmups + pairs):
                order = [0, 1]
                rng.shuffle(order)
                checksums = []
                for variant in order:
                    elapsed, checksum = map(int, run([str(work / "measure"), str(index + variant), str(iterations), "1"]).split())
                    checksums.append(checksum)
                    if round_number >= warmups:
                        samples["after" if variant else "before"].append(elapsed / (iterations * UNROLL))
                if checksums[0] != checksums[1]:
                    raise ValueError("kernel outputs differ")
            ratios = [a / b for a, b in zip(samples["after"], samples["before"])]
            boots = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(2000))
            k = kernels[index]
            rows.append({"bits": k["bits"], "multiplier": k["multiplier"], "streams": k["streams"],
                         "samples_ns_per_operation": samples,
                         "median_ns_per_operation": {v: statistics.median(s) for v, s in samples.items()},
                         "after_over_before": statistics.median(ratios), "ratio_95_interval": [boots[49], boots[1949]]})
            print(k["name"], rows[-1]["after_over_before"], flush=True)
        report = {"cpu": cpu, "host": platform.platform(), "pairs": pairs, "warmups": warmups,
                  "iterations": iterations, "unroll": UNROLL, "unit": "nanoseconds per operation",
                  "method": "Paired seeded random order, same native binary, checksums match; dependent chains and eight independent streams; user-interactive QoS request, no core pinning or clock-frequency inference. Timer brackets kernel only. Constant setup and loop overhead remain included.",
                  "keel_sha256": hashlib.sha256(keel.read_bytes()).hexdigest(),
                  "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  "clang_version": run(["clang", "--version"]).strip(),
                  "assembly_sha256": hashlib.sha256(assembly.read_bytes()).hexdigest(),
                  "assembly": assembly.read_text(), "harness": driver.read_text(), "results": rows}
    output.write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keel", type=Path, default=ROOT / "target/release/keel")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=9)
    parser.add_argument("--iterations", type=int, default=1000000)
    args = parser.parse_args()
    if args.pairs < 3 or args.iterations < 1000:
        parser.error("use at least three pairs and 1000 iterations")
    measure(args.keel.resolve(), args.output, args.pairs, args.iterations)
