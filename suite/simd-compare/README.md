# Small SIMD comparison corpus

This download-free companion to the application suite compares explicit Keel IR
and supported GNU C vectors. It changes no production optimization policy.
Eleven paired cases cover wrapping u32 add/multiply, signed i32 comparison masks,
signed low i32-to-i64 widening, a two-input shuffle, wrapping sum reduction,
16-byte copy, finite f32 addition, a one-source dword permutation, a two-source
low/high-pair dword shuffle and unsigned byte ordering masks. Pointer-only boundaries avoid vector ABI
confounders. C widening/shuffle/reduction use supported lane expressions; they
are source equivalents, not assertions that the frontend emits the same IR.
The two focused permutations use the supported `__builtin_shufflevector` surface;
this does not assume complete vendor intrinsic or header compatibility.
See [SIMD IR](../../docs/simd-ir.md) and
[C vector limits](../../docs/gnu-vector-mode.md). This is a small maintained
sample, not exhaustive ISA, ABI, FP exception, memory-fault or throughput coverage.

```sh
# Follow the worktree's Rust build guidance; preserve any project toolchain pin.
python3 suite/fast_opt_locked.py /tmp/keel-simd-build.jsonl \
  cargo build --release -p keel --offline --locked -j2
python3 -B -m unittest discover -s suite/simd-compare -p 'test_*.py'

# New artifact directory; default: all 4 profiles, all cases, None/Fast/Full.
python3 -B suite/simd-compare/run.py --keel target/release/keel \
  --output /tmp/keel-simd-check-01

# Optional allocation/selection comparison on baseline Apple ARM64.
python3 -B suite/simd-compare/run.py --keel /absolute/path/to/frozen/keel \
  --output /tmp/keel-simd-arm-timing-01 --profiles apple-arm64 \
  --variants full full-baseline-select full-fast-alloc \
  --benchmark --rounds 7 --repeats 100000

# On Linux x86: all profiles build, only proven supported profiles execute.
python3 -B suite/simd-compare/run.py --keel /absolute/path/to/frozen/keel \
  --clang /usr/bin/clang --output /tmp/keel-simd-x86-01 \
  --profiles v1 v2 v3 --benchmark --cpu 2
```

The public runner acquires the existing `fast_opt_locked.py` host lock for the
entire campaign. Invoke it directly; do not nest another lock wrapper. Keep the
host idle and do not rebuild compilers during a campaign. Supply existing
compilers; no downloads, remote execution, host configuration changes or automatic
compiler builds occur. `--clang` selects the reference/compiler/linker;
`--cases add copy`, `--variants full`, and `--profiles v1` narrow a rerun.
Use a fresh output directory under `/tmp` (recommended), ignored `target/`,
or `suite/reports/`.
Resolved cargo-managed target symlinks are supported. Binaries, inputs, assembly,
objects, disassembly and JSON logs/reports stay there, never in Git.

Profiles are explicit: x86-64 psABI v1/v2/v3 on Linux and ARMv8-A on Apple
ARM64. Keel receives `--cpu-level v1|v2|v3`; Clang receives the matching
`-march=x86-64[-v2|-v3]`. Apple Clang uses `-march=armv8-a`, Keel's baseline
NEON target is selected explicitly, and neither uses native tuning. Unsupported
or foreign profiles are compile-only, with runtime correctness marked skipped.
Linux execution requires the intersection of every reported processor's feature
flags. AVX requires Linux's OS-enabled AVX flag plus XSAVE and the entire v3
feature set, including BMI1/2, F16C, FMA, LZCNT and MOVBE. Linux's `avx` flag
already incorporates OSXSAVE/XCR0 enablement; `/proc/cpuinfo` does not expose
a separate `osxsave` flag. Missing evidence fails closed. Translated Apple
processes and other OS/architecture combinations cannot execute generated kernels. There is
no force-execute override. Cross-object generation is not hardware validation.

