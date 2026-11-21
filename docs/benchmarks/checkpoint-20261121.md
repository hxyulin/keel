# Performance checkpoint, November 21, 2026

Keel revision `811c69a1` was built in release mode. The
[chart data](checkpoint-20261121.json) records the samples, correctness
checksums, artifact hashes and tool versions.

## Application runtime

Keel `-O` and Clang `-O2` built the same pinned C suites with their own
preprocessors. Both configurations passed the suite checks. Each workload
ran once as a warmup and then in seven rotated rounds. Each round included
an identical copy of the Clang image as a control. Timings cover the whole
process, including startup. Ratios are medians of paired wall times.
Linux ran on one pinned CPU on a Ryzen 9 9950X3D with Clang 23.1.5. macOS
ran on an Apple M5 Max with Apple Clang 21.0.0.

| Application | Apple M5 Max | AMD Zen 5 Linux |
| --- | ---: | ---: |
| Lua | 1.17× | 1.21× |
| SQLite | 1.20× | 1.25× |
| LZ4 | 1.07× | 0.91× |
| yyjson | 1.09× | 1.08× |
| QuickJS | 1.13× | 1.29× |
| Zstd | 1.03× | 1.06× |
| Overall | **1.11×** | **1.13×** |

Each application is the geometric mean of its workloads. The overall result
weights each application equally. The [November 5 baseline](runtime-20261105.md)
reported 1.37× on M5 Max and 1.35× on Zen 5. The host software and Linux
Clang version changed between runs. QuickJS string remains the widest
individual gap at 1.26× and 1.35×. On Linux, Keel's LZ4 compression took
0.84× Clang's wall time. The largest identical-control deviations were 5.2%
on M5 Max and 3.9% on Zen 5.

## Compilation and generated kernel code

The C-to-object benchmark uses one translation unit from the bridge's
[kernel fixture](../../crates/keel-cranelift/kernels/kernels.c). It includes
compiler startup, preprocessing, optimization, code generation, assembly and
object writing. The IR-to-object benchmark feeds identical optimized Keel IR
to both backends and sums their in-process stage timers. The backend
measurement excludes IR parsing, IR optimization and file I/O. Keel emits unwind
metadata, and Cranelift runs with unwind generation disabled.

| Stage and configuration | Apple M5 Max | Intel Core i7-1360P Linux |
| --- | ---: | ---: |
| C to object, Keel | 4.90 ms | 4.89 ms |
| C to object, Keel `-O` | 6.93 ms | 7.77 ms |
| C to object, Clang `-O0` | 18.00 ms | 20.62 ms |
| C to object, Clang `-O2` | 43.97 ms | 85.51 ms |
| Optimized Keel IR to object, Keel baseline | 0.555 ms | 1.482 ms |
| Optimized Keel IR to object, Keel full | 0.587 ms | 1.531 ms |
| Optimized Keel IR to object, Cranelift `none` | 0.982 ms | 1.836 ms |
| Optimized Keel IR to object, Cranelift `speed` | 1.359 ms | 2.477 ms |

Linux used Ubuntu Clang 18.1.3 and one pinned CPU. Both hosts used Cranelift
0.136.3, with seven measured rounds following two warmups. For runtime, the
driver interleaves configurations for each of 21 kernels, checks their
checksums, and measures time within the driver. On the same optimized Keel IR,
Keel took **0.90×** Cranelift `speed` time on macOS and **0.96×** on Linux by equally weighted geometric
mean. On Linux the ratios ranged from 0.34× for dispatch to 1.51× for narrow
integers. The C-to-object Clang builds also ran the kernels as a same-source
LLVM comparison. Against Clang `-O2` the kernel geomeans were 1.01× on macOS
and 1.14× on Linux. The [LLVM IR exporter](../../crates/keel-llvm/README.md#explicit-limitations)
cannot translate the full kernel set. The [Cranelift bridge](../../crates/keel-cranelift/README.md)
compares the generated kernels. See the [November 5 compiler baseline](compiler-20261105.md)
for the earlier compilation measurements.

## Assembly and linking

The Mach-O sample assembles two identical pieces of Clang-generated text
and separately links the same pair of objects into an ad-hoc signed
executable. The ELF sample links two identical Clang PIC objects with the
same glibc startup objects, DSO and interpreter into a dynamically linked
non-PIE executable. The Linux sample runs on one pinned Zen 5 CPU. Each
linker output passed 48 independent checks across 12 kernels and four input
lengths.

| Sample | Keel | Comparators |
| --- | ---: | --- |
| Mach-O assembly | 7.18 ms | Clang integrated assembler 19.64 ms, Clang driver 26.28 ms |
| Mach-O link and signing | 4.82 ms | Apple ld 15.32 ms, Clang driver 23.56 ms |
| ELF link | 2.97 ms | GNU ld.bfd 2.47 at 6.00 ms, LLVM LLD 23.1.5 at 6.60 ms |

These are medians of seven Mach-O and eleven ELF rounds, following two
warmups. The timed commands include process startup and output file writes.
Keel's previous result on the [Mach-O fixture](assembly-link-20261105.md)
was 16.7 ms. ELF timings cover a small hosted fixture and each tool's
default layout and feature set. The separate [Linux kernel benchmark](linux-kernel.md)
still describes the earlier pinned KVM campaign.

## Reproduce

```sh
cargo build --release --locked -p keel -p keel-suite
cargo build --release --locked --manifest-path crates/keel-cranelift/Cargo.toml --bin keel-cranelift-bench
cargo suite test -c clang-O2,keel-O
cargo suite run -c clang-O2,keel-O -n 7
python3 suite/integration/compiler-bench/compiler.py --runtime \
  --keel target/release/keel \
  --bench crates/keel-cranelift/target/release/keel-cranelift-bench \
  --out target/compiler-checkpoint
python3 -B suite/linker/run.py --keel target/release/keel --out target/linker-checkpoint
```

On Linux, prefix each timing command with `taskset -c N` using an idle CPU
and use `-j 1` for the pinned suite run. The ELF runner discovers `ld.bfd`,
`ld.lld` and `mold` when installed. For runtime, divide each Keel round by
the corresponding Clang round and take the median. See the
[performance guide](../development/performance.md) for the separate
cycle-based protocol used to accept compiler changes.
