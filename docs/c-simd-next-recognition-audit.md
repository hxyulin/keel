# C SIMD recognition: private low signed widening

This document describes a bounded `stack-forward` extension that recognizes
C code building an `i64x2` from the two low lanes of an `i32x4` through a
private stack temporary, and replaces the vector reload with
`swiden_low.i64x2`. It follows the [lane reduction pilot](c-lane-reduction-pilot.md);
the wrapping `i32x4` sum and SSE4.1 register widening were already integrated
before this change.

## Problem

In `suite/simd-compare/widen.c`, a GNU vector compound literal does not become
an insert chain. `init.rs::sub_slots` assigns lane byte offsets;
`irgen.rs::init_local` zeroes private storage and evaluates and stores each
initializer. Ordinary stack forwarding removes input homes but cannot
reconstruct the final vector reload from scalar stores:

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

The rewrite is only `v21 = swiden_low.i64x2 v4` at the original reload
position, with the reload's source location. Existing forwarding cleanup then
removes the private stores once all reads disappear, and DCE removes dead pure
producers. The input load is not folded into a narrower memory-source
instruction.

`stack_forward.rs` already owns resolved stack addresses, constant in-bounds
offset closure, whole-slot escape/debug/frame-observer exclusion, same-block
last-writer byte facts, lifetime barriers and removal of stores only after all
reads disappear, so the matcher reuses those facts rather than adding alias
analysis to instcombine. `stack_promote.rs` rejects vector accesses and
overlapping field shapes, so enabling vector promotion would not solve mixed
scalar-store/vector-load construction.

## Matcher

The helper is roughly 90 lines in `crates/keel-opt/src/stack_forward.rs`:

```rust
fn signed_low_widen_source(
    f: &Function,
    at: Inst,
    slot: StackSlot,
    bytes: &FxHashMap<(StackSlot, usize), (Value, usize)>,
) -> Option<Value>
```

It returns the resolved `i32x4` source. Caller gates are an `i64x2` load at
offset zero of a 16-byte slot that is not `local_only`, after the existing
escaped-slot and aliased-load-result checks. Existing exact/global forwarding
keeps precedence; the proof is attempted before the integer-only
reconstruction fallback. Failure does not mutate IR or byte facts.

For each half `h = 0, 1`:

- Bytes `8*h + n`, for `n = 0..8`, must resolve to the same stored value with
  source byte `n` (the final last writers, not store syntax).
- That value must be an i64 `Convert { op: Sext }` of an i32
  `VectorExtract { arg: source, lane: h }`.
- Both halves must name the same `i32x4` source.
- The extensions, extracts and any source instruction must be live and belong
  to the reload block; a live block parameter of that block is also accepted as
  the source. Nonzero source result indices are checked against the producer's
  result list.

Verified SSA then supplies execution order and availability without position
searches or a new dominance query. There is no producer walk through bitcasts,
copies or phis beyond resolving existing SSA aliases. Shared extracts,
extensions, sources and reload results are eligible: no producer is
overwritten, re-evaluated, cloned or explicitly deleted, and DCE decides what
dies.

The added work per candidate is sixteen byte lookups and four producer
inspections. There is no new analysis, full-function scan, recursive matcher,
use table, cap, API, pass name or schedule change. A load becomes a conversion
in one direction, so the helper cannot match its own output.

## Correctness argument

- **Barriers.** `stack_forward.rs::run` only tracks live unaliased `StackAddr`
  results. Any unrecognized address use escapes the whole slot, as do stored,
  passed, returned or converted addresses, volatile and atomic accesses,
  `debug_vars` and implicit `AggregateVaArg` homes. Inline asm, frame-address
  observers and returns-twice calls disable the pass. Opaque stack save/restore
  marks set `local_only`, which this matcher rejects entirely. Ordinary calls do
  not clear facts for a proven nonescaping fixed slot.
- **Byte facts.** Every store invalidates overlapping exact entries and
  overwritten byte facts; integer stores then record `(resolved value, byte)`.
  Local maps clear at block entry. A later partial or overlapping write fails
  the proof; earlier zeroing or torn writes fully overwritten by the final pair
  pass. Uninitialized bytes fail coverage; zero-filled omitted lanes fail the
  extension proof. Zero extension, i16 sources, mixed sources and high,
  reversed or repeated lanes fail.
