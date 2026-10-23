# Native profile interpretation at fef1af78

Research base: `fef1af784a2179e08fcab49a4995f34a2cea29fe`, 2026-10-23 UTC.
This audit owns only this document. It changes no production code, pass schedule,
ABI, FP contract, ISA feature policy or application. No subagents, compiler
invocations, native oracles or additional profiles were run by this interpreter.
The separately owned fresh runtime campaign supplies the current evidence below.

## Current evidence, rather than historical priority

LinuxHost artifacts are `/tmp/keel-runtime-fef1-r5/`. The fresh release/default-feature
compiler is `/tmp/keel-compile-fef1-r5/frozen/keel`, SHA-256
`28ccf147d9cc46a4df93bd1eed6684c3b105fd397f63dc32c6d4de30b204e33d`.
Raw Linux distributions, generated arrays, 44 translation units and 436 union
native includes are recorded in `provenance.json`. Keel uses explicit Full
optimization, Full selection/allocation and v3, with v1 controls. Final Clang
references use `-O2 -ffp-contract=off -march=x86-64-v3` or baseline x86-64;
initial Clang builds without explicit contraction control are superseded.

The worker qualified all four application variants before sampling, including
Lua/QuickJS fib and sort, maintained/larger SQLite queries, portable Lua tests
and five QuickJS upstream tests. These are finite application checks of the
unchanged compiler, not execution of either proposed transformation.

Two independent v3 captures per workload/compiler supply user-cycle leaf IPs.
The following percentages are **pooled sampled cycle-period shares of all user
samples**, including the small Python launch driver and system libraries:

- Lua fib: `luaV_execute` 84.67%, 4,374 of 5,170 leaf samples; Clang 81.89%.
- QuickJS fib: `JS_CallInternal` 97.55%, 5,671 of 5,816; Clang 99.47%.
- QuickJS sort: `JS_CallInternal` 44.92%, 2,688 of 5,984. Clang attributes
  69.66% to `js_array_cmp_generic` and 16.18% to `JS_CallInternal`; different
  inlining/call boundaries preclude comparing these percentages as speed ratios.
- SQLite maintained queries: `sqlite3VdbeExec` 27.79%, 1,572 of 5,652;
  larger queries: 27.92%, 1,606 of 5,750. Clang shares are 23.11% and 23.13%.

All 28 worker captures, including bounded v1 controls, report zero lost samples;
the measured SMT sibling was idle. Unrelated Trilio/ROS work remained active.
These are descriptive current profiles, not quiet-host timings, instruction
latencies or measured optimization payoffs. Callchain completeness is unproven;
leaf IPs are primary. No compile-cost ranking is established by this audit.

An independent read-only comparison matches all 1,863 tracked non-document files
to the retained `/tmp/keel-mask-review/combined` snapshot. Fresh Full IR hashes
also match the retained pilot IR for each inspected TU. Current assembly is used:
QuickJS/SQLite retained assembly predates the integrated signed-byte load cover.
IR identity alone would not establish current encoding or execution.

## 1. Consume a byte-to-i32 extension at a dense switch

**First bounded implementation proposal.** Owners:
`crates/keel-codegen/src/select.rs`, `x86_64.rs::switch`, and a focused codegen
regression. The first blocker is selection retaining a pure conversion as an
allocated value; generic switch emission then copies that value into its index
scratch. This is neither a frontend recognition failure nor a missing optimizer
round. Do not change switch semantics or the Full schedule.

Current SQLite `sqlite3VdbeExec`, block15:

```text
v76 = load.i8 v75
v77 = uext.i32 v76
switch v77, default(...), [0: ..., ... 184: ...]
```

`v77` has one operand occurrence, at the switch. **The original byte `v76`
has six uses**, including other extensions on successors. It must remain
materialized and live. The current linked binary at function offsets is:

```asm
+0x1cc: movzx esi,byte ptr [rax]  # 0f b6 30
+0x1cf: mov edi,esi               # 89 f7
+0x1d1: mov eax,edi               # 89 f8
+0x1d3: cmp eax,184
```

Counterfactual: keep the same byte read and source ESI, omit allocation of the
single-use widened value, and use `mov eax,esi` directly before the unchanged
range check/table dispatch. This specific window removes one instruction/two
bytes. It is a **handwritten static recipe**, not changed native execution.
The copied EDI IP has 53 samples / 0.940% sampled period in maintained queries
and 39 / 0.679% in larger queries. Those samples justify investigation; they
are not removable-time estimates, especially for a register move.