Two additional **compile-only capability probes** distinguish frontend from
backend limits: `wide.c` uses 32-byte vector arguments/results and must compile
on v3, reject the disabled AVX value/ABI support on v1/v2, and diagnose the
unsupported C width on ARM. `wide.keel` must compile only on v3 and diagnose
unsupported 256-bit values elsewhere. Expected diagnostics are checked;
unrelated errors, crashes and timeouts fail the campaign. These probes are not performance cases.
If support expands, update their contract and documentation deliberately.

`report.json` separates these evidence categories:

- **Correctness:** 64 deterministic independent Python oracle rows per case,
  including integer extremes and seeded random words. All 16 result bytes,
  exact input/output aliases and output canaries are checked. Explicit IR also
  runs unaligned input/output checks; C vector dereferences remain aligned.
  Floating inputs are small exactly representable integers; no claim about
  NaNs, signed zero, rounding modes or architectural status follows. Clang is
  another checked participant, not the oracle. A failed build/check makes the
  run fail and prevents all optional measurements. Unreached stages are marked
  not run; foreign/unproven execution is marked skipped. Partial reports survive.
- **Frontend/backend:** C-to-IR preflight evidence is saved separately. An
  accepted C program can still fail the subsequent backend pipeline. Explicit
  IR bypasses C. Clang's compilation is a combined frontend/backend boundary.
- **Instructions:** emitted assembly opcode counts and the full assembly are
  observations, never an instruction-shape correctness gate. Native objects
  have `otool`/`objdump` disassembly; `--objdump /path/to/llvm-objdump` additionally
  decodes foreign objects. Without a decoder, foreign object disassembly is
  explicitly null; assembly evidence remains available. Review the actual
  disassembly to confirm work retention before interpreting runtime changes.
- **Spills:** `confirmed_spills` is null because the CLI does not expose an
  allocator spill report. `stack_reference_candidates` retains assembly lines
  mentioning stack/frame registers. Those include saves, frames and scalar
  legalization; they must not be counted as confirmed spills. This corpus does
  not force register pressure or claim exhaustive allocator coverage.
- **Code size:** native object text bytes use the existing suite object parser;
  total object bytes are separate. No executable/debug/container size is
  substituted for text size.
- **Compilation:** only `--benchmark` populates `compile_seconds`. One warmup
  and seeded shuffled serial rounds time CLI-to-native-object compilation,
  including startup, input/output and frontend (for C). Separate assembly
  emission, capability checks, driver builds and linking are excluded. Each
  sample regenerates a sibling object without changing retained primary objects.
  Command wall durations in logs are diagnostics, not internal phase timings.
- **Runtime:** only `--benchmark` populates `runtime_seconds`. Every build and
  native correctness check precedes measurement. One warmup and seeded shuffled
  rounds use CLOCK_MONOTONIC around repeated external calls over the 64 rows,
  with a checksum addition and volatile sink per call. Inputs/oracles, setup and
  printing are untimed; timing output checks both call count and checksum. No
  LTO, payload volatile loads, time subtraction or cycle conversion is used.
  All variants use one copied/hashed executable pathname. These are warm small
  kernel **hardware measurements**, strongly affected by call/driver overhead;
  memory copy is not a bandwidth benchmark. Raw seconds are retained without
  aggregate speedup claims or CI performance thresholds. Tiny smoke runs only
  validate the harness. Repeat counts are explicit, not calibrated.

Host/CPU details, detected flags, affinity, load, available Linux governor
settings, target/flags, Clang version, Git revision/dirty state and SHA-256s of
compilers, sources, driver and primary artifacts are retained. Binary identity
checks bracket timing and every round. Keel build features are not introspectable;
retain the build command/toolchain/features alongside a frozen binary when
comparing revisions. Frequency, thermal state, ASLR and code page placement are
observed only in part and remain uncontrolled. Missing observations are not
proof of identical run conditions. `modeled_estimates` is explicitly null:
this runner does not run llvm-mca or mix model predictions with native samples.
Existing pattern-lab model experiments remain separate.

The focused selection cases are deliberately compact:

- `permute-dword`: `[a2,a0,a3,a1]` retains every input lane and exercises
  one-source packed dword selection, including C's repeated-source form.
