# Next C SIMD recognition audit

Read-only production investigation at clean main
`d7d89d7daf6dfd8778869f4aa5487707deaa229e`, in the isolated
`research/c-shuffle-recognition` worktree. This document is the only Git change.
No production matcher, frontend builtin, header, pipeline or backend change is
proposed as implemented. The wrapping i32x4 sum (`68d55bbc`) and SSE4.1 register
widening (`d0aec154`) are already integrated and are not new recommendations.

## One next match: private low signed widening construction

Prefer a bounded `stack_forward.rs` extension for the existing measured
`suite/simd-compare/widen.c`, replacing its private vector reload with
`swiden_low.i64x2` of the already evaluated i32x4 source. This closes its
C-to-explicit-IR code-shape gap on all four inspected profiles. It does not
establish a runtime speedup or close the remaining explicit-IR-to-Clang gap.

The [frozen research](current-compiler-research-dbe0b1f6.md) measured substantial
C widening and shuffle gaps; [the reduction pilot](c-lane-reduction-pilot.md)
left construction traffic unresolved. Fresh frontend and Full IR confirm that
neither compound literal becomes an insert chain. `init.rs::sub_slots` assigns
lane byte offsets; `irgen.rs::init_local` zeroes private storage and evaluates
and stores each initializer. Constant lane reads use `vextract`; lane lvalues
use byte-addressed stores. Ordinary stack forwarding removes input homes but
cannot reconstruct the final vector reload from scalar stores:

```text
v4 = load.i32x4 input
... private zero initialization ...
v12 = vextract v4, 0
v13 = sext.i64 v12
store align 16 v13, private
v16 = vextract v4, 1
v17 = sext.i64 v16
store align 8 v17, private_plus_8
v21 = load.i64x2 align 16 private
store v21, output
```

The proposed replacement is only `v21 = swiden_low.i64x2 v4` at the original
reload position. Existing forwarding cleanup can remove private stores once
all reads disappear, and DCE can remove dead pure producers. Preserve source
location. Do not fold the input load into a narrower memory-source instruction.

`stack_forward.rs` already owns resolved stack addresses, constant in-bounds
offset closure, whole-slot escape/debug/frame-observer exclusion, same-block
last-writer byte facts, lifetime barriers and removal of stores only after all
reads disappear. Its reconstruction helper currently accepts i8/i16/i32/i64
loads; exact vector-store/vector-load forwarding already works. Reuse those
facts rather than add separate alias analysis to instcombine.
`stack_promote.rs` accepts only scalar integer/pointer fields, rejects vector
accesses and overlapping field shapes, and proves definite initialization;
simply enabling vector promotion does not solve mixed scalar-store/vector-load
construction. Its broader CFG promotion is unnecessary for this pilot.

## Required eligibility and correctness argument

These are implementation gates, not claims that an automatic matcher was
tested here:

- Admit only an exact nonvolatile i64x2 reload of a contained private 16-byte
  slot at offset zero, using existing whole-function address-use closure.
  Start with one block, ordinary fixed lifetime, no debug home, escaping or
  aliased slot address, frame observer, returns-twice, stack mark/restore,
  atomic or unsupported address use. No cross-block reconstruction or new
  dominance inference. Ordinary calls cannot touch a proven nonescaping slot;
  they must retain identity, operands and order.
- At the reload, require all sixteen byte facts: bytes 0..7 are the ordered
  bytes of one exact i64 store value, and bytes 8..15 of another. Each value
  must resolve to `sext.i64(vextract source, lane)` with an i32 scalar and the
  same resolved, layout-owned i32x4 source, lanes exactly 0 then 1. Resolve
  every operand before comparison. Reject torn/overlapping final pieces,
  incomplete coverage, alternate lane widths, casts or operations beyond this
  exact pair, high/reversed/repeated lanes, mixed sources and zero extension.
  Earlier overwritten zero stores do not matter; later partial stores must
  invalidate eligibility. Verified SSA supplies dominance; do not walk through
  aliases to instructions outside the layout.