Matched current Clang dispatch reads `movzbl (%r12),%r13d`, checks 184 and
indexes its relative table using R13 directly. It lacks Keel's intermediate
EDI conversion copy. Different allocation and table-scratch choices explain
the remaining instruction differences; the pilot does not copy Clang's whole
dispatch or discard the bounds check.

Admit only canonical, adjacent `uext.i32` from a materialized ordinary i8 load
to its sole switch use, in one block with compatible complete debug boundaries
and no conflicting covers, machine matches or constraints. Start with existing
dense, zero-minimum switches whose cases fit 0..255. Cover **only the pure
extension**, preserving the original i32 switch width/case/default semantics,
all edge arguments/copies and the shared byte's other uses. Publish the byte as
an allocation operand at the switch. Reuse existing full-register-zero proof
for the materialized load; absent a physical representation proof, retain the
ordinary path or explicitly zero-extend the byte into the index scratch.
Do not simply consume an arbitrary byte register's unspecified upper bits.

Work budget: one constant-size predecessor/producer query at each existing
switch visit, existing saturated use counts and operand tables; no extra
function scan, analysis map, fixed point or pass invocation. A conservative
textual screen finds four shapes each in QuickJS and SQLite, none in `lvm.c`.
These are not typed admissions or eight savings: the QuickJS `JS_CallInternal`
block96 shape already coalesces its extension and has no conversion copy.
SQLite block15 is the demonstrated new opportunity.

Reproducer: unchanged qualified `sqlite3.c`, its original definitions and native
Full command in `commands.jsonl`; inspect `functions/sqlite3VdbeExec.keel-v3.ir`
and the `+0x1cc` binary window. No application source rewrite is required.
Native gate for an implementation: all 256 bytes, default holes and cases near
184/255; shared source uses on multiple successors; argument permutations and
cycles; input/address aliases; registers and forced spills; debug/constraint,
shared-extension and signed/other-width rejection. Require v1/v2/v3 legality,
all selectors/allocators, text/typed/chunked equality, guarded exact-m8 reads
and first-fault order, complete spill homes, ABI/call-clobber controls and
debug/release emission guards. Recount actual selected sites and run the
qualified applications with GCC/Clang links before any runtime claim.

## 2. Omit a coalesced extension after a proven full i32 write

**Second, smaller emission proposal; runtime benefit unmeasured.** Owner:
`crates/keel-codegen/src/x86_64.rs`, scalar `ConvertOp::Uext` emission, with a
focused regression and, if needed, a narrow helper in `fold.rs`. The first
blocker is the unconditional `from == I32` MOV, even when source/destination
already coincide. The IR extension remains semantically necessary; allocation
has already coalesced the values. No optimizer fact or scheduling change is
needed.

Current Lua `luaV_execute` block465, opcode 34 (`OP_ADD`):

```text
v2759 = ushr v6381, 16
v2761 = and v2759, 255          // i32
v2762 = uext.i64 v2761
v2764 = shl v2762, 4
```

Current binary offsets `+0x4350/+0x4353/+0x4359/+0x435b` emit
`shr esi,16; and esi,255; mov esi,esi; shl rsi,4`. The AND's full 32-bit
destination write already zeroes RSI's upper half. Removing only `89 f6`
preserves the exact 64-bit shift input and all flags. There are 31 matching
five-instruction field windows, mapped in source and binary order. The SHR
and AND at this particular site carry 205 and 73 pooled fib samples, 3.976%
and 1.411% sampled period. **None of the 31 self-MOV IPs has a fib sample.**
Their surrounding block's relevance is established; saved runtime cost is not.

Clang's same opcode entry, obtained from dispatch-table element 34, uses
`shr ecx,12; and ecx,4080; lea ...` for the scaled B field. It retains no
equivalent self MOV. Its combined shift/mask and LEA are different arithmetic
selection choices, outside this proposal; Keel's SHR/AND/SHL remain intact.

Initially admit only `uext.i64` from a canonical, materialized i32 AND whose
full-width destination write is proven to reach this use, with equal allocated
source/destination registers. Prefer adjacent producer/conversion, no allocation
edits between them, and conservative exclusions for debug/constraint/cover/raw
boundaries. If that physical full-write proof cannot be established, keep MOV.
Do not infer it merely from the IR value's numeric range. In particular, an
incoming i32 ABI argument can have dirty upper register bits: its self MOV
must remain. Narrow operations, aliases, block parameters, calls, split/spill
transitions without proof and other extension widths remain outside the pilot.

