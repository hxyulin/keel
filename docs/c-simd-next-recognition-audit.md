# Next C SIMD recognition audit

## Independent review on 9208b9e4

The review branch `review/simd-recognition-audit` reapplies candidate
`ebaae2c104950a215c92b79bad21ff3c32ccd540` to the requested clean main
`9208b9e4061bc46bd0b17d668bbfb8c1bc8d7fed`. The original worktree and main
are untouched. No production correctness defect was found in the bounded
matcher. Review traced final byte writers, alias resolution, live result and
parameter ownership, same-block availability under verified SSA, exact/global
precedence, insertion through the instruction snapshot, and whole-slot
remaining-read store retention. The existing escape, debug, atomic/volatile,
frame-observer, returns-twice and stack-lifetime exclusions remain unchanged.
The helper neither deletes shared producers nor changes external effects.

Review strengthens two tests. A new optimizer case replaces consecutive vector
reloads through an aliased derived address while retaining a same-block pointer
read and both private stores, then checks verification and standalone
idempotence. As with existing address closure, a stale live aliased `ptradd`
remains conservatively excluded; the eligible fixture removes the replaced
instruction while retaining its SSA alias. The ARM native callback now zeros
the caller-saved upper halves of v8-v15 in addition to v0-v7/v16-v31, preserving
the ABI-required low halves. Inspection of Apple Clang's generated callback
confirmed the lane writes and low-half save/restore. This repairs a test
coverage gap, not an observed compiler ABI failure.

Fresh debug and release checks passed: 41 optimizer library tests, 24 existing
stack-forward tests, 10 lifetime tests, and all eight focused widening tests
(83 active tests per profile; the existing sealed-profile test stays ignored).
Both profiles also passed the eight x86/two ARM backend widening tests, the
emission feature guard, and the dedicated C integration test. The latter
checks frontend versus Full formation, verified IR, strict feature legality,
text/direct/chunked object equality and normal/forced-spill allocation. Native
Apple ARM execution passed all six configurations in each profile with the
stronger callback. All Rust commands used `CARGO_CACHE_LINKS=0`,
`suite/fast_opt_locked.py`, `cargo`, `--offline --locked -j2`,
the existing toolchain/default features and managed target directory.

LinuxHost freshly linked the review's exported ELF objects, without rebuilding the
compiler there. Clang 23.1.1 and GCC 16.2.0 each passed all 24 x86 configurations
per profile (48 runs per profile, 96 total), after checking required flags on
every reported CPU. The shared lock covered native builds and execution.
Returned object hashes match the local exports. These runs include snapshot
mutation, physical SIMD clobbers, single evaluation, signed edge/random values,
exact aliases, canaries and protected input/output page edges. The partial-input
fault witness is explicitly align-1 IR; C vector accesses use aligned complete
objects. The witness faults before shared output publication and the later
effect, and its partial output faults before the later effect.

Independent LLVM decoding on LinuxHost inspected twelve x86 objects and one ARM
object per profile. The candidate and fault witness retain full `movups` input
loads/output stores and register `pmovsxdq` at SSE4.1; effective feature removal
selects the SSE2 compare/unpack recipe. ARM retains `ldr q`, `sshll` and `str q`,
including a full source spill/reload across the clobbering call. No m64 input
cover is authorized. Encoding checks and native execution remain separate
evidence; execution on modern CPUs does not certify feature-masked hardware.

Review logs, exported IR/assembly/objects, callback assembly, native hash reports
and the LinuxHost runner are under `/tmp/keel-swiden-review-*` on the relevant
hosts. Lock logs include `debug`, `release`, `opt-final-{debug,release}`,
`backend-final`, `backend-release`, `guard-release`, `c-debug-final`, `c-release`
and `linuxhost-{debug,release}`. No new runtime or compile-cost timing was attempted;
the original contention-deferred seven-round campaign remains deferred. This
review changes no FP semantics, ABI, default width, pipeline or frontend/backend
surface, and makes no application performance claim. Parent integration remains
separate.

## Implementation acceptance from 9208b9e4

On 2026-10-23 the bounded design refined in `dbfc4eb7` was implemented from
clean `9208b9e4061bc46bd0b17d668bbfb8c1bc8d7fed` in an isolated worktree.
`d2422cc4` owns only `keel-opt/src/stack_forward.rs` and its new integration
suite; `d037033a` owns the dedicated C-to-object test and two native fixtures.
This section supersedes the future-tense acceptance requirements below; the
original research and counterfactual evidence remain historical.

