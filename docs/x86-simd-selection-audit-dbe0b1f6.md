# Next bounded SIMD selection pilots at dbe0b1f6

Read-only production audit, 2026-10-23. Source base is
`dbe0b1f65f8ed0a18d84e27fe8644529a0d64810`. This document owns only the audit;
none of the proposed selectors is implemented. The completed PMOVSXDQ m64/m128
covers, PMULLD retention, AVX2 half-local dword shuffle, direct integer adds,
constant-vector address fold, wrapping reductions and C shift/saturation/FP
conversion work are retained controls, not recommendations to repeat.

## Evidence and scope

A clean `git archive` of the base was built under
`/tmp/keel-simd-audit-dbe0b1f6/source` on MacHost. The only added source in that
snapshot was an unchanged copy of maintained
`suite/width-comparison/compiler.rs` at
`crates/keel/examples/width_comparison.rs`. No other worktree was changed.
The exact build was:

```sh
python3 suite/fast_opt_locked.py \
  /tmp/keel-simd-audit-dbe0b1f6/build.jsonl \
  cargo build --offline --locked -j2 -p keel \
  --bin keel --example width_comparison
```

The frozen debug CLI SHA-256 is
`651f59d0dce213c206e0eada221806381cfc85d268a9a479af4586066d2d4aa8`;
the frozen width-driver SHA-256 is
`4d7567e6ae9de1f9d64748710f47f417b2ad0249229a42f56637c171a0febaa5`.
Existing toolchain/dependencies, cargo's default cache and managed target were
preserved. These are compiler identities, not optimized compiler-speed samples.

The maintained `suite/simd-compare` corpus was also run with `--variants full`
and all four profiles, without `--benchmark`. All 132 build rows completed;
33 ARM-native correctness rows passed, while 99 foreign x86 rows correctly
skipped execution. All eight width-capability probes passed. Runtime and
compile sample arrays are empty. Its maintained `widen.keel` confirms the same
five-instruction low-widen recipe after a full 16-byte user load at v2/v3.
Its `widen.c` equivalent instead forms scalar extracts/inserts and scratch
memory; improving the backend conversion will not automatically recognize that
frontend expression. This distinction limits C applicability claims.

The width driver compiles unchanged `suite/runtime-kernels/kernels.c`, verifies
IR, runs the maintained default pipeline with explicit target capabilities,
then saves optimized IR, assembly and real objects. Captures cover x86 v1/v2/v3
and v3 with AVX2 disabled, each with 128/256 preference, plus baseline Darwin
ARM64 at 128. CLI probes use Full optimization and the default allocator;
IR conversion probes use the default compile pipeline. Every execution is
serialized through `suite/fast_opt_locked.py`. No timed campaign was run.
The LinuxHost benchmark worker retained ownership of timing and the shared lock.

Apple LLVM objdump 21 independently decoded the emitted objects. ELF symbol
sizes bound counts and instruction-byte hashes, excluding alignment padding.
All twelve default-width kernel bodies are byte-identical at v2 and v3.
Only `rk_widen32` differs between v1/v2, or v3 128/256 preference. Its counts
are respectively 104/379, 76/257 and 84/292 instructions/bytes. This is an
observation about emitted code, not a width profitability verdict.
`rk_balance32` is 163 instructions/610 bytes and `rk_clamped` 158/602 at all
these profiles. Their remaining register widens are the first pilot below.
None of these twelve runtime kernels contains `swiden_high`, `vshuffle` or
vector subtraction.
Consequently the other two pilots have focused C applicability evidence, not
an observed occurrence in this twelve-kernel corpus.

## 1. SSE4.1 signed low dword-to-qword register conversion

**First pilot:** exactly `swiden_low.i64x2` from `i32x4`, using an XMM register
source when SSE4.1 is selected. Owner:
`crates/keel-codegen/src/x86_64/vector.rs::vector_convert` (line 473 at base),
with retention in `lower` (line 248). This is downstream of existing load/DAG
covers; those covers keep priority and their exact memory contracts.

Observed before, after a vector select in `rk_balance32` (four occurrences)
and `rk_clamped` (two):

```asm
movaps   xmm13, source
xorps    xmm14, xmm14
pcmpgtd  xmm14, xmm13
punpckldq xmm13, xmm14
movaps   destination, xmm13
```

