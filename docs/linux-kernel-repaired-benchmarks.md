# Repaired kernel compiler and runtime measurements

The repaired optimizer snapshot produces runnable kernels at O2, Os and O3.
This comparison measures O2 and Os against GCC 14.4.0 on the pinned minimal
Linux v6.12 captured core. [Raw measurements and hashes](../suite/linux-kernel/audit-policy-fix.json)
include every measured sample and the correctness controls.

## Compilation and linking

Both compilers process the same 484 preprocessed C files and 22 handwritten
assembly files. Compilation includes assembly to objects. Saved assembly from
C inputs is excluded so work is not counted twice. Inputs/configuration hashes
are checked; every repeated object set is identical.

The host is an AMD Ryzen 9 5950X Linux workstation. Keel uses a release build.
Each configuration has one excluded warmup and three measured runs at four
jobs, with reversed configuration order between rounds. Final linking uses
GNU ld 2.46 for GCC and Keel for Keel, with the same expanded script, ordered
archive membership and required 2 MiB LOAD alignment. Link timing is a separate
process measurement with output hashes checked outside its timer.

| Preset | GCC objects | Keel objects | GCC final link | Keel final link |
|---|---:|---:|---:|---:|
| O2 | 36.396 s | 10.792 s | 192.6 ms | 65.6 ms |
| Os | 34.378 s | 10.859 s | 184.1 ms | 62.7 ms |

These are separately timed stages, not full clean Kbuild times. Preprocessing,
host-tool preparation, objtool, archives, packaging and compiler bootstrap are
excluded. GCC retains the captured target/ABI/safety flags. Keel uses its native ABI,
kernel code model, explicit audit feature policy and disabled optional unwind
tables. This does not establish support for every Kbuild flag.

The captured inputs were originally preprocessed with GCC Os:
configuration and optimization macros remain frozen for both measured presets.
The same flag names do not imply identical optimization work.

## Runtime

Runtime uses a separate AMD Ryzen 9 9950X3D Linux workstation, QEMU 11.1.2 KVM,
one qemu64 vCPU, 128 MiB RAM and host logical CPU 2. The governor reports
powersave. This task made no host tuning changes. The CPU is not isolated.
An identical GCC-built freestanding userspace fixture uses direct syscalls,
not libc or vDSO calls. Its hash matches across both presets.

Each matched-preset comparison has one excluded whole-guest warmup per image
and nine alternating measured pairs. Every syscall result, data check and
operation count is validated. Ratios below are medians of paired Keel/GCC
elapsed times, not necessarily quotients of independently computed medians.

| Workload | GCC O2 ms | Keel O2 ms | O2 ratio | GCC Os ms | Keel Os ms | Os ratio |
|---|---:|---:|---:|---:|---:|---:|
| 5,000,000 getpid calls | 154.967 | 248.826 | 1.606 | 332.021 | 492.031 | 1.571 |
| 300,000 open/read/close iterations | 68.622 | 233.073 | 3.398 | 188.012 | 476.030 | 2.519 |
| Fault/protect/check 1,310,720 pages | 377.353 | 895.156 | 2.373 | 828.052 | 1896.119 | 2.257 |
| 5,000 fork/COW/wait iterations | 27.024 | 62.316 | 2.301 | 60.004 | 132.008 | 2.195 |
| 200,000 pipe round trips | 228.875 | 446.038 | 1.955 | 476.030 | 884.055 | 1.830 |

Keel emits objects about 3.17–3.37 times faster here, while these checked kernel
workloads take 1.57–3.40 times as long. This is a compile-time/runtime tradeoff,
not a general performance ranking or evidence that the scheduling repair
caused a runtime speedup. Earlier benchmarks used different compiler revisions
and optimizer combinations.

All **40 positive guests** (including four warmups) passed. Four wrong-PID
controls, one per image/preset, returned the normal FAIL marker and exit 35;
the checker rejected every control. Faults and timeouts never count as passes.

## Size and startup

| Preset | GCC bzImage bytes | Keel bzImage bytes | Keel/GCC |
|---|---:|---:|---:|
| O2 | 1,119,232 | 1,553,408 | 1.388 |
| Os | 926,720 | 1,471,488 | 1.588 |

Optional compiler unwind tables are disabled on both sides. The measured cores
contain zero .eh_frame bytes, so that old source of size inflation is absent.
Executable-section sizes and selected function sizes are recorded in the ledger;
section totals include padding and do not measure resident memory.

Guest init-entry clock readings are within about one percent between matched
images. Host launch-to-ready times vary more. These short runs do not establish
a startup speedup. Firmware/decompression and guest timekeeping also bound what
each clock measures.

## Scope and reproduction

The source base and optimizer overlay hashes identify the isolated compiler
snapshot; concurrent frontend work is excluded. External boot/setup, decompressor
and vDSO preparation remain reference stages. This is not a complete raw-source
Kbuild benchmark, arbitrary configuration support or bare-metal qualification.
It does not test networking, physical storage, multicore scalability or stack
unwinding fidelity.

Use the [compilation runner](../suite/linux-kernel/compile-bench.py) against the
pinned manifest. Rebuild and verify ordered archives, final-link with the
unchanged script, package through [package.py](../suite/linux-kernel/package.py)
and require the correctness fixture to pass first. Then, on a Linux KVM host:

```sh
python3 suite/linux-kernel/bench.py \
  --gcc-kernel /path/to/gcc/bzImage --keel-kernel /path/to/keel/bzImage \
  --cpu 2 --repeats 9 --out /path/to/new-runtime-results
```

Run O2 and Os separately with the corresponding images. Retain small logs,
JSON records and hashes; remove disposable images and object trees afterward.