The helper and call site add roughly 90 lines. They consume the existing final
sixteen byte facts and require the exact two i64 sign extensions of lanes 0/1
of one resolved i32x4 snapshot. The source, extracts and extensions must be
live and owned by the reload block, with a live source block parameter also
allowed. Nonzero source result indices are checked against their producer's
result list. Existing exact/global forwarding keeps precedence. Success adds
one conversion at the reload's position/location and uses the existing
replacement/removal and whole-slot remaining-load cleanup. Failed proofs do
not mutate IR. Shared pure producers remain eligible and DCE owns deadness.
No analysis, use scan, cap, API, schedule, header, ABI, FP or default-width
change was added. The existing little-endian target assumption still applies.

The seven focused optimizer tests include a 35-scenario byte-composition
matrix, direct aliases/detached producers/live parameters, nonzero source
result ownership, source locations and unchanged effect identities/flags/order,
shared-producer survival, residual-read store retention, standalone idempotence
and Full fixed points/straight-line CFG exposure. They verify valid fixtures;
detached fixtures assert rejection without claiming valid SSA. A future
producer is rejected by the SSA verifier. Earlier overwritten zeros/torn/overlap
writes can match; incomplete or finally damaged bytes, unsupported conversions,
wrong lanes/sources/types, escapes, debug homes, volatile/atomic observations,
frame observers, returns-twice and opaque stack marks do not gain permission.
The existing alias guard and lifetime suites remain intact.

Validation used the shared default cargo cache, disabled link caching and the
shared lock, preserving the toolchain and default features:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-low-widen-93690-opt-final.jsonl \
  cargo test --offline --locked -j2 -p keel-opt \
  --test stack_forward_widen --test stack_forward --test stack_forward_lifetimes --lib
python3 suite/fast_opt_locked.py /tmp/keel-low-widen-93690-backend.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen \
  --test vector_widen_x86 --test vector_widen_aarch64
KEEL_C_STACK_WIDEN_EVIDENCE=/tmp/keel-low-widen-93690-native-final \
  python3 suite/fast_opt_locked.py /tmp/keel-low-widen-93690-c-final.jsonl \
  cargo test --offline --locked -j2 -p keel --test stack_forward_widen