- The resulting two lanes equal signed extension of source lanes 0 and 1,
  byte for byte. IR vectors have no undef/poison lanes. A partially initialized
  C compound literal actually has zero-filled remaining lanes; reject it
  rather than replace those zeros with a source lane. General uninitialized
  stack bytes give no complete proof. No undef seed or invented lane is needed.
  Byte offsets must use the supported little-endian lane layout; do not infer
  an unvalidated big-endian memory representation contract.
- C casts/promotions have already resolved to scalar IR. LP64 `long` is i64;
  `(long)((S)a)[i]` yields signed i32 extension. `(long)((U)a)[i]` yields uext,
  and signed-short lane widening has i16 input, so both fail this gate. Numeric
  FP conversion, FP arithmetic and environment semantics remain untouched.
  This is not recognition from source spelling or signedness guessed from
  the integer vector shape.
- Reuse evaluated SSA values, never re-evaluate an initializer or a source
  expression. Keep both external input effects and output publication in order;
  move no external load, store or call. The input remains exactly sixteen
  bytes even though only its low eight value bytes contribute. Output remains
  exactly sixteen bytes. This forbids an m64 PMOVSXDQ load cover here, including
  when high source bytes would fault. Removing proven unobservable private
  accesses uses the existing stack pass's abstraction, not a promise to
  preserve stack-exhaustion faults or debugger edits.
- Source vectors and replacement reload results may have multiple users:
  neither is overwritten or duplicated. Shared scalar extracts/extensions are
  also semantically safe but can eliminate less work. For the initial measured
  payoff gate, require each extract and extension to have only its chain/store
  consumer. `stack-forward` has no instcombine `Uses` table: budget one lazy
  function-wide resolved-use scan after the first structural candidate, or
  explicitly accept shared producers without claiming their removal. Never
  infer single use from adjacent layout. Preserve all other slot reads/stores;
  existing `remaining_loads` controls store deletion.

Keep work bounded: use the existing address/escape scan and sixteen byte
lookups, inspect exactly two extension/extract pairs, with no recursive
producer walk. An initial admission cap of 8,192 layout instructions and
16,384 values can bound a new lazy use scan; reject larger functions. Existing
stack-forward address closure is not constant-time and the Full pipeline still
runs three bounded rounds, followed by its epilogue (`keel-opt/src/lib.rs`).
Do not claim constant total work or change scheduling. Replacing a load with a
conversion is one-way; test the helper and Full pipeline at a fixed point.

## Bounded re-probes and decoded objects

A frozen release/default-feature compiler was built from the clean base using
the existing pin and shared cargo cache, with link caching disabled:

```sh
python3 suite/fast_opt_locked.py \
  /tmp/keel-next-c-d7d89d7d/build.jsonl \
  cargo build --offline --locked -j2 -p keel --release
```

Compiler SHA-256:
`54b9a00678360da9b5027aa5e6c3c1ac7a61607a7654d128ff21d5bcd72ba7ec`.
All later compiler commands use that copied binary. Original maintained C and
IR are unchanged. For a counterfactual only, `probe.py` replaces the single
Full-IR reload with the explicit conversion (or shuffle), then runs the existing
Full optimizer and `--verify-each`. This is a hand rewrite of two exact examples,
not a new production pass or measured automatic matching result.
Both hand examples are Full fixed points modulo parser SSA renumbering.
All 24 LinuxHost-executed object hashes match their locally decoded counterparts.

Independent LLVM objdump decoding of one-function objects gives whole-function
instruction counts / text bytes, including prologues and epilogues:

- Widen v1: C **23 / 93**, hand rewrite **14 / 55**, maintained IR **14 / 55**.
- Widen v2 and v3: C **23 / 93**, hand **10 / 38**, IR **10 / 38**.
- Widen baseline Apple ARM64: C **22 / 88**, hand **12 / 48**, IR **12 / 48**.
- Shuffle every x86 level: C **31 / 126**, hand **24 / 92**, IR **24 / 92**.
- Shuffle ARM64: C **25 / 100**, hand **17 / 68**, IR **17 / 68**.
- Already explicit C `shuffle-pairs`: C and IR both **14 / 54** on x86,
  **17 / 68** on ARM. This is a control, not a new opportunity.