- **Layout.** With little-endian scalar layout and lane zero at the lowest
  address, concatenating the two i64 values equals signed widening of lanes 0
  and 1. Both supported targets are little endian, and existing integer
  reconstruction makes the same assumption; a big-endian target would need to
  gate both.
- **Effects.** The input is an already evaluated SSA snapshot. The original
  16-byte external load is kept even though only its low eight bytes
  contribute; no m64 `pmovsxdq` memory form is created. Its flags, pointer,
  location and order relative to calls and other effects are preserved, and a
  clobbering call between source evaluation and reload does not cause a source
  reload. The output stays an exact 16-byte store.
- **Store deletion.** A successful match bypasses `remaining_loads.insert(slot)`
  like existing forwarding. Store deletion stays a separate whole-slot decision,
  so one unmatched read anywhere keeps all private stores.

No FP operation, environment contract, vector ABI, frontend surface, header or
default width changes.

## Tests

```sh
cargo test -p keel-opt --test stack_forward_widen --test stack_forward \
  --test stack_forward_lifetimes --lib
cargo test -p keel-codegen --test vector_widen_x86 --test vector_widen_aarch64
cargo test -p keel --test stack_forward_widen
cargo test -p keel-codegen --release --lib feature_guard_tests
```

- The optimizer suite includes a 35-scenario byte-composition matrix, direct
  aliases, detached producers, live block parameters, nonzero source result
  ownership, source locations, unchanged effect identities/flags/order, shared
  producer survival, residual-read store retention, standalone idempotence and
  Full fixed points. An aliased derived-address case checks that a same-block
  pointer read and both private stores are retained.
- The C integration test distinguishes unoptimized frontend construction from
  Full matcher formation, verifies IR, and emits x86-64 strict v1/v2/v3,
  effective v3 minus `sse41,avx,avx2,fma,f16c`, and baseline Apple arm64
  objects. Baseline/tree/full selection, normal/empty allocation pools and
  text/typed/chunked object parity are checked for every configuration.
- Native execution covers 625 signed edge combinations plus seeded random
  inputs, single source-pointer evaluation, the original snapshot across a
  mutating call that also clobbers all caller-saved SIMD registers (on arm64,
  including the upper halves of v8-v15), exact input/output aliases, canaries
  and protected page edges. A separate align-1 IR witness with only eight
  readable input bytes must fault before output publication and before a later
  external effect; it makes no defined-C alignment claim.

Execution on modern CPUs is not feature-masked v1 hardware certification.

## Static results

Whole-function instruction counts and text bytes for the C `widen` case,
including frames and argument moves (decoded with LLVM objdump):

| Profile | Before (instructions / bytes) | After (instructions / bytes) |
| --- | ---: | ---: |
| x86-64 v1 | 26 / 93 | 17 / 55 |
| x86-64 v2 | 26 / 93 | 13 / 38 |
| x86-64 v3 | 26 / 93 | 13 / 38 |
| Apple arm64 | 22 / 88 | 12 / 48 |

The C output is now byte-identical to the explicit-IR control on each profile.
SSE4.1 uses register `pmovsxdq`, v1 uses the existing SSE2 compare/unpack
recipe, and arm64 uses `sshll`; all keep full `movups`/`ldr q` input loads and
full output stores. Clang's objects still use a different footprint and code
shape, so the remaining explicit-IR-to-Clang gap is not closed. No runtime
timing has been measured; static counts are not speedup ratios.

## Why not shuffle reconstruction

The measured interleaved shuffle in `suite/simd-compare/shuffle.c` also goes
through private stores, and a complete extract/insert permutation stays an
extract/insert chain after Full. `instcombine/vector.rs` has no general
chain-to-shuffle rule, and nontrivial shuffle composition is deliberately
restrained to preserve target recipes. Even a hand-formed shuffle for that
mask uses the general scalar shuffle lowering, because native pair/dword
selectors do not cover it (C 31 / 126 versus hand-formed 24 / 92 on x86).
Widening needs only two extension/extract proofs and removes more instructions,
so shuffle reconstruction and its selector question are deferred.

## Limits

This match preserves the pointer-only C ABI and changes no vector calling
convention, FP behavior, frontend acceptance, vendor headers or default
widths. It handles no loops, induction, tails, numeric vector conversions,
partial vectors or general auto-vectorization. Cross-block construction is not
matched (a Full CFG merge may expose a local candidate). Frames, argument moves
and general shuffle scalarization remain.
