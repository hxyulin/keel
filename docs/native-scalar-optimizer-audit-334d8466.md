# Native scalar optimizer audit at 334d8466

Research base: `334d84665dce141785503dae8d26e51eb1c72d35` (2026-10-23).
This audit owns only this document. No production source, pass schedule, ABI,
FP, interposition or ISA policy changes. No subagents, main changes, sibling
integration, Rust builds or timing campaign were needed. The instruction-selection
worker shared retained application artifacts and owns separate scalar emission
candidates; those are not optimizer recommendations here.

## Provenance and scope

`git diff d7d89d7d..334d8466` changes only two audit documents. All **1,857
tracked non-document files** at this base match the frozen native baseline
snapshot byte-for-byte. Its release compiler SHA-256, checked before and after
probes, is `72b0c01e1bf06d2c2e57233c8f2bc213f24f64b75cc8b757aaa640b158ea7681`.
Forty-three retained QuickJS/Lua/SQLite compile-source records were rechecked
against their recorded source hashes. Raw application source and flags were
reused from the [native qualification](provenance/linux-binary128-tc-layout.md)
and [SQLite audit](native-sqlite-reciprocal-audit.md), without source rewrites,
identity macro overrides or new libc assumptions.

Retained raw-source Full v1/v3 IR pairs have identical hashes at each level:

- QuickJS `quickjs.c`: `397d32ae65e7a64ac41a430fe26bcc072f413c2439987f195644538e22901d32`.
- Lua `lvm.c`: `3857c6b008d97d24aa681c865890bc79010ebf2b61a8eac45ead9e4fcdcb6a6d`.
- SQLite `sqlite3.c`: `4c1e50658d84d78283bbabb77ca4809171e39b1f37b9b3aea1f9e0220c987997`.

Fresh raw-source 64-pass prefixes reproduce the retained QuickJS and SQLite
Full IR byte-for-byte. Prefixes are direct `cc` invocations on raw sources;
serialized IR replay is used only for diagnostic counterfactuals **after** Full.
An optimization-free serialization control separates printer renumbering from
actual changes. The retained native SQLite schema-2 Full report has SHA-256
`096b7a4ad6f3a26e65b91906e02d6cb518bfbd29c3161de42a8e65ac709f8178`;
selected per-pass observations are retained externally. Report timing fields and
probe wall times are not compile-cost measurements.

No hash-verified current dynamic profile was established in this round.
`JS_CallInternal`, `luaV_execute` and `sqlite3FpDecode` are historical priority
regions, not proven current dynamic bottlenecks. Their present native IR and
encoding establish static activation only. No removable runtime cost is claimed.

## 1. Propagate a proven zero sign through constant arithmetic right shift

**Recommended bounded implementation investigation.** Owner:
`crates/keel-opt/src/instcombine.rs`, especially `possible`/`bits_and_shifts`,
with focused optimizer regressions. Do not enable global `known-bits` or add a
new analysis/scheduling framework.

Native QuickJS `JS_CallInternal`, original Full values `v5861`, `v5884` and
`v5886`, contains this bytecode-field extraction:

```text
byte = load.i8 pc
wide = uext.i32 byte
shifted = sshr wide, 5
field = and shifted, 7
```

The first blocking pass is the first `instcombine`, pipeline index **3**
(zero-based). Direct raw-source prefixes immediately after that pass and after
the next instcombine (index 10) retain the mask; Full retains it too. The local
`possible` query handles `Shl` and `Ushr`, but returns the full type mask for
`Sshr`. Its consumer therefore cannot prove that the final `and` is redundant.
An extra instcombine on the minimal reducer also retains it: this is a missing
local fact, rather than evidence for another scheduling round. Existing opt-in
`known-bits` removes the exact application `and`, reducing this function from
9,376 to 9,375 laid-out IR instructions.

The proof is exact: `uext.i32` from any i8 bit pattern is in `[0,255]`. Its i32
sign bit is zero, so arithmetic shift by five equals unsigned shift and produces
`[0,7]`. AND with seven is identity. SQLite `hexFunc` has the analogous
`uext.i32(load.i8)`, arithmetic shift four, mask fifteen. A narrow load-fed
exact-shape screen finds **two sites in two functions** across these modules:
these QuickJS and SQLite sites, and none in Lua. Broader sign-known-zero shapes
were not counted; these counts must not be advertised as all admissions of a
more general rule.

For the smallest pilot, admit i8-to-i32 unsigned extension followed by an i32
constant arithmetic right shift and a covering literal mask. Alternatively,
extend the existing `possible` Sshr query only when the existing bounded source
query proves the i32 sign bit cannot be set, returning its shifted possible-bit
superset; retain the conservative full mask otherwise. That broader admission
requires a fresh impact count. Keep the existing depth-three producer bound,
masked shift-count semantics and worklist. No extra function/module scan, range
map, fixed point or new pass invocation is needed. Work is a bounded producer
query at an already visited instruction. Compile cost remains unmeasured;
compare unchanged-input compilation under the shared lock before promotion.

Keep the byte load at its original position and width. Removing only the pure
identity changes no store, pointer, fault order, ABI transport, FP operation,
NaN/signed-zero behavior, contraction or ISA requirement. Do not derive the proof
from the pointed-to allocation or from a different load. Negative-sign inputs
without unsigned-extension proof must retain their mask (`(-1 >> 5) & 7` is 7).
Regression admission should include every byte, unknown-sign inputs, all masked
count boundaries including 0/31/32/negative literals, masks missing a possible
bit, and depth-limit fallback. Other integer widths need deliberate coverage
before widening this pilot.

