# AVX2 direct-destination packed integer adds

This bounded emission pilot starts from main
`8bb21e9a17620184bb3571ae19221ddc673f1281`. Generic 256-bit integer `viadd`
now emits `vpaddb/w/d/q destination, left, right` directly into the allocated
result, instead of staging both operands and publishing a scratch result.
All four integer lane widths share this one path. There is no change to the
optimizer, optimization context, default width, address folding, ABI, or FP
selection. Subtraction and other wide operations retain their existing recipes.

The safety argument depends on the existing allocation and emission contracts:

- Wide functions exclude SIMD indices 12–15 from allocation, including their
  XMM views. `use_freg` resolves SSA aliases; an allocated operand is read in
  place, while a spill is reloaded only into its requested scratch register.
- The left and right operands use distinct reload scratches FS0 and FS1, in
  their existing order, before the result is written. A spilled result uses
  FS0 only after both operands have been captured. `def_done` completes its
  existing exact-width spill store. No operand is folded into an add's memory
  operand and no application load/store is moved or widened.
- The VEX integer instructions read both complete YMM sources before writing
  the destination, so either source may share the destination when its SSA
  lifetime permits. Live sources retain separate allocations. Register views
  remain 256 bits; no XMM write can discard an upper half. All lane arithmetic
  wraps, and there is no FP operand-order or NaN question in this pilot.
- The existing `keep` gate requires AVX2. Strict v1/v2 still reject wide SSA
  transport, AVX without AVX2 retains scalar/narrow lowering, unsupported
  forms retain existing diagnostics, and the ordinary release emission guard
  still checks operand-dependent feature requirements.

`x86_avx2_direct_add` checks text/typed/chunked object parity and instruction
feature legality without optimizing its dedicated arithmetic corpus. Its
16 functions cover four integer widths, distinct/same/resolved-alias operands,
inputs live after the add, and both inputs live across an external function
that executes `vzeroall`. Native C uses unsigned scalar lane addition and
`memcpy`, independently of compiler vector arithmetic. It checks deterministic
patterns and overflow edges, all byte offsets 0–31, output aliases to either
input, adjacent canaries, both protected-page boundaries, and unchanged MXCSR.
Inputs are exactly 32 bytes; the output captures result and original inputs in
three separate 32-byte stores.

The matrix uses all three selectors, both allocators, and normal, one-SIMD,
empty-SIMD and completely empty register pools. Seven controlled layouts per
selector capture inputs before register reuse and force either source/result
alias, independent left/right/result spills, both spilled sources with a
register result, and all spills. The allocator locates IR definitions rather
than assuming textual SSA numbers survive capture movement; each exported
function asserts the actual three add registers. Strict v1/v2 reject on all
three output paths. A register-ABI case asserts that neither reload scratch
appears in the direct add. AVX-only normal/spilled text and objects are
byte-identical to base.

Local debug and release dedicated tests, existing `x86_avx256_generic`,
`x86_wide_conversions`, `x86_widen_sum`, and debug/release emission-guard checks
pass. Clippy passes with existing repository warnings and no warning in the
new test. LinuxHost native Linux x86-64 execution passes 47 dedicated exported
oracles and 36 existing generic, wide SysV ABI and conversion oracles. The
existing ABI harness requires `-mavx2`; its native runner also links `-lm` and
allows its existing unused C parameters. The dedicated pointer-ABI oracle uses
`-march=x86-64 -mno-avx` for its C harness, with explicit inline `vzeroall`.
The signed-sum guarded-input scalar oracle passes separately at both revisions.
These are finite native tests on AVX2-capable hardware, not v1/v2-only execution
or an exhaustive proof. No C frontend surface or FP semantics are extended.