Possible after: `pmovsxdq destination, source`, using existing
`V2Opcode::Pmovsxdq`. Five register instructions become one for each captured
normal allocation: a prospective 16-instruction reduction in `balance32` and
8 in `clamped`. This is local recipe arithmetic; changed frame layout,
allocation or later selection must be recounted from a candidate object.
It does not count the already folded loads in `rk_widen32` as opportunities.

Eligibility is exactly the stated signed source/result pair plus SSE4.1.
Keep the existing SSE2 path at v1 or with SSE4.1 disabled; retain existing
paths for unsigned widening, other widths, high widening, FP conversions,
64-bit vectors and any 256-bit operand/result. Resolve source aliases, reload
a spilled source at its existing **16-byte vector home width**, then define
the result and complete its existing 16-byte spill store. The register
instruction consumes the low two dwords and defines all 128 result bits;
destination/source alias is legal after the source has been captured.
Do not introduce its m64 form in this pilot, fold a user load, or narrow a
16-byte spill reload merely because the opcode also supports m64.

Practical C already reaches this operation through ordinary scalar-loop
vectorization: signed selected i32 values contribute to wrapping i64 totals.
Numeric vector cast builtins are unnecessary. This does not extend the three
architectural SSE2 **FP** conversion wrappers or their MXCSR contract.
On ARM the analogous low conversion retains native `sshll`; no ARM change
is indicated.

Acceptance gates: extend `vector_widen_x86` with explicit FeatureSelector
v1/v2/v3 and disabled-SSE4.1 controls, exact source/result shape negatives,
all three selectors, both allocators, controlled source/result aliases and
independent/all spills. Preserve existing widen-load tests, text/typed/chunked
parity and debug/release emission guards. Native scalar signed-extension
oracles must include INT32_MIN/MAX, all bit positions, deterministic random
values, live source capture and call pressure; pointer wrappers need unchanged
16-byte load/store widths, offsets, aliases, canaries and both protected-page
edges. Test spills by instruction windows as well as returned values.

## 2. AVX2 same-source qword permutation

**First pilot:** generic `i64x4` same-resolved-source shuffles, with four
constant indices normalized to 0..3. Owner:
`crates/keel-codegen/src/x86_64/avx256.rs::wshuffle` (line 323), alongside its
existing dword classifier. Do not broaden the completed half-local dword rule.

The C probe `return __builtin_shufflevector(a,a,3,2,1,0)` produces this generic
shuffle and reaches the general byte-mask path. Its actual reverse recipe is
26 instructions: two input stages, zero accumulator, two half broadcasts,
sixteen mask-materialization instructions (eight qword stores), two
`vpshufb`, two ORs, and result publication. The full C function is 41
instructions/207 bytes, including frontend parameter storage and frame/ABI work.

Possible after, retaining current staging/publication policy:
`wstage(source,FS1); vpermq FS0,FS1,27; wdone(result)`.
That is three register instructions for the captured normal allocation,
a prospective local saving of 23 instructions, eight mask stores and two
mask reads. Keep frontend parameter storage and all application memory accesses.
Do not claim the whole frame disappears. Existing `V3Opcode::Vpermq` already
encodes the YMM-register/immediate form and requires AVX2.

Restrict the first pilot to integer qwords, equal resolved sources and valid
four-lane indices; repeats and either concatenated-source spelling may be
normalized only after equality is proved. Keep fallback for distinct sources,
other lane widths, malformed masks and AVX without AVX2. V1/v2 continue rejecting
wide SSA/ABI rather than silently upgrading. Read/reload the full 32-byte
source before defining FS0; reserved FS1 protects live/aliased sources and
existing `wdone` handles the 32-byte result spill. No FP arithmetic, MXCSR
operation, application load fold or new ABI policy is needed.

Practical C includes the supported `__builtin_shufflevector` and
`_mm256_permute4x64_epi64` in `crates/keel-c/include/avx2intrin.h:137`.
The ordinary-C reverse was actually compiled; wrapper availability is a
maintained frontend fact, not a claim that every wrapper optimization was
independently inspected. ARM has no supported 256-bit C/SSA equivalent;
its 128-bit window control emits `ext #8` instead of general lane staging.