python3 suite/fast_opt_locked.py /tmp/keel-low-widen-93690-release.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen --release --lib feature_guard_tests
```

The optimizer run passed 41 library, 24 existing forwarding, 10 lifetime and
seven new tests (one existing sealed-profile test remains ignored). The reused
backend suites passed eight x86 and two ARM tests; the release legality guard
passed. The dedicated C test passed in debug and release. It distinguishes
unoptimized frontend construction from actual Full matcher formation, verifies
IR and emits strict v1/v2/v3, effective v3 minus
`sse41,avx,avx2,fma,f16c`, and baseline Apple ARM objects. Baseline/tree/full
selection, normal/empty allocation pools, and text/typed/chunked object parity
are checked for all configurations, including forced spills.

Native Apple ARM execution passed all six selection/allocation configurations
in both build profiles. LinuxHost (Ryzen 9 9950X3D, Linux) passed 48 runs per build
profile: four x86 feature configurations times three selectors times two pool
configurations times independent Clang 23.1.1/GCC 16.2.0 harness builds. CPU
flags were checked before x86 execution. The scalar oracle covers 625 signed
edge combinations plus seeded random inputs through 2,048 rows, single source
pointer evaluation, original snapshots across a mutating call that also zeros
all caller-saved SIMD registers, exact input/output aliases, canaries and both
protected page edges. A separate align-1 IR construction activates the matcher
and retains its full sixteen-byte input load and output store: eight readable
input bytes fault before shared output publication and a later external effect;
eight writable output bytes fault before the later effect. The deliberately
unaligned fault witness makes no defined-C-alignment claim. Execution on these
newer CPUs is not feature-masked v1 hardware certification.

Frozen release compilers were built from the baseline archive and candidate
with the same locked cargo command. Their SHA-256 identities are:

- Baseline: `54b9a00678360da9b5027aa5e6c3c1ac7a61607a7654d128ff21d5bcd72ba7ec`.
- Candidate: `f920f263c4b77403ea1f457398b5d32f52ed2f18f1a05a8f7a186aeb85310e8c`.

The maintained `suite/simd-compare/run.py` paired Full `widen` campaign completed
static/correctness checks on all four profiles. Verified Full C IR now forms
`swiden_low.i64x2`; frontend C lowering is unchanged. Independent LinuxHost LLVM
objdump decoding gives the following whole-function counts, including frames
and argument moves. Text bytes come from the object section, separately from
assembly counts:

| Profile | Baseline C instructions / text bytes | Candidate C instructions / text bytes |
| --- | ---: | ---: |
| v1 | 26 / 93 | 17 / 55 |
| v2 | 26 / 93 | 13 / 38 |
| v3 | 26 / 93 | 13 / 38 |
| Apple ARM | 22 / 88 | 12 / 48 |

Candidate C text is byte-identical to the explicit IR control on each profile;
the explicit IR text is unchanged between compiler revisions. Decoding retains
full `movups`/`ldr q` input snapshots and full output stores; SSE4.1 uses register
`pmovsxdq`, v1 uses the existing SSE2 recipe, and ARM uses `sshll`. This grants
no m64 external-load cover. The smaller Clang objects still use a different
footprint/code shape; no remaining explicit-IR-to-Clang gap is claimed closed.

The requested paired seven-round benchmark was attempted with frozen baseline
and candidate compilers. The maintained preflight deferred it at load 2.74
(`complete:false`, `contended host; timing deferred`). No measured runtime or
compile-cost samples were accepted. Diagnostic command/build wall times are
not compiler-cost measurements. Static counts are not speedup ratios, and this
finite sample establishes no application or arbitrary auto-vectorization claim.

Artifacts, exact logs, Full IR, frozen compilers, object parity and static
reports remain under `/tmp/keel-low-widen-93690-*`. Local acceptance logs end in
`opt-final`, `c-final`, `c-release`, `backend` and `release`; the completed static
report is `static-final/report.json`, and the deferred timing report is
`benchmark/report.json`. LinuxHost native results are
`/tmp/keel-low-widen-93690-native{,-release}/linuxhost-native.json`, with lock logs
`/tmp/keel-low-widen-93690-linuxhost-{final,release}.jsonl`; independent decoded
counts are `static-final/decoded-counts.json` on both hosts. No generated
artifact is committed. Independent review and parent integration remain open.

## Historical design and research

Design refinement at clean main
`334d84665dce141785503dae8d26e51eb1c72d35`, in an isolated research worktree.
At that research revision this document was the only Git change; no production
matcher, frontend builtin, header, pipeline or backend change was implemented. The original object/native
investigation below was at `d7d89d7daf6dfd8778869f4aa5487707deaa229e`;
its evidence is historical, not a fresh execution claim for this base.
The wrapping i32x4 sum (`68d55bbc`) and SSE4.1 register widening (`d0aec154`)
are already integrated and are not new recommendations.

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

## Smallest production change proposed

Reuse the ordinary `stack-forward` pass and its existing byte facts. Do not add
an analysis, public API, pass name, schedule switch, use table or function-size
admission cap. Proposed private helper signature (design only):

```rust
fn signed_low_widen_source(
    f: &Function,
    at: Inst,
    slot: StackSlot,
    bytes: &FxHashMap<(StackSlot, usize), (Value, usize)>,
) -> Option<Value>
```

The return is the resolved i32x4 source, not a newly constructed value or a
producer list. `at` supplies the reload block. No new representation is needed:
read sixteen existing `(stored_value, source_byte)` entries directly. Caller
gates are `ty == Type::Vector(VectorType::I64x2)`, offset zero, slot size
exactly 16 and `!local_only`, after the existing escaped-slot and aliased-load-result checks.
Retain existing exact/global forwarding precedence; attempt this proof before
the integer-only reconstruction fallback. Success inserts one
`InstData::Convert { op: ConvertOp::SwidenLow, ty: Type::Vector(VectorType::I64x2), arg: source }`
before the load, copies `inst_loc` using `set_inst_loc`, then uses the existing
replace-uses/remove-load path. Failure does not mutate IR or byte facts.

For each half `h = 0, 1`, obtain its first stored value and require bytes
`8*h + n`, for `n = 0..8`, to resolve to that same value with source byte `n`.
Require its resolved type to be i64 and definition exactly
`Convert { op: Sext, ty: I64, arg }`; the resolved `arg` must be i32 defined by
`VectorExtract { arg: source, lane: h }`. The two resolved sources must be the
same i32x4 value. Check `ValueDef::Result(inst, 0)`, nonremoved instruction and
matching first result for each extension/extract before inspecting data.
`Function::is_value_defined` also accepts live block parameters for the source;
check it before inspecting source ownership. A result source must belong to
the indicated result index of a live instruction (`inst_results`); do not
unnecessarily restrict a source to result zero. Arena data survive instruction
removal: a readable `inst_data` entry alone proves nothing about availability.
Never call `inst_block` on a removed instruction.

For this first increment, require the two extensions/extracts and any source
instruction to belong to the reload block; allow a source parameter of that
block. Verified SSA then supplies execution order and availability without
`inst_position` searches or a new dominance query. No producer walk through
bitcasts, copies, phis or other operations; resolving existing SSA aliases is
required and is distinct from interpreting a new producer operation. Reject
malformed/detached producer states conservatively; this is not a promise to
repair invalid SSA. Existing cross-block exact/scalar forwarding stays intact;
the new byte-composition proof does not read global `unique`/`subranges` facts.
A Full CFG merge may subsequently expose a valid local candidate.

Shared extracts, extensions, sources and reload results are eligible. The
matched scalar producers are pure and every reused SSA value is immutable;
no producer is overwritten, re-evaluated, cloned or explicitly deleted.
A use-count gate is unnecessary for correctness; its only purpose would be to guarantee removal of every scalar producer. Accept sharing
and let existing DCE determine what dies. This supersedes the earlier proposed
single-use payoff gate, lazy resolved-use scan and 8,192/16,384 admission caps.
Do not promise the unshared instruction-count reduction for shared cases.

The added candidate work is sixteen byte lookups and four producer inspections,
plus alias resolution and constant-size ownership/type checks. Existing address
closure, writer scans, instruction insertion/removal and alias chains still
prevent a constant-total-work claim. There is no new full-function scan, recursive
matcher, analysis invalidation scheme or tuning cap. A load becomes a conversion
in one direction; the helper cannot match its own output. Test standalone
idempotence and Full fixed points; retain the existing three Full rounds and
epilogue in `keel-opt/src/lib.rs`.

## Existing facts, safety barriers and correctness argument

`stack_forward.rs::run` collects only live unaliased `StackAddr` results, closes
constant nonnegative in-bounds `ptradd` chains with checked addition, and resolves
operands during address lookup and escape checking. Unrecognized address uses
escape the whole slot, even if they occur after the candidate. Storing the
address as a value, passing/returning/converting it, unsupported derivations,
volatile accesses and atomic instructions exclude it. Atomic accesses are
separate `InstData` variants and fail the allowed-use whitelist; they are not
ordinary loads with a missing flag check. `debug_vars` and implicit
`AggregateVaArg` homes also exclude the slot. Inline asm, frame-address observers
and direct/indirect/invoke returns-twice calls disable the pass. Preserve these
existing conservative exclusions, including observations of unrelated slots.

A restore without a known opaque save mark, an aliased mark, or observation of
saved SP disables forwarding. Valid opaque marks set `local_only`, disable
cross-block writer facts and store deletion, and clear local facts at save and
restore. The new match rejects `local_only` entirely, including otherwise local
intervals; scalar forwarding retains its current behavior. Ordinary calls do
not clear facts for a proven nonescaping fixed slot. This narrower first match
requires no changes to lifetime analysis.

At every store, `exact` entries overlapping the written range are invalidated;
all overwritten byte facts are removed. Integer i8/i16/i32/i64 stores then
record `(resolved value, byte index)`. Vector/pointer/FP stores can establish
exact-type facts but do not seed integer reconstruction bytes. At every block
entry both local maps clear. The sixteen-byte proof consequently uses final
last writers, not adjacent store syntax or zero-initialization folklore.

Require final bytes 0..7 and 8..15 to be the complete ordered extension values.
A later partial/overlapping write fails the proof; earlier zeroing, overlaps or
torn writes fully overwritten by the final pair are harmless and should pass.
No identity of Store instructions or extra write-history map is necessary.
Uninitialized bytes fail coverage; zero-filled omitted C initializer lanes
have complete coverage but fail the exact extension proof. Zero extension,
i16 sources, mixed sources, high/reversed/repeated lanes and extra casts fail.
C promotions/signedness have already become scalar IR; do not infer them from
source spelling. LP64 `long` is i64 on these supported targets.

With little-endian scalar byte layout and lane zero at the lowest address,
concatenating these two complete i64 representations equals signed widening
of i32x4 lanes 0 and 1. Current x86-64 and AArch64 targets are little endian,
and existing integer reconstruction already uses shifts in that direction.
The target-free pass has no endian context: this proposal explicitly shares
that supported-target assumption, not a new big-endian contract. Before adding
a big-endian target, gate/adapt this and existing byte reconstruction together.
IR vector values contain no undef/poison lanes; inventing lanes is unnecessary.
No FP operation, environment contract, vector ABI or frontend surface changes.

The input is an already evaluated SSA snapshot. Keep the original exact
16-byte external load even though only its low eight value bytes contribute;
never form an m64 PMOVSXDQ memory cover. Preserve its flags, pointer, location
and order with calls and all other external effects. A clobbering call between
source evaluation and reload must not cause source reloading. Output remains
an exact 16-byte store. Changing only a proven unobservable private reload
preserves source effects/first-fault order; existing stack optimization does not
promise debugger edits of excluded private homes or stack-exhaustion faults.
A pure source instruction may move later under existing Full scheduling; the
proof makes no new permission to move external effects.

Success must bypass `remaining_loads.insert(slot)` just as existing forwarding
does. Failure records the slot. Store deletion is a separate final whole-slot
decision: only nonescaped, non-`local_only` slots with no remaining loads lose
stores. One unmatched pointer/vector/uninitialized read in any block keeps
all private stores, even if one widening reload was replaced. Do not delete
stores or shared pure producers from the helper, or infer whole-slot deadness
from this single reload. The live-result alias guard already in the load arm
must run before the new proof so it cannot overwrite a preexisting SSA alias.

## Bounded implementation and test handoff

One implementation increment should own only `crates/keel-opt/src/stack_forward.rs`
and a focused optimizer integration test (for example
`crates/keel-opt/tests/stack_forward_widen.rs`), plus this document's acceptance
update. No other research document or backend/frontend/header ownership.
Keep the helper and call site to roughly 100 source lines; if new analysis or
an IR API appears necessary, return the design issue instead of widening scope.
Bound the first optimizer matrix to about 35 parameterized scenarios, reusing
existing barrier tests. Run that suite plus existing `stack_forward`,
`stack_forward_lifetimes` and the in-module alias tests under the shared lock.
Then run `vector_widen_x86`, `vector_widen_aarch64` and the release
`feature_guard_tests` filter in `keel-codegen` once; retain their existing
features/target configuration. This is a work budget, not a semantic or
function-size cutoff.

The optimizer tests should assert semantic IR properties and preserved effect
instructions, not mirror helper branches or demand an accidental SSA numbering:

- Accept the signed low pair, zeroing then full overwrite, repaired overlap,
  and shared extract/extension/source/result cases. Resolve aliases on both
  stored values, extracts, sources and addresses; compare resolved values.
  Accept an ordinary call on an external pointer between input evaluation and
  reload while retaining that exact input-load instruction and the call.
- Reject uninitialized, read-before-write, one-half-only, zero-filled omitted
  lane, torn i32 overwrite, offset-four overlap, zero extension, i16 extraction,
  mixed sources, high/repeated/reversed lanes, wrong width/slot size/offset and
  unsupported cast chains. A preexisting exact vector store/load remains the
  existing exact-forwarding control, not evidence for this matcher.
- Exercise volatile load and store, atomic load and write, address escape before
  and after reload, stored/returned address, debug home, implicit vararg home,
  inline asm, frame observer, returns-twice direct/indirect/invoke, opaque
  saves/restores and observed/external/aliased marks. Existing scalar tests cover
  much of this; add representative vector negatives rather than duplicate every
  scalar lifetime permutation.
- Split the two stores/reload across blocks and require no new standalone
  reconstruction. Retain a competing unreadable load in another block and prove
  stores remain after a successful local match. Separately allow Full to merge
  straight-line blocks; do not demand that Full retain the original block shape.
- Use direct in-memory tests for aliases and detached extension/extract/source
  results (text reparsing destroys those states), aliased live reload results,
  a live block-parameter source and source locations. Verify valid input/output;
  invalid detached fixtures check rejection/no resurrection without claiming
  they verify. SSA verification rejects future/non-dominating definitions;
  matcher ownership checks are not a second SSA verifier.
- Check original external load/store/call instruction identities, flags,
  operands and relative order after the named pass, exact widths and preserved
  source location on the new conversion. Check returned data and surviving
  shared producers, residual-read store retention, standalone second-run false
  and Full fixed point modulo parser SSA renumbering.

A second delegated validation increment should own only a focused C-to-object
integration test and its native driver/guard fixtures. Keep pointer-only ABI;
exercise single evaluation, a source retained over an external mutating/clobbering
call, signed extrema/random values, exact aliases, canaries and protected page
edges for both 16-byte input and output. With only eight input bytes readable,
an explicit unaligned IR witness must fault before output publication and before
any later external effect; this is not a defined C vector-alignment test.
Test normal/forced-spill allocation, text/direct/chunked object parity, v1/v2/v3,
effective v3 with `-sse41,-avx,-avx2,-fma,-f16c`, and baseline ARM. Keep release
feature legality enabled. Existing explicit-register widening backend evidence
below remains separate: it cannot prove the new automatic matcher, its source
load footprint or native fault order. Native LinuxHost validation can reuse fleet
access and existing tools; no new compiler/header/ABI/FP framework is justified.

## Fresh design reductions at 334d8466

On 2026-10-23 the existing default-feature debug CLI was built with the pinned
configuration and cached compilation, using:

```sh
python3 suite/fast_opt_locked.py \
  /tmp/keel-swiden-design-334d8466/build.jsonl \
  cargo build --offline --locked -j2 -p keel
