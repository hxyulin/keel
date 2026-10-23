# Unmasked FP proof and remaining hardware exposure

`tests/x86_unmasked_fp.rs` exports an independent Linux x86_64 proof packet;
its subtree README describes the exact finite inventory and acceptance protocol.
The exporter does not run target code. Root LinuxHost native execution and fresh
review are pending; fullprofile remains false. Local compilation alone does not
establish Linux signal delivery or close this goal.

## Audited SSE/AVX contract

`docs/simd-ir.md` distinguishes portable numerical FP from architectural status
operations. The conversion, miscellaneous and comparison raw SSE/AVX IDs already
have `Intrinsic::info().pure == false`. `InstData::is_pinned` therefore pins them;
`InstData::memory_effects` classifies these families as `Barrier`, and
`InstData::may_trap` conservatively permits traps. These are compiler barriers,
not implicit hardware fences. The source contains no all-masks-set precondition
for these raw families. There is no contract reason to add duplicate IDs or
change production lowering merely to test an unmasked environment.

The new fixture checks live and dead results independently of optimization,
three selectors and two allocators, with normal, tiny and empty pools. Two
profiles and raw/optimized IR produce 72 production ELF objects, not 108.
Forced single-instruction assembly supplies same-hardware observations, independent
of portable FP and of the compiler's instruction selector. All RC/DAZ/FTZ states
are exercised. Each child starts with exactly one exception unmasked and all
sticky flags clear. Input classes target IE, DE, ZE, OE, UE and PE separately;
unsupported opcode/class combinations remain explicit no-trap observations.
Linux may classify the denormal exception with `FPE_FLTUND`; the record retains
both exact si_code and saved MXCSR to distinguish DE from UE.

The proof records the exact fault instruction address and bytes, signal code,
saved MXCSR, input-fault address, and ordered before/after memory markers. It
checks first source-memory fault before FP, first FP before result write, and
first of two distinct faulting operations in either order. Fault handlers only
capture, write and exit. The proof neither resumes a signal frame nor equates
physical register assignments with SSA values. It adds no exception-effect
contract to portable FP and demands no universal numerical payload.

Scope is baseline raw SSE and AVX conversion/misc/compare families, including
all compare and AVX rounding controls. Separate unmasked evidence remains for
FMA/F16C, MMX-value conversion and SSE3/4 horizontal/dot/round families. A finite
same-CPU oracle corpus cannot establish all possible instruction/input behavior.
No unmasked FMA, F16C, MMX or additional SSE3/4 operation is accepted merely
because it occurs in the base corpus; each needs explicit exposure and proof
outside this packet's sealed inventory.

## Other unmasked exposure and proof gaps

The source already exposes the following architectural APIs. Their signatures
are impure/pinned and their FP effects are barriers; their presence does not
supply terminal-fault evidence. No duplicate IDs are needed simply to exercise
those existing unmasked effects.

| Family | Existing exposure | Remaining unmasked work |
|---|---|---|
| Raw FMA3 | 96 explicit opcode/order/width IDs, separate from portable FMA | Independent fused principal/source classification, scalar upper lanes, all written operand orders and terminal faults |
| F16C | Four half/single conversion IDs; narrowing has explicit controls | Per-form exception obligations, RC/control behavior, input classes and terminal faults |
| SSE3/SSE4.1 FP | Six horizontal/addsub, four round and two dot IDs | Independent unmasked lane/control/width proof, including inactive dot lanes and precision-suppression controls |
| MMX-sized immutable values | Six `x86_sse_mmxvalue_*` conversions lowered through SSE2 with zero-padding/upper-bit merges | Unmasked proof of these value recipes; they promise no physical MMX/x87 register or tag effects |

Genuine MMX register/tag transitions are a separate exposure/contract gap. An
SSE2 implementation of an immutable packed-I64 value, or the new x87-only
region, cannot establish physical MMX behavior. Base-corpus acceptance does not
close any of these unmasked rows.

## Bounded x87 extension and remaining native proof

`docs/f80/numerical.md` explicitly requires empty x87 tags, all six CW exception
masks set and no pending unmasked exception. PC/RC are inherited and CW preserved.
Its single-step numerical, multi-result and signed integer-memory operations
include input FLD effects, snapshot FNSTSW immediately after the principal opcode,
then store/discard results and restore empty tags and entry TOP. Thus exceptional
input loading and result storage are physically distinct stages. Their returned
status includes temporary physical TOP; it is not a fault-context ABI.

`basic.md`, `binary.md`, `compare.md`, `dc.md`, `bcd.md` and `constants.md` also
bound their raw value contracts to masked CW. `stages.md` separately permits
unmasked exceptions interrupting ordinary F80 sequences with temporary operands;
that value-level statement does not override the raw numerical precondition.
`legacy-images.md` admits ordinary masked execution and flag-only unmasked CW
seeds without pending exceptions, but excludes pending-unmasked entry for waiting
forms and leaves no-WAIT pending capture/delivery unproved. The SSE/AVX packet changes
none of those owner contracts and supplies no x87 unmasked proof.

The additive [bounded x87 execution region](f80/unmasked-region.md) provides
actual IR/codegen exposure through one appended ID, preserving the preceding
678 IDs and masked raw APIs. Eighty stable principal controls cover numerical,
basic, binary, compare, integer, FI, BCD, constants and multi-result families.
Entry requires empty tags at any TOP, no pending exception, exactly one selected
CW exception unmasked and valid inherited PC/RC. Each load, principal and result
store has a no-WAIT snapshot followed by an explicit wait. Normal completion
resets exceptions and restores entry CW and empty TOP. Terminal faults do not
resume, return a result or execute the normal reset.

The dedicated fixture now exports 108 production ELF objects and three independent
forced-ASM oracle objects. Its finite domains include all eight empty TOP values,
clear/seeded masked flags, supported PC/RC states, aliases, live/dead results,
forced spilling, optimized stages and competing source-memory/FP faults. The
terminal record distinguishes saved producer FIP/FOP from delivery RIP and
retains no-WAIT images plus memory markers. The exact roster, row hashes and
limits are in [the implementation report](f80/unmasked-region-20261023.json).

The remaining proof is genuine Linux execution of all 216 GCC/Clang object runs
and all eight mandatory shards per run, followed by fresh independent review.
Local cross-compilation and byte/stage checks do not establish kernel delivery,
per-form hardware trap obligations or native FIP/FOP attribution. Existing masked
contracts remain intact; arbitrary nonempty entry and signal-context restart are
outside this bounded contract. The remaining x87 proof stays part of the full
goal, and fullprofile remains false pending root acceptance.

The [R2 receipt repair and next-stage plan](x86-unmasked-fp-r2.json) records the
shared manifest/environment binding finding and its focused regressions. Root
owns native acceptance of each newly sealed packet.

The next [bounded FMA/F16C stage](x86-unmasked-fused.json) implements a separate
forced-ASM terminal proof for the existing typed APIs. Its native/review result
is pending root. The [three bounded legacy/MMX-value stages](x86-unmasked-legacy.json)
provide separate SSE3/round, all-control dot-product and immutable MMX-value
packets. Their native/review gates remain pending root. Raw-MMX tag images are
diagnostic; they are not equated with candidate SSA state.
