# Unmasked FP: contract and remaining hardware exposure

This page describes how Keel's raw x86 floating-point intrinsics behave when FP
exceptions are unmasked, and which families still lack unmasked validation. Full
x86 profile support is not claimed, and native Linux signal-delivery validation
has not been completed.

## SSE/AVX contract

[simd-ir.md](simd-ir.md) distinguishes portable numerical FP from architectural
status operations. The raw SSE/AVX conversion, miscellaneous, and comparison IDs
have `Intrinsic::info().pure == false`, so:

- `InstData::is_pinned` pins them;
- `InstData::memory_effects` classifies them as `Barrier`;
- `InstData::may_trap` conservatively permits traps.

These are compiler barriers, not hardware fences. The raw families have no
all-masks-set precondition, so there is no need for duplicate IDs or different
lowering to support an unmasked environment.

The validation design for these families covers:

- live and dead results, with and without optimization, across three selectors
  and two allocators with normal, tiny, and empty register pools (72 production
  ELF objects across two profiles and raw/optimized IR);
- forced single-instruction assembly as an oracle independent of portable FP and
  of Keel's instruction selector;
- all RC/DAZ/FTZ states, with each child process starting with exactly one
  exception unmasked and all sticky flags clear;
- inputs that target IE, DE, ZE, OE, UE, and PE separately, with unsupported
  opcode/class combinations recorded as explicit no-trap observations.

Linux may report the denormal exception as `FPE_FLTUND`, so each record keeps
both the exact `si_code` and the saved MXCSR to distinguish DE from UE. Records
include the faulting instruction address and bytes, signal code, saved MXCSR,
input-fault address, and ordered before/after memory markers. The checks cover a
source-memory fault before the FP fault, an FP fault before the result write, and
the first of two distinct faulting operations in either order. Fault handlers
only capture, write, and exit; they never resume a signal frame, and physical
register assignments are not equated with SSA values. No exception-effect
contract is added to portable FP.

Scope is baseline raw SSE and AVX conversion/misc/compare, including all compare
and AVX rounding controls. A finite same-CPU corpus cannot establish all
instruction/input behavior.

## Other families

These APIs already exist; they are impure and pinned, and their FP effects are
barriers. That does not by itself provide terminal-fault evidence.

| Family | Existing exposure | Remaining unmasked work |
|---|---|---|
| Raw FMA3 | 96 explicit opcode/order/width IDs, separate from portable FMA | Fused principal/source classification, scalar upper lanes, all operand orders, and terminal faults |
| F16C | Four half/single conversion IDs; narrowing has explicit controls | Per-form exception obligations, RC/control behavior, input classes, and terminal faults |
| SSE3/SSE4.1 FP | Six horizontal/addsub, four round, and two dot IDs | Lane/control/width proof, including inactive dot lanes and precision-suppression controls |
| MMX-sized immutable values | Six `x86_sse_mmxvalue_*` conversions lowered through SSE2 with zero-padding/upper-bit merges | Unmasked proof of these value recipes; they promise no physical MMX/x87 register or tag effects |

Physical MMX register/tag transitions are a separate gap. An SSE2
implementation of an immutable packed-I64 value, or the x87-only region, does
not establish physical MMX behavior.

## x87

[f80/numerical.md](f80/numerical.md) requires empty x87 tags, all six CW
exception masks set, and no pending unmasked exception; PC/RC are inherited and
CW is preserved. Its operations perform input FLD, snapshot FNSTSW immediately
after the principal opcode, then store/discard results and restore empty tags
and the entry TOP. Input loading and result storage are therefore distinct
exceptional stages. The returned status includes the temporary physical TOP; it
is not a fault-context ABI.

[basic](f80/basic.md), [binary](f80/binary.md), [compare](f80/compare.md),
[dc](f80/dc.md), [bcd](f80/bcd.md), and [constants](f80/constants.md) also
restrict their raw contracts to a masked CW. [stages](f80/stages.md) separately
allows unmasked exceptions to interrupt ordinary F80 sequences with temporary
operands; that value-level statement does not override the raw numerical
precondition. [legacy-images](f80/legacy-images.md) admits ordinary masked
execution and flag-only unmasked CW seeds without pending exceptions, but
excludes pending-unmasked entry for waiting forms and leaves no-WAIT pending
capture/delivery unvalidated.

The [bounded x87 execution region](f80/unmasked-region.md) is the unmasked x87
path. It uses one appended intrinsic ID and leaves the masked raw APIs
unchanged. Eighty stable principal controls cover the numerical, basic, binary,
compare, integer, FI, BCD, constants, and multi-result families. Entry requires
empty tags at any TOP, no pending exception, exactly one selected CW exception
unmasked, and valid inherited PC/RC. Each load, principal, and result store has
a no-WAIT snapshot followed by an explicit wait. Normal completion resets
exceptions and restores the entry CW and empty TOP. Terminal faults do not
resume, return a result, or run the normal reset.

Its fixture design covers 108 production ELF objects and three forced-assembly
oracle objects, all eight empty TOP values, clear and seeded masked flags,
supported PC/RC states, aliases, live/dead results, forced spilling, optimized
stages, and competing source-memory/FP faults. The terminal record distinguishes
the saved producer FIP/FOP from the delivery RIP.

## Remaining work

- Native Linux execution of the unmasked x87 region packet (216 GCC/Clang object
  runs, eight shards each). Cross-compilation and byte/stage checks do not
  establish kernel delivery, per-form hardware trap obligations, or native
  FIP/FOP attribution.
- Separate forced-assembly terminal proofs for the FMA/F16C, SSE3/round,
  dot-product, and MMX-value families. Raw MMX tag images there are diagnostic
  and are not equated with SSA state.
- Arbitrary nonempty x87 entry and signal-context restart are outside these
  contracts.