Retained native v1 and v3 QuickJS text contains `sar esi,5; and esi,7` at this
site. The reduced original and existing-known-bits counterfactual independently
decode to the same byte load and arithmetic shift, with only the `and` removed
from the relevant operation sequence. Four native GCC/Clang harness executables
(v1 and v3 each) exhaust all **256 bytes** with the input at the last byte before
a protected page; unknown-sign negative controls also pass. This establishes
finite reducer execution and a one-byte end-boundary footprint check, not an
application speedup or an exhaustive memory/fault proof. The transformed whole
application was not linked or executed in this round.

## 2. Select already-computed arguments on same-destination conditional edges

**Hold pending copy-lowering/profitability evidence.** Owners:
`crates/keel-opt/src/simplify_cfg.rs`/the existing `if_convert.rs` helper and
focused regressions; coordinate emitted-copy behavior with codegen. Do not
enable general if-conversion on these observations.

QuickJS has two signed i32 min choices in `JS_CallInternal`; SQLite has one in
`sqlite3FpDecode`. In serialization-normalized Full IR, the first QuickJS site is:

```text
loaded = load.i16 address
wide = uext.i32 loaded
condition = icmp.slt argument, wide
brif condition, join(argument), join(wide)
```

Both operands already dominate the terminator. Replacing the two edges with
`select condition,argument,wide; jump join(selected)` computes the identical i32
join argument. It speculates no instructions and preserves every load/store,
call, FP operation and their order. Nonzero condition polarity is retained.

The exact first blocker is `simplify-cfg` at pipeline index **1**. Raw prefix 1
(after inline) has separate forwarding arms; prefix 2 (after CFG cleanup) has
two same-target edges. `fold_branches` folds only identical block **and argument**
calls; differing arguments correctly prevent its current jump-only rewrite.
The shapes survive prefixes 6 and 64. The existing opt-in `if-convert` pass
performs the counterfactual directly. Its broader transformations affect 116
QuickJS, five Lua and 414 SQLite functions, so those broad counts are not this
pilot's impact. A one-parameter i32, distinct-argument, non-self-target screen
finds **29 sites / 22 QuickJS functions**, **zero Lua sites**, and **72 sites /
61 SQLite functions**, including the three highlighted sites.

The minimal reduced function loads exactly two bytes, unsigned-extends to i32,
and returns the signed minimum with an i32 argument. Four native GCC/Clang
executables (v1/v3) each pass **100,045** boundary/random pairs with that load at
the last two bytes before a protected page. However, baseline emits
`cmp; jl; mov`, while selected code emits `mov; mov; cmp; cmovl; mov` in the
relevant sequence. Branch elimination adds copies in this allocation. No win is
established; native branch frequency/predictability and application register
pressure are unknown.

If revisited, admit only one differing i32 join argument with existing canonical
SSA definitions, non-self target, and precomputed dominating values. Preserve
other equal arguments verbatim; exclude arm speculation, FP/pointers, memory
motion and multi-argument parallel-copy issues from the initial pilot. A single
existing terminator scan suffices; a proposed cap of 64 conversions per function
and 16,384 laid-out instructions bounds a trial while admitting both inspected
functions. This is a future trial budget, not implemented policy. Gate codegen
copy behavior and locked compile/runtime cost before proposing default admission.

## Negative findings and receipts

Extra `branch-ranges` changes seven QuickJS and 39 SQLite functions, but neither
VM nor `sqlite3FpDecode`; Lua has zero changed functions. Extra
`stack-forward-cfg` changes **zero functions in all three modules**. Extra
`known-bits` changes nine QuickJS and eight SQLite functions, zero Lua; this
motivates only the exact local signed-shift fact above. It does not establish
profitability of global range/known-bits scheduling. Broader if-conversion also
speculates an integer shift in Lua; that is outside recommendation 2.

The [native reciprocal audit](native-sqlite-reciprocal-audit.md) remains binding:
the digit-store loop has distinct loads across an ordinary `memcpy` escape;
extra GVN does not merge them. No libc semantics, no-capture contract, fake alias
or builtin provenance is inferred from a linker name. Private aggregate, byval
and canonical-cache pilots remain without an established payoff. No generic
analysis manager, unconditional GVN rewrite, escape relaxation or FP arithmetic
rewrite is recommended.

Artifacts live only under `/tmp/keel-opt-native-334d8466/` on LinuxHost and locally.
`identity.json`, `final-audit.json`, `probes.json`, `bits-probes.json`,
`native-full-report-summary.json`, `traces.json`, `bit-traces.json`,
`same-join-screen.json`, `mask-screen.json`, `oracle-commands.json` and
`mask-oracle-commands.json` retain hashes, exact commands, observations and native
outcomes. Scripts, reducers, generated IR/text/objects, binaries and reports stay
outside Git. Original shared artifacts remain under
`/tmp/keel-spill-payoff-d7d89d7d/` and
`/tmp/keel-native-reciprocal-d7d89d7d/`.

Compiler probes, direct raw-source traces and native oracle commands serialize
through `suite/fast_opt_locked.py`, using task-specific `/tmp` JSONL receipts.
No Rust compilation/tests or cargo invocation was necessary; no build outputs,
toolchain or wrapper configuration changed. Existing release CLI emissions retain
ISA legality enforcement. Reducer object decoding and native LinuxHost execution are
distinct evidence; frontend/IR acceptance alone is not native validation. LinuxHost
is not a v1-only CPU. No fresh whole-application oracle or release-guard unit test
was run, and no exhaustive ABI/ISA/FP conformance claim is made. The maintained
[ISA](userspace-isa-status.md), [C intrinsic](c-x86-intrinsics.md) and
[GNU vector](gnu-vector-mode.md) contracts remain unchanged.