The ignored `measure_signed_sum_direct_add` test uses the maintained
`signed_widen_sum.ir` fixture and explicitly requests wide optimization in its
test context. Base and candidate optimized IR have identical SHA-256
`468d0615f2dee7cedc7ec6880a2a43ddc9a7b5e7ec4149a621476f01eb7631d1`.
Both use Full selection/allocation, normal pools, and the same i64x4 loop.
This isolates emission from width selection. Static results include the whole
function: 123 → 102 emitted instructions, 444 → 350 `.text` bytes, and
26 → 5 `vmovups` instructions. The four-add loop loses all 12 staging copies;
three final accumulator adds lose nine more. All nine `vpaddq` instructions
remain (including two unchanged reduction instructions). All ten emitted
memory-address instructions are textually identical in their original order.
GNU disassembly of both ELF objects is retained with the native evidence.

Native kernel timing is a separate fixed-width experiment on LinuxHost, pinned to
CPU 4 under the shared investigation lock. A single C executable links renamed
base/candidate objects, first checks each against an unsigned wrapping oracle,
then alternates arm order. Each arm processes 268,435,456 integers per sample;
12 measured rounds follow one warmup round. Median nanoseconds per call at
lengths 16, 64, 256, 4,096, 262,144 and 1,048,576 are respectively:

- Base: 4.046, 6.225, 13.997, 203.105, 12,713.045, 50,808.527.
- Candidate: 3.483, 4.613, 9.408, 117.093, 7,687.707, 31,337.527.

The recorded load was 0.52–0.56 on 32 logical CPUs, with no competing build
visible in the before/after process captures. These are bounded kernel timings
including function overhead; they do not compare optimizer widths or establish
application speedups. An earlier short timing run was superseded by this run.

Reproduce the dedicated cross/native test and the separate static/compiler
measurement using cargo's shared cache and managed per-checkout targets:

```sh
KEEL_AVX2_ADD_EVIDENCE=/tmp/keel-direct-add-candidate \
python3 suite/fast_opt_locked.py \
  /tmp/keel-direct-add-build.jsonl cargo test \
  --offline --locked -j2 -p keel-codegen --release \
  --test x86_avx2_direct_add

KEEL_AVX2_ADD_EVIDENCE=/tmp/keel-direct-add-candidate \
python3 suite/fast_opt_locked.py \
  /tmp/keel-direct-add-measure.jsonl cargo test \
  --offline --locked -j2 -p keel-codegen --release \
  --test x86_avx2_direct_add measure_signed_sum_direct_add \
  -- --ignored --nocapture
```

For base, archive the pinned commit into a disposable `/tmp` source directory
and copy only the new test into it. Run its ignored measurement unchanged.
Exported objects, assembly, optimized IR and CSVs are under
`/tmp/keel-direct-add-{base,candidate}` locally. The native pointer-ABI runner
is the existing `crates/keel-codegen/tests/x86_widen/run_native.py`, serialized
through `suite/fast_opt_locked.py`. LinuxHost artifacts, exact native commands,
C drivers, raw timings, process/load captures, GNU disassembly and SHA-256
reports are under `/tmp/keel-direct-add-native`; local captures are under
`/tmp/keel-direct-add-native-results`. The dedicated native report's SHA-256 is
`0edb5caa5076a69244efbcb8d72c0f413170a79cca025d13b1b4b4bfb683137b`;
the existing-family report's is
`1340af5991eb973c8cacd6c26ebbebfd335b7682f168ab8fda7b12780e2f1948`.
All generated evidence stays outside Git.

Compiler cost is measured separately in optimized test binaries on LinuxHost,
using the same pre-parsed, already optimized module, 100 samples after three
warmups. Median microseconds for base → candidate are 83.167 → 55.916 for text
codegen, 80.796 → 53.731 for typed codegen, and 180.673 → 115.297 for typed
object assembly/ELF generation. Parsing, IR verification/optimization before
codegen, C frontend work, Rust builds, process startup and native execution
are outside these stage samples. Base and candidate each regenerate exactly
the locally exported object bytes. These are small single-module observations,
not a compiler throughput claim. The shared lock delayed the base run by
77 seconds behind sibling work; the recorded host load during the measured
runs was 1.18–1.31 on 32 logical CPUs. Local measurements at load 11.3 were
discarded. Raw LinuxHost stage samples and lock metadata are in
`compile-{base,candidate}/measure/compile-ns.csv` and `compile-lock.jsonl`
beneath the native evidence directory. Rust build wall times depend on cache
state and are not compared.