Acceptance gates: all 256 qword controls and equal-source spellings; boundaries
that change one index/source/width; same SSA, resolved aliases and live inputs;
three selectors, both allocators and controlled/empty spill pools. Reuse
`x86_avx2_shuffle_selection` and generic-wide oracle conventions, with exact
immediate assertions and Clang/GNU encoding comparisons for all registers.
Require v1/v2 rejection, AVX-only fallback byte identity, three object paths,
release legality, raw-bit scalar shuffle oracles, ABI calls, unaligned 32-byte
memory, output/input aliases, canaries and both guard edges. Adding FP shapes
would require separate raw NaN/zero/MXCSR tests and is outside this first pilot.

## 3. AVX2 direct-destination dword subtraction

**First pilot:** `visub` on `i32x8` only. Owner:
`crates/keel-codegen/src/x86_64/avx256.rs::avx256`, vector-binary branch at
lines 421–460. The completed direct-add branch remains its unchanged control.
The focused GNU-vector C probe `V8 sub8(V8 a,V8 b) { return a-b; }` emits:

```asm
vmovups ymm13, left
vmovups ymm14, right
vpsubd  ymm13, ymm13, ymm14
vmovups destination, ymm13
```

Possible after: resolve/reload left with FS0 and right with FS1, define the
result, emit `vpsubd destination,left,right`, then `def_done`.
Four instructions become one for register operands/result. The current whole
C function has 24 instructions/85 bytes; its parameter-memory traffic is
separate and remains. The inspected `i64x4` probe has the same staging pattern,
but expanding to qword/byte/word subtraction should be a later bounded step.

Require AVX2 and exactly i32x8 on both sources/result. Preserve operand order:
subtraction is not commutative. Destination may alias either input only after
both reads, with distinct reserved reload scratches; two spilled inputs plus
a spilled FS0 result must still read both original homes before writing.
Retain full-width reload/spill completion and the current 32-byte SysV ABI,
including shared XMM/YMM allocation exclusions. No source load is folded or
moved. AVX-only keeps its existing legal fallback, v1/v2 keep wide rejection,
and other integer/FP operations remain untouched. All arithmetic is modular;
there is no FP arithmetic or exception-status transformation.

Practical C includes ordinary 32-byte integer vector subtraction and existing
`_mm256_sub_epi32` at `crates/keel-c/include/avx2intrin.h:50`. ARM's supported
128-bit integer operations already use native arithmetic; it does not justify
introducing 256-bit ARM values.

Acceptance gates: mirror `x86_avx2_direct_add` for the one subtraction shape,
including text/typed/chunked parity, typed operand-feature legality,
debug/release guards, all selectors/allocators/pool restrictions, exact
controlled register windows, either destination alias, live operands across
`vzeroall`, independent spills and all-spilled source order. Native unsigned
scalar subtraction must include equal operands, zero/min/max, borrow/wrap and
random values. Preserve pointer ABI, separate AVX register-ABI checks, exact
32-byte user loads/stores, offsets 0..31, guard edges, canaries and MXCSR.

## Inspected exclusions and interpretation

`swiden_high.i64x2` from i32x4 does scalarize at all three x86 levels: the probe
is 21 instructions/65 bytes with six scratch-memory instructions. ARM emits
`sshll2` for this exact operation. This is a real lowering gap, but no maintained
kernel emits high widening and the C frontend has no general numeric vector
conversion builtin. It ranks below the low-register conversion with six real
kernel occurrences. A two-source i64x4 window `[1,2,3,4]` also reaches general
mask staging (57 instructions/298 bytes whole function); it is excluded from
the single-source permutation pilot because cross-source half construction and
alignment need a separate proof. Do not assume 256-bit PALIGNR crosses halves.

The useful release target remains explicit v3, with v1/v2 controls. The largest
maintained residual identified here is also addressable at v2 via SSE4.1;
merely choosing v3 currently does not select it. The two wide pilots need AVX2
already in v3. Nothing observed here motivates optional AES/PCLMUL, CLFSH,
AVX-512 or another optional extension. Frontend recognition, optimizer formation,
backend selection, assembler encoding and native execution are distinct:
recognizing a C wrapper is not evidence of compact instruction selection.

