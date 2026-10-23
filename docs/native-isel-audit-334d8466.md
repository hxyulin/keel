# Native instruction-selection priorities at 334d8466

Read-only production investigation, 2026-10-23, from clean
`334d84665dce141785503dae8d26e51eb1c72d35`. This document owns the research;
it implements no selector, allocator, optimizer, header or application change.
There are **three proposed pilots**, in the order below. Their ranking uses
actual native-source occurrence and recognizable interpreter paths, not measured
dynamic counts or the size of artificial vector expansions. Runtime benefit is
unproven for all three.

## Frozen evidence and boundaries

All 1,857 non-documentation files at this base were SHA-256 compared with the
retained native `d7d89d7d` build snapshot, with no differences. The Git diff
between those revisions contains only two documentation files. Both retained
production release compiler binaries independently hash to
`72b0c01e1bf06d2c2e57233c8f2bc213f24f64b75cc8b757aaa640b158ea7681`.
No rebuild, changed toolchain, helper overlay or held spill implementation was
needed. No main/sibling worktree was changed and no subagent was spawned.

Primary inputs are **raw Linux C**, with native preprocessing, Full optimization,
Full selection/allocation, explicit v3 and generic tuning. Retained v1 controls
were reused; fresh v2 IR/text/objects were compiled from the same raw sources,
working directories and definitions. This is not foreign-target IR replay.
The three inspected translation units are QuickJS `quickjs.c`, Lua `lvm.c`,
and SQLite `sqlite3.c`; counts below are not whole-application counts.