- `shuffle-pairs`: `[a1,a0,b3,b2]` exercises the low pair from one source and
  high pair from the other. The original `shuffle` still covers a different,
  interleaved mask that requires the general path.
- `compare-u8`: unsigned byte `<` returns all-ones/zero bytes. Every lane sees
  0/255, 127/128 in both orders and equality at all four boundary values,
  followed by seeded random bytes. This targets the sign-bias recipe; it does
  not claim coverage of every predicate or all 65,536 operand pairs.

These cases use the existing scalar Python oracles and native driver, including
all output bytes, exact aliases, canaries and the unaligned explicit-IR path.
Pointer-only calls, exact memory widths and scalar evaluation rules are retained;
no FP arithmetic or environment contract is added. Selection documentation in
[x86 SIMD selection](../../docs/x86-simd-selection.md) describes broader semantic
and encoding evidence. Inspect native objects for `pshufd`, `shufps` and
`packsswb` when evaluating the improvements; those observations are not ordinary
correctness assertions. This sample does not cover arbitrary masks, register
pressure, exhaustive signedness combinations or whole-application performance.

The prior frozen `74ecf5db` → `3f5130a2` comparison of the original eight cases
on AMD 9950X3D and Apple M5 Max emitted identical text (old/new runtime ratios
0.995–1.002x). The maintained longer positive32/adjacent/widen32 loops likewise
emitted identical Keel code. That null result remains valid: these added cases
address a coverage gap, not evidence that the earlier workloads became faster.
For a small follow-up, reuse verified frozen binaries and the identical current
corpus with `--cases permute-dword shuffle-pairs compare-u8 --variants full
--benchmark --rounds 7 --repeats 1000000`; preserve host, flags and driver path.
Inspect code alongside samples and report call-overhead sensitivity rather than
inferring an application speedup from compact kernels.

A bounded follow-up on 2026-10-23 reused those frozen release/default-feature
binaries after verifying their recorded SHA-256s. It ran only the three new
cases, Full selection/allocation, one warmup and seven measured rounds of
64,000,000 calls, with identical corpus/driver sources and flags within each
host. LinuxHost used CPU 2. Old and new were separate serial campaigns, so drift
between revisions remains uncontrolled. Both C and explicit IR passed checks.

On LinuxHost, at every v1/v2/v3 level, `permute-dword` text shrank 84 → 38 bytes
and `shuffle-pairs` 90 → 54 bytes. Native disassembly replaced stack-based
scalar lane movement with `pshufd 0x72` and `shufps 0xb1`, respectively.
Old/new median runtime ratios across C/IR were 4.054–4.065x and 4.194–4.205x
on v1/v2; v3 measured 3.360x and 3.444–3.463x. These are observations of this
call/checksum driver on AMD 9950X3D, not isolated instruction throughput or
application speedups. `compare-u8` text shrank 90 → 73 bytes and its six-step
bias construction became `pcmpeqd; psllw 7; packsswb`, but runtime ratios
0.992–1.005x were inconclusive. Unchanged Clang controls ranged 0.995–1.004x.
CLI-to-object compile ratios for Keel ranged 0.998–1.015x; startup/I/O are
included and no compile-time improvement is established.

On Apple M5 Max, all old/new text was identical. Keel runtime ratios ranged
0.972–1.167x while unchanged Clang controls shifted 1.075–1.101x; this run
supports no ARM runtime improvement. Compile timings also shifted with unchanged
code and are not attributed to selection. Raw objects, disassembly, all samples,
commands and hashes for this finite follow-up were retained outside Git under
`/tmp/keel-corpus-20261023/` on both hosts (the combined comparison is on MacHost).
No compiler rebuild occurred during measurements and no benchmark threshold
was added to CI.

Normal unit tests have no timing thresholds or benchmark campaigns. They check
CPU gating, hand-computed oracle edges, diagnostic contracts, honest spill
labels, and an existing Clang driver against both valid and corrupted results.
No compiler optimization settings are adjusted based on these tests.