Work budget: a producer lookup and bounded local/boundary checks at the existing
conversion emission, with no scan, persistent register-state tracker or new pass.
Reproducer: original native Full `lvm.c` with `-DLUA_USE_LINUX`, block465 and
the offsets above. Native gate: arbitrary dirty-upper inputs through the AND,
sign-bit/all-one/zero masks, 64-bit consumers and returns; negative ABI-input
controls requiring MOV; live aliases, call clobbers, equal/distinct registers,
spills and allocator edits; v1/v2/v3, every selector/allocator and parity path,
unchanged memory accesses/MXCSR, and debug/release legality. Only the admitted
register instruction may disappear. No memory widening, FP, ABI or feature
change accompanies the proposed omission.

## Important negative findings

High function share is not proof that a tempting local rewrite applies.
QuickJS fib's sampled `JS_CallInternal+0x4ed0` byte read feeds **both** i32 and
i64 unsigned extensions. Its source must remain live; the newly implemented
single-use load covers correctly do not erase it. Other leading QuickJS PCs
land in aggregate transport and compiler stack traffic. This audit establishes
no new legal small removal there and does not revive held byval/private-memory,
spill-cache or archived LEA proposals.

A separate exact load/extension/literal-equality screen finds 48 possible
memory-CMP windows in Lua's VM. All 48 map to actual bytes, but their load/CMP
IPs receive only **one** pooled fib sample. Their large static count does not
justify ranking them as a current hot optimization. No third pilot is proposed.

Keel's Lua preprocessed input contains the genuine computed-goto table; its C
lowering (`irgen.rs`'s shared dispatch/table fusion) forms a switch. Clang keeps
direct address-table dispatch. The sampled `luaV_execute+0x119` is the relative
table base addition; removing that operation or its guard requires a separate
dispatch design/proof, not a new header wrapper, SIMD promise or schedule tweak.

SQLite's larger-query profile gives `sqlite3FpDecode` 3.65%, below the current
Vdbe region. Even current samples do not invalidate the maintained
[distinct-load reciprocal finding](native-sqlite-reciprocal-audit.md): ordinary
`memcpy` escape provenance and the intervening store still prevent equating
the two dividends. No symbol-based memory semantics are inferred.

## Receipts and limits

Worker receipts: `build.json` SHA-256
`eeaea2ce1db9e266580048758e3abe5d0ff586bda700894ce1e43c17d754b0e0`,
`rankings.json` SHA-256
`6774919bec983bdd0348376d3807fcf2ddf6944fb72e6b693cc722242afa8c08`,
and `checks.json` SHA-256
`10e160c1af0a6a5888e8443518728fb1c2e0f090a55ac603db8e992191a7552d`.
Final `function-evidence.json` is
`0b1f563d0ac07518af0279dfd60054706f33c179bac8c32afc836e2e6a5979b2`.
Per-repeat ranks, self/inclusive reports, sampled offsets, compiler/source,
objects, binaries and Clang references remain in the runtime worker's bundle.

Interpreter artifacts are outside Git at `/tmp/keel-gap-fef1af78-r5/` locally,
with a copy on LinuxHost. `current-v3/function-map.json`,
`current-switch-screen.json`, `sampled-field-windows.json`,
`current-u8-memory-compare-screen.json`, `current-u8-cmp-pc-map.json` and their
scripts retain static screens, exact decoded windows and IP counts. Textual
screens do not replace future typed eligibility or native transformed-code
tests. Frontend lowering, optimized IR, native encoding, and original-program
native execution are separate evidence layers; neither counterfactual was
compiled or executed here.

The [ISA](userspace-isa-status.md), [C intrinsic](c-x86-intrinsics.md) and
[GNU vector/ABI](gnu-vector-mode.md) boundaries remain authoritative. Release
legality stays enforced. No AVX-512, FP reassociation/contraction, user memory
motion, feature expansion or benchmark framework change is recommended. Future
Rust gates retain existing pins/features/profile and use
`python3 suite/fast_opt_locked.py /tmp/UNIQUE.jsonl
cargo COMMAND --offline --locked -j2 ...`; compiler/native probes
also serialize through the shared lock. No build output was removed or changed
to improve cache hits, and no sibling work was integrated.
