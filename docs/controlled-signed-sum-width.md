# Controlled signed i32 → i64 reduction width comparison

The existing explicit-wide path is profitable for the tested long, warm signed
sum on LinuxHost, and loses on short inputs. **Every production default remains
128 bits.** This campaign changes only a dedicated suite runner, its checks and
this document. It contains no optimizer, codegen, runtime-kernel, or selection
change, including the concurrent AVX2 add-selection work.

## Controlled provenance

The frozen compiler is main `8bb21e9a17620184bb3571ae19221ddc673f1281`.
An isolated archive was built on each host. All 1,952 tracked base blobs were
checked against their Git blob IDs, locally and on LinuxHost. The only added build
input is `suite/width-comparison/compiler.rs`, copied as
`crates/keel/examples/width_comparison.rs` in the archive. Existing toolchain,
Cargo.lock, features and release profile were retained. No sibling work was
integrated. Artifacts, snapshots and binaries are under `/tmp`, outside Git.

The example uses the CLI's C preprocessor and frontend, initial IR verification,
`DEFAULT_PIPELINE` with default verification policy, canonical restoration,
generic tuning, `SelectionOptions::ALL`, Full allocation, effective-feature
projection and chunked integrated object assembly. It retains the release
emission legality guard. The only treatment is
`capabilities.with_wide_preference(false|true)`. It neither changes feature
legality nor reparses textual IR. A final IR verification runs in both arms.
The `pair` mode runs both library compilations in one process; their objects
and optimized IR must equal the corresponding standalone library outputs.

Before any timing, **the entire 128-bit object and optimized IR must exactly
match ordinary CLI output** from the same build, input path and flags:

```sh
keel cc -O --target x86_64-unknown-linux-gnu --cpu-level v3 --emit-object kernels.c -o cli.o
keel cc -O --target x86_64-unknown-linux-gnu --cpu-level v3 --emit-ir kernels.c -o cli.ir
width_comparison x86_64-unknown-linux-gnu v3 pair kernels.c pair
```

Parity passed for strict v1, strict v2, v3 and v3 with `-avx2` on LinuxHost,
and for `aarch64-apple-darwin` locally on MacHost. Both width requests produce
identical objects in v1/v2/AVX-only/ARM. Only v3 changes. Independent function
byte comparison finds **only `rk_widen32` changes among the twelve kernels**.
These checks resolve the prior historical textual-IR provenance failure;
the old 49.214 µs versus 56.190 µs comparison is not reused as evidence.

The LinuxHost comparison compiler SHA-256 is
`324df1bbb837ef60d7900127278ae5ee5e6f3155fe334389e95eace6bed92929`;
CLI SHA-256 is
`77f473cb4004a8f8ca8c8eaec263a8e6599f17db53f2c946f211b670739d9054`.
`results.json` freezes source, runner, driver, oracle, object and executable
hashes, exact commands and outputs. `base-manifest.json` and `base-verified.json`
in the LinuxHost snapshot record the base blob verification; manifest SHA-256 is
`675484a9b955f08b9ba8d58e97f27f80ee186a281661c3b6517028035c8fd664`.

## Native behavior and memory

The unchanged maintained `runtime-kernels/kernels.c`, `driver.c` and Python
input generator are used. Before timing, both v3 executables pass the independent
wrapping signed-sum oracle at every length 0–81, 257, 4096, 65536 and 1048576.
The timed driver, input generation, single warmup call, external calls, volatile
sink and checksum workload are unchanged; one frozen driver object links both
arms, without LTO. Each sample rechecks the result before its timer.

Dedicated `guard.c` additionally checks both page boundaries, all sixteen byte
alignments, lengths 0–81, starts 0–5, signed extrema, negative elements and initial
sums 0, UINT64_MAX and 0xfedcba9876543210. `seeded.c` supplies a separate ordinary
C range/initial-sum control compiled through the same pipeline. There are 26,568
seeded checks per profile/width, plus unseeded maintained-kernel checks, reversed
and negative-bound empty ranges and a valid negative start. All eight LinuxHost
profile/width combinations and both native Apple ARM combinations pass.