The widening hand rewrite reaches the integrated register `pmovsxdq` at
effective SSE4.1, the SSE2 compare/unpack recipe at v1, and ARM `sshll`.
It retains a full input `movups`/`ldr q` and full output store. Effective v3
with `-sse41,-avx,-avx2,-fma,-f16c` retains the 14-instruction fallback;
the original remains 23. Release feature legality remains authoritative;
no feature is enabled by recognition. The release `feature_guard_tests` gate
passed its one active test, separately from these finite decoded inventories.

Eight extra accepted C probes retain evidence distinguishing signed-low,
unsigned-low, signed-high, reversed, repeated, mixed-source, promoted-short
and partially initialized constructions. They inspect Full IR, not hypothetical
matcher rejection. A separate complete extract/insert permutation remains an
extract/insert chain after Full. Existing `instcombine/vector.rs` knows extract
through splat/insert/shuffle, redundant/overwritten inserts, shuffle identity
and limited composition, and nested widen conversions; it has neither the
general chain-to-shuffle rule nor this memory reconstruction rule. Nontrivial
shuffle composition is deliberately restrained to preserve target recipes.

This explains why an insert-chain rule is not the smallest next measured fix.
Even hand-forming the measured interleaved shuffle still uses the general
scalar shuffle lowering; native pair/dword selectors do not cover this mask.
Widening needs only two extension/extract proofs and removes more instructions
at v2/v3. Defer shuffle reconstruction and its separate selector question.

## Native evidence, rollout gates and limits

Native checks held `/tmp/keel-perf-20261019.lock` on each host. Apple ARM64
passed eight C/IR/hand bundles; LinuxHost passed 24 across v1/v2/v3 after checking
the intersection of CPU flags, including OS-enabled AVX eligibility. Each uses
the maintained driver and 64 independent seeded oracle rows, all result bytes,
exact aliases and canaries. Explicit-IR controls additionally check misalignment;
C vector pointer alignment preconditions remain in force. Six Apple and eighteen
LinuxHost guarded bundles check all eight combinations of aligned input/output
page edges. Widening also accepts an inaccessible unused third input pointer.
A separate IR-footprint execution probe with only eight source bytes readable
faults before output publication; its pointer violates C vector alignment, so
it provides no defined-source-C correctness claim. All hand probes pass.

Before implementation is accepted, add optimizer positive/negative tests for
every eligibility gate above, resolved aliases and detached producers, multiple
root/source users, partial/torn/overlapping writers, debug homes, escapes,
volatile/atomic accesses, stack lifetime barriers and out-of-block stores.
Test effect identities/order and source locations, normal and forced-spill
selection/allocation, direct/text/chunked object parity, effective feature
overrides and fixed points. Native C tests should retain a source vector across
an external mutating/clobbering call and check single evaluation, all signed
extrema/random values, aliases and exact footprints. These are future pass
gates; this audit did not implement them or prove arbitrary fault ordering.

Artifacts, exact commands, frontend/Full/hand IR, C probes, decoded objects,
counts, native/guard results and hashes are under `/tmp/keel-next-c-d7d89d7d`
on MacHost and LinuxHost. The source build and compiler emission ran on MacHost;
LinuxHost linked and executed those ELF objects without rebuilding a compiler.
No timing campaign ran. Lock logs show load above the maintained timing
preflight threshold on both hosts; drawing timing conclusions would be
inappropriate. Driver timer output is discarded. Instruction/byte reductions
are static observations, not performance ratios or exhaustive ISA certification.

This match preserves pointer-only C ABI and changes no vector calling convention,
FP behavior, frontend acceptance, vendor headers or default widths. It handles
no loops, induction, tails, numeric vector conversions, partial vectors or
arbitrary auto-vectorization. Frames/argument moves and general shuffle
scalarization remain. Broader promotion and widening shapes need separate
evidence; no framework expansion is required for the proposed first match.
