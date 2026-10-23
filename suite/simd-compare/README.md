# Small SIMD comparison corpus

This download-free companion to the application suite compares explicit Keel IR
and supported GNU C vectors. It changes no production optimization policy.
Eight paired cases cover wrapping u32 add/multiply, signed i32 comparison masks,
signed low i32-to-i64 widening, a two-input shuffle, wrapping sum reduction,
16-byte copy and finite f32 addition. Pointer-only boundaries avoid vector ABI
confounders. C widening/shuffle/reduction use supported lane expressions; they
are source equivalents, not assertions that the frontend emits the same IR.
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

Normal unit tests have no timing thresholds or benchmark campaigns. They check
CPU gating, hand-computed oracle edges, diagnostic contracts, honest spill
labels, and an existing Clang driver against both valid and corrupted results.
No compiler optimization settings are adjusted based on these tests.