python3 suite/fast_opt_locked.py \
  /tmp/keel-swiden-design-334d8466/tests.jsonl \
  cargo test --offline --locked -j2 -p keel-opt \
  --test stack_forward --test stack_forward_lifetimes --lib
```

The 41 library, 24 active stack-forward and 10 lifetime tests passed; one sealed
QuickJS profile test was ignored as declared. The live-load-result alias test,
debug homes, scalar initialization/overlaps, calls and lifetime/observer tests
are existing behavior checks, not implementation tests for a new matcher.

A frozen CLI copy has SHA-256
`556789b6a1d251358fd291f1d39c47d5ea0e24fdf656c7658a647adb8f74a30f`.
Artifacts, scripts, exact lock logs and snapshots are in
`/tmp/keel-swiden-design-334d8466` outside Git; cargo retains its managed checkout
target/cache. `probe.py` ran 30 IR reductions through standalone `stack-forward`
and Full with `--verify-each`: signed-low, sharing, overwritten zeros, repaired
and final overlaps/torn writes, uninitialized/partial/zero-filled lanes,
zero extension, mixed/high/reversed/repeated lanes, volatile/atomic reads,
escapes before/after, ordinary/returns-twice calls, frame/asm observers,
save/restore, block split and a residual pointer read. All 60 outputs retained
the vector reload and formed no `swiden_low`. Thus even the proposed positives
are still misses; negative outputs are not tested matcher rejections.

An explicit hand replacement of only that reload with `swiden_low.i64x2` passed
standalone cleanup and Full, both at fixed point modulo parser SSA renumbering.
Both retain a full i32x4 input load and full output store and remove the private
accesses. This proves only the current pipeline's handling of this explicit IR.
`cprobe.py` additionally saved 32 verified frontend/Full snapshots: eight C
shapes (signed/unsigned low, high, reversed, repeated, mixed, short, omitted lane)
on x86-64 Linux and Apple ARM64. Two Full clobbering-call snapshots, ordinary
and `-g`, keep the source vector load before the call and publication after it.
Debug-home and detached/alias cases require in-memory assertions as specified
above; this CLI experiment does not prove those future vector matcher gates.

No fresh encoding, native execution or performance experiment ran in this round;
no LinuxHost access was needed. No timing inference is made from lock/build duration
or host load. Frontend shape, optimizer proof, decoded encoding, native values
and native fault delivery remain separate evidence categories.

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