The reviewed [native application provenance](provenance/linux-binary128-tc-layout.md#independent-native-provenance-and-application-results)
supplies the original source distributions/configurations. Every retained
distribution file was rechecked against that review: Lua 75, QuickJS 66,
SQLite 4, without mismatch. No app/header/input byte or identity macro changed.
Original flags remain QuickJS `-D_GNU_SOURCE -DCONFIG_VERSION="2024-01-13"
-DCONFIG_BIGNUM -I.`, Lua `-DLUA_USE_LINUX`, and SQLite
`-DSQLITE_THREADSAFE=1 -D_GNU_SOURCE -DLONGDOUBLE_TYPE=double`.

GNU objdump independently decodes the real ELF objects. Emission comments map
Full SSA instructions and block labels to allocated registers, spill reloads
and final instruction windows. Selection/emission source was inspected at
`select.rs` and `x86_64.rs`; these are traces through current selection and
allocation outcomes, not a new allocator instrumentation build. V1/v2 function
byte hashes match for all three inspected functions. V3 differs elsewhere;
the reported candidate shapes/counts agree across all three levels.

All compiler/assembler probes used the shared
`/tmp/keel-perf-20261019.lock` through `suite/fast_opt_locked.py`, coordinated
with optimizer research. Application rebuilds were avoided. Load snapshots
exceeded 1; no timing campaign, profile, throughput or speedup conclusion was
attempted. Existing application correctness receipts are reused evidence;
**this round executes no generated target code**. Release emission legality
remains enabled. A successful object on LinuxHost is not v1-only CPU execution.

## 1. Distinct-destination constant pointer addition with LEA

Owner: `crates/keel-codegen/src/x86_64.rs::binary`, beside the existing
immediate/simple-ALU path. Keep `two_address` and general arithmetic rules
unchanged in the first pilot; this needs no general peephole framework.
Proposed focused regression owner: `crates/keel-codegen/tests/x86_pointer_lea.rs`.

In native Lua `luaV_execute`, block12 contains:

```text
v33 = iconst.i64 4
v34 = ptradd v17, v33
v35 = load.i32 v17
```

The address is not a single-use folded memory operand: the new PC travels
through dispatch while the old PC still supplies the instruction fetch.
Current selection leaves the pointer operation materialized. Full allocation
places the old PC in R15, the new PC in R8; emission/decoding is:

```asm
# object offsets 0x1c12, 0x1c15, 0x1c19 at v3
mov r8, r15
add r8, 4
mov r9d, dword ptr [r15]
```

Proposed local recipe: `lea r8,[r15+4]`, followed by the **same** dword fetch.
The two register instructions occupy seven bytes; the independently assembled
LEA occupies four. This operation recurs in dispatch blocks. A conservative
assembly/SSA screen finds 167 such pairs in `luaV_execute`, 793 in
`JS_CallInternal`, and five in `sqlite3FpDecode`. Across their translation
units the counts are 189, 2,802 and 4,094 respectively, at v1/v2/v3. These are
static copy/add pairs, not execution counts or predicted whole-function deltas.

Eligibility: exactly ordinary pointer-width `Ptradd`, a folded signed disp32
offset, and distinct physical source/destination registers after existing
operand resolution. Keep the coalesced `d == a` ADD path, nonconstant/large
offsets, other arithmetic/types and special/raw regions unchanged. Read or
reload the source before defining the result; retain `def_done` and its
existing spill width/order. Do not combine the pointer spill reload with LEA
or fold/move a user memory access. LEA computes the same modular 64-bit
address without dereferencing it, including noncanonical intermediate bits.
It changes no user memory footprint, ABI, FP operation or MXCSR state.
Ordinary Ptradd has no observable flags result; explicit flag-owned regions
and flag-consuming bundles must remain outside this shortcut.

Matched Clang 23.1.1 compiles the same raw Lua TU at `-O2
-ffp-contract=off -march=x86-64-v3`; its decoded dispatch uses
`lea r15,[r12+4]` before the old-PC dword fetch. This supports the local
recipe, not an attribution of all compiler differences to it. A header-free
scalar control also emits Clang LEA at each strict level; Keel coalesces that
small control and uses ADD, so the actual Lua window is the activation evidence.

Acceptance: explicit v1/v2/v3, all selectors and both allocators, controlled
distinct/coalesced registers, live old PC, spilled source/result, signed disp32
edges and large-offset exclusions; exact text/typed/chunked object parity and
release feature guard. Native unsigned-bit pointer arithmetic oracles must
cover wrap boundaries without dereferencing invalid addresses. A pointer-fetch
fixture must retain its exact four-byte access and guard-page fault position,
including calls/live inputs and unchanged ABI transport. Recount application
objects after implementation; preserve all existing load/address covers.

## 2. Adjacent single-use signed byte load to i32

Owners: `crates/keel-codegen/src/select.rs` for the bounded cover and operand
transfer, `crates/keel-codegen/src/x86_64.rs` for exact-width signed emission.
First scope is only ordinary `load.i8` feeding `sext.i32`.
Proposed focused regression owner:
`crates/keel-codegen/tests/x86_signed_byte_load.rs`.

Native `JS_CallInternal` block60 has `v12600 = load.i8 v9090`, followed by
`v12601 = sext.i32 v12600`. The byte value has one resolved use. Selection
currently leaves both instructions; allocation gives both values EDI:

```asm
# object offsets 0x625b, 0x6262, 0x6265 at v3
mov rax, qword ptr [rbp-4120]
movzx edi, byte ptr [rax]
movsx edi, dil
```

Proposed: retain the pointer reload, then `movsx edi,byte ptr [rax]`.
Seven bytes/two data instructions become three bytes/one in this window.
The following `uext.i64` and ABI/store behavior remain separate controls;
do not silently turn this into a sign extension to i64.

The exact adjacent i8-to-i32 screen has 67 sites in QuickJS and 244 in
SQLite, including three each in `JS_CallInternal` and `sqlite3FpDecode`;
Lua has zero. SQLite's block67 uses an indexed address and loads one character
before a signed comparison with 53. Wider signed-load screens have 247/2/934
QuickJS/Lua/SQLite sites, but those are **not** admissions to this first pilot.
For example Lua's word load/sign-extension is retained as a negative control.

Require one resolved use, same block, adjacency apart from constants, matching
source/debug boundaries, no conflicting cover/constraint, and an ordinary
nonvolatile/non-atomic byte load. Preserve the original effect position: never
cross an intervening load, store, call, barrier, raw region or potentially
faulting operation. Transfer every selected address leaf to allocation at the
consumer and retain existing constant/indexed address behavior. The memory
operand remains exactly m8, even at a protected boundary. Source-address and
result-register aliasing are legal only when the address is read before the
destination write; resolve/reload all address leaves accordingly. Result spills
must retain their ordinary home width/completion, not a one-byte spill shortcut.
No new ISA requirement, ABI, FP or MXCSR behavior is involved.

Acceptance: all 256 byte patterns against an independent signed-i32 oracle,
separate pointer/result aliases, source reuse negatives, word/dword/i64-result
negatives, volatile/atomic/call/barrier/source-boundary exclusions, all selection
and allocation tiers and forced spills. Require exact user-load counts/widths,
both guarded-page edges, canaries, call pressure and typed/text/chunked parity,
plus debug/release legality and existing memory-cover regressions. Application
byte/character paths need finite native correctness before any runtime claim.

## 3. Single-use i32 constant mask compared with zero

Owners: `crates/keel-codegen/src/select.rs` and
`crates/keel-codegen/src/x86_64.rs::compare`; integrate with existing compare
and branch allocation operands rather than adding an assembly rewriter.
Proposed focused regression owner: `crates/keel-codegen/tests/x86_mask_test.rs`.

Lua `luaV_execute` block73 has a tag load/extension, `and` with 15, then
`icmp.ne` with zero. Only the compare uses the masked result. Current selection
retains the AND; allocation reuses EDI. The decoded v3 object at 0x73ee is:

```asm
and edi, 15
cmp edi, 0
setne dil
movzx edi, dil
```

Proposed: `test edi,15`, then the same condition materialization. The mask and
compare become one instruction. **This exact TEST imm32 is six bytes, equal
to the current AND+CMP's six bytes**; instruction reduction is not a code-size
or speed claim. Clang's small matched control chooses a narrower byte TEST;
that width optimization is outside this proposal.

There are 13 adjacent i32 constant-mask/zero-compare screen sites in `lvm.c`,
11 inside `luaV_execute`; the QuickJS and SQLite TUs have 50 and 66. Neither
`JS_CallInternal` nor `sqlite3FpDecode` has this shape. These counts exclude
register masks and i64 masks. Tag checks make the Lua sites plausibly recurrent,
but there is no fresh profile establishing their cost.

Require exact i32 AND, constant i32 mask, one resolved result use, adjacent
Eq/Ne-zero comparison in the same block, compatible source boundaries and no
raw/effect barrier. Consume only ZF for Eq/Ne; do not infer other predicates or
keep flags across an intervening instruction. Preserve any original load and
extension at its exact width/position; this pilot uses register TEST only.
Transfer the unmasked source as an allocation operand to the compare and its
existing branch/select fusion, so it stays live even when the AND is covered.
Preserve the source if live elsewhere, boolean canonicalization and partial-
register rules. No new memory access, ISA level, ABI or FP semantics is needed.

Acceptance: masks zero/all-ones/sign-bit/alternating bits, arbitrary source
patterns and Eq/Ne oracles; shared-mask-result and nonzero-compare exclusions;
all selectors/allocators, aliases/spills, compare materialization and fused
branch/select consumers, live sources across calls and explicit flag barriers.
Require text/typed/chunked parity, baseline legality in debug/release and native
exact-width memory/ABI regression controls before application measurements.

## Revalidated backlog and limits

The outstanding [AVX2 qword-permute/direct-sub proposals](x86-simd-selection-audit-dbe0b1f6.md)
still reproduce with the frozen current compiler. Ordinary C qword reversal
has 41 instructions/207 symbol bytes; its selected shuffle recipe still has
26 instructions, including eight mask stores and two mask reads. Ordinary
i32x8 subtraction has 24 instructions/85 symbol bytes, retaining the four-
instruction staging/subtract/publication window. Matched Clang produces a
single `vpermpd` plus RET for integer-bit reversal and `vpsubd` plus RET for
subtraction; this does not imply FP arithmetic or a new FP contract.

V1/v2 can form frontend wide IR but reject actual wide codegen; AVX-only
(`v3,-avx2`) emits the existing fallback. All three native Full application TUs
contain no 256-bit vector shapes, `vshuffle` or `visub`. Thus these remain
backlog items with focused source applicability, below the three scalar pilots;
they are not two additional implementation recommendations. Their existing
feature/ABI/alias/spill/footprint acceptance gates remain required.

Header-free AArch64 controls cross-emit/decode baseline objects: pointer ADD
already has three operands, while signed-byte conversion remains LDRB+SXTB and
the mask case remains AND+CMP. ARM signed-load/test improvements could be
investigated separately; no native ARM app relevance or execution was established
here, and this round recommends no ARM production change.

Completed PMOVSXDQ/load covers, PMULLD, direct AVX2 adds, vector-address folding
and 128-bit reductions were not repeated. The held compiler-spill reload pilot
is existing static evidence with unproven runtime payoff, not a new finding.
Optimizer research owns range/CFG work; its same-join select experiment remains
held because current CMOV staging can add copies. The native SQLite reciprocal
investigation's [distinct-load boundary](native-sqlite-reciprocal-audit.md)
is unchanged. These pilots do not justify memory alias relaxation, FP
reassociation/contraction, AVX-512 or feature expansion.

## Receipts and reproduction

Artifacts/scripts/probes are outside Git at `/tmp/keel-isel-334d8466/` on LinuxHost,
with scripts, JSON and selected disassemblies copied to the same local path.
`production-audit.json`, `distribution-audit.json` and `identity.json` record
current production/source/binary/object identities and exact ELF symbol extents.
`profile-summary.json`, each profile's `scan.json`/`address-screen.json`, and
`probe-commands.json`/`control-commands.json` retain counts and exact argv.
The two screens are conservative textual diagnostics, not replacement IR
verification or eligibility tests; future implementation must count actual
typed admissions and independently recount changed objects.

Original v1/v3 Full QuickJS/Lua inputs remain at
`/tmp/keel-spill-payoff-d7d89d7d/{quickjs,lvm}-v{1,3}-baseline.{ir,s,o}`;
only **baseline** artifacts were used. SQLite inputs remain at
`/tmp/keel-native-reciprocal-d7d89d7d/v{1,3}-plain-full.ir` and
`v{1,3}-sqlite3.{s,o}`. Fresh v2 files use task-owned prefixes. `controls.py`
derives its raw-source arguments from those qualified command receipts.
`profile_scan.py` reproduces the screens; `identity.py` hashes symbol bytes
without inter-function alignment padding.

`arm-recipe-commands.json` records cross-ARM decoding and an independent
assembler check of the three proposed scalar instructions: Keel strict
v1/v2/v3 and Clang produce identical `.text` bytes. This is **encoding evidence
for handwritten recipes**, not implemented selection, allocation, native
execution, guard-page proof or tests of changed production code.

No Rust command was necessary. Future Rust work must retain the existing pins,
features/profile and default managed cache, using
`python3 suite/fast_opt_locked.py /tmp/UNIQUE.jsonl
cargo COMMAND --offline --locked -j2 ...`. No outputs were deleted,
cache/toolchain settings changed, or broad timing campaign run. The maintained
[ISA](userspace-isa-status.md), [C intrinsic](c-x86-intrinsics.md), and
[GNU vector/ABI](gnu-vector-mode.md) contracts remain authoritative.