All artifacts are outside Git under `/tmp/keel-simd-audit-dbe0b1f6` on MacHost
and LinuxHost. `commands.json`, `hashes.json`, `decoded-counts.json`, `out/*.ir`,
`out/*.s`, `out/*.o`, `out/*.local.dis` and the build/static/feature lock logs
retain exact commands, inputs and evidence. `feature-checks.json` records
assembler acceptance of PMOVSXDQ at v2/v3 but rejection at v1, and acceptance
of VPERMQ/VPSUBD only with AVX2. This encoding-level check supplements, and
does not replace, the proposed ordinary-codegen release guards.

## Finite native and encoding checks

LinuxHost Clang 23.1.1 and GCC 16.2.0 built independent unsigned/scalar references
for the current C subtraction/reverse/window objects and signed low/high IR
conversion objects. Each compiler's harness passed 4,096 deterministic random
cases per function with unchanged MXCSR. Exact local/exported object hashes
were compared. The native command waited 177.7 seconds for the benchmark lock;
it did not run alongside the timed worker.

Separate handwritten register recipes, outside Git, checked PMOVSXDQ XMM0/XMM0,
VPERMQ YMM0/YMM0/27, VPSUBD YMM0/YMM0/YMM1 and the inspected VPSUBQ analogue.
Keel and Clang assemblers produced identical decoded instruction bytes. Each
assembler's recipe objects passed the same 4,096-case native comparison with
both Clang and GCC harnesses (four combinations). These establish finite
instruction value/encoding/ordinary register-ABI behavior on this AVX2 host.
They are not implemented selectors, controlled-allocation tests, guarded-page
proofs, release-codegen checks, execution on feature-restricted hardware, or
an exhaustive FP/exception contract. The production acceptance gates above
remain required. No benchmark or compiler-throughput conclusion follows.

Native evidence is `native-results.json`, `native-object-hashes.json`,
`recipe-results.json`, `recipes-{keel,clang}.dis` and native lock logs under the
audit directory on both hosts. The maintained corpus report is
`maintained/report.json` on MacHost. Its source revision is the requested base;
its dirty marker records this new audit document only. The compiler itself
came from the clean frozen archive. The only committed change is this document.

## Reproduction outline

After the frozen build above, copy both binaries from its managed `target/debug`
into a task-owned `/tmp` directory and hash them. Run the width driver once per
profile/width under the shared wrapper; its exact argument order is
`TARGET PROFILE WIDTH INPUT OUTPUT_PREFIX`. Profiles are `v1`, `v2`, `v3`,
`avx` (v3 minus AVX2), and `arm`; ARM uses
`aarch64-apple-darwin`, x86 uses `x86_64-unknown-linux-gnu`.
Keep each output prefix unique. Do not invoke the timing runner.

The focused source operations are the exact `V8 sub8` and qword reverse
expressions above, plus `__builtin_shufflevector(a,b,1,2,3,4)` for i64x4 and
`__builtin_shufflevector(a,b,1,2)` for i64x2. Define the corresponding GNU
vector typedefs at 32/16 bytes and compile with `cc -O --target ...
--cpu-level v3`, separately to text and `--emit-object`. Wide probes must
continue rejecting at v1/v2 and on ARM. The conversion-only IR is:

```text
func @low(i32x4) -> i64x2 {
entry(v0:i32x4):
 v1 = swiden_low.i64x2 v0
 return v1
}
```

Replace `low`/`swiden_low` with `high`/`swiden_high` for the inspected exclusion.
Use `compile --target ... --cpu-level LEVEL`, separately to text and object.
Compare actual objects with independent objdump, bounded by ELF function symbol
sizes. For the maintained companion corpus, invoke its public runner directly
(it owns the lock):

```sh
python3 -B suite/simd-compare/run.py --keel /tmp/OWNED/bin/keel \
  --output /tmp/OWNED/maintained --variants full \
  --profiles v1 v2 v3 apple-arm64 --objdump /usr/bin/objdump
```

Do not add another lock around that public runner. Native oracle execution and
all other compiler commands use the shared wrapper directly, after waiting for
the benchmark worker. Retain logs and generated inputs/outputs outside Git.