The existing release `x86_widen_sum` test passes its 48 selection/allocation/pool
profiles with text/typed/chunked byte parity, operand-dependent feature checks,
native guarded ranges and address-overflow fallback. Existing
`x86_signed_wide_load` passes its six encoding/selection/export tests. Its
separately executed exported native bundle checks AVX2 and repaired AVX-only
explicit-wide lowering, aliasing, live upper halves, spills and exact accesses.
The release `x86_64::feature_guard_tests` gate also passes. The one existing
unused `REMAT` warning is unrelated to this runner.

The measured function retains the scalar SysV signature
`U(const int *, const I *, const U *, U)`; the two unused array arguments do not
become vector ABI arguments. Incoming callee-save registers are pushed/restored,
and results remain in RAX. Both arms have one unused scalar-argument stack
store/reload before the loop. The 128-bit frame reserves 32 bytes after saving
RBX/R12; the wide frame additionally saves R15, reserves 71 bytes, aligns RSP
to 32, and restores from RBP on return. Wide returns execute `vzeroupper`.
Neither loop spills accumulators or accesses the stack. The narrow finish
stores 16 bytes to its own stack and reads two 8-byte lanes; the wide finish
uses cross-half permutation, shifts/adds and extraction entirely in registers.

For eight inputs, 128 performs four exact **8-byte** PMOVSXDQ reads at offsets
0/8/16/24 and advances 32 bytes. For sixteen inputs, 256 performs four exact
**16-byte** VPMOVSXDQ reads at offsets 0/16/32/48 and advances 64 bytes. Their
guards require at least 8/16 elements and round the group count down accordingly.
Both scalar tails read exactly one 4-byte element, sign-extend and add per
iteration. Existing address-overflow guards retain scalar fallback. No floating
arithmetic, reassociation, FMA or FP-control mutation is introduced; these
integer tests do not establish unrelated FP behavior.

GNU `objdump`/`nm`/`objcopy` audit, identical in both final campaigns:

| `rk_widen32` | 128 | Explicit 256 |
| --- | ---: | ---: |
| Function bytes | 256 | 407 |
| Steady-loop inputs | 8 | 16 |
| Steady-loop instructions / bytes | 15 / 61 | 33 / 137 |
| Finish instructions / bytes | 8 / 31 | 18 / 83 |

The wide loop still has twelve add-stage vector copies and six address-building
instructions per sixteen inputs. No concurrent add-selection optimization is
included. V1/v2/AVX-only reduction disassemblies contain no AVX-family mnemonic
or YMM/ZMM operand. This scan supplements feature legality; it is not exhaustive
instruction-decoder certification. Native execution was on an AVX2-capable CPU,
not a physically strict-v1/v2 or CPUID-masked machine.

## Repeatable measurements, 2026-10-23

LinuxHost is a Ryzen 9 9950X3D, Linux x86-64. Both final campaigns use CPU 2 affinity,
the shared `/tmp/keel-perf-20261019.lock`, serialized work, one full discarded
warmup round, then nine paired rounds each. Seeds are 20261023 and 20261024;
each round shuffles arm order and runs the two arms adjacent for each length.
Repetitions are `min(1000000, max(256, 67108864 / n))`, rounded down.
Each timed process additionally performs the unchanged driver warmup.

Before compile and runtime rounds, the runner records load and a 250 ms CPU-idle
probe and aborts timing if 1-minute load exceeds 1 or selected-CPU busy fraction
exceeds 10%. Both campaigns pass: maximum recorded loads 0.516/0.859 and busy
fractions 3.85%/4.00%. The active `amd-pstate-epp` driver and `powersave` governor
are recorded, not modified. CPU frequency/boost, interrupts, SMT sibling and
physical code placement are not fixed; preflight does not prove zero interference.

Median nanoseconds per call, including driver call/sink/checksum overhead:

| Inputs | A: 128 | A: 256 | B: 128 | B: 256 |
| --- | ---: | ---: | ---: | ---: |
| 8 | 2.037 | 3.118 | 2.049 | 3.084 |
| 17 | 2.397 | 2.783 | 2.422 | 2.774 |
| 257 | 14.002 | 13.566 | 13.908 | 13.526 |
| 4096 | 222.250 | 196.104 | 219.417 | 195.894 |
| 65536 | 3478.125 | 3040.303 | 3476.367 | 3035.517 |
| 1048576 | 56244.461 | 49449.504 | 56226.449 | 49461.844 |

At 1048576 elements, median baseline/wide ratios are 1.1374x/1.1368x:
wide takes about **12.1% less time**. All eighteen paired ratios exceed 1,
ranging 1.1270–1.1622x. Both campaigns favor wide in every pair at 4096 and
65536. The small 257-element gain is noisier (one B pair is 0.9974x); both
short lengths favor 128 in every pair. These samples establish neither the exact
crossover length nor a general microarchitectural explanation.

Compile service time is measured separately over the whole twelve-function C
file, from source read/preprocessing through object assembly, including initial
verification and the unmodified Full pipeline. It excludes final verification,
IR/assembly captures, output writes and module destruction. After one discarded
warmup pair, nine interleaved samples give median 128→256 times of
2.203→2.522 ms (A, +14.5%) and 2.198→2.526 ms (B, +15.0%). Command wall times
also retain the extra diagnostic captures and must not be called compile-only
measurements. No compile-time gain is claimed.

This supports an explicit, workload-specific wide experiment for long eligible
signed sums on this host. It does not support changing the default: short
regressions, greater code/stack size and compile cost remain. It provides no
cold-cache, concurrent, other-x86-microarchitecture, whole-application, unsigned,
multistream, FP or ARM profitability claim. ARM is a correctness/parity control.

## Reproduction and artifacts

Use the new runner and its dedicated helpers without editing the maintained
runtime-kernel sources. Copy `compiler.rs` to the example location in a clean
archive of the base, then build in that archive:

```sh
python3 suite/fast_opt_locked.py /tmp/width-build.jsonl \
  cargo build --offline --locked -j2 --release -p keel \
  --bin keel --example width_comparison
python3 suite/fast_opt_locked.py /tmp/width-run.jsonl \
  python3 suite/width-comparison/run.py \
  --compiler /ABS/SNAPSHOT/target/release/examples/width_comparison \
  --keel /ABS/SNAPSHOT/target/release/keel \
  --output /tmp/width-new-campaign --cpu 2 --seed 20261023 --rounds 9
python3 suite/width-comparison/audit.py /tmp/width-new-campaign
python3 -m unittest discover -s suite/width-comparison -p 'test_*.py'
```

Run the entire campaign under the lock wrapper, never two nested lock wrappers.
Use a fresh output directory. Re-run with another seed. For native Apple ARM,
replace target with `--target aarch64-apple-darwin --controls-only`, omit `--cpu`,
and use binaries built on that host. The runner fails before timing on parity,
correctness or contention failure. Cross-target native linking is not supported.

LinuxHost snapshot: `/tmp/keel-width-controlled-8bb21e9a`.
Final artifacts: `/tmp/keel-width-controlled-linuxhost-final-{a,b}`,
including raw `samples` and `compile` rows in `results.json`, `commands.jsonl`,
objects, assembly, IR, disassembly, `audit.json` and retained binaries.
The first audit's output-less objcopy invocation rewrote object metadata after
measurement. Originals were restored from retained same-process objects,
verified against the pre-timing SHA-256 hashes; `audit-restoration.json` records
that repair. The maintained auditor now supplies a separate output object and
asserts its input hash is unchanged. Final function counts/hashes are identical.
Lock/build/test log: `/tmp/keel-width-controlled-linuxhost.jsonl`.
Supplemental native wide-load bundle: `/tmp/keel-width-controlled-wide-load`.
Local Apple artifacts: `/tmp/keel-width-controlled-apple-final`;
local lock log: `/tmp/keel-width-controlled-local.jsonl`.
The preliminary run1 predates same-process parity and is excluded from final
results. Final A/B runner/compiler hashes match. Two runner unit tests pass.
