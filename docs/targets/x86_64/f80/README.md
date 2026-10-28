# F80 support

F80 is an x87 extended value, never an alias for F64. `Type::bytes()` is
10, the exact load/store payload. SysV object layouts give it size and
alignment 16; six padding bytes are not part of the value. Existing Fconst
and its u64 payload remain unchanged. `F80const { significand, sign_exp }`
and `fconst.f80 0x3fff, 0x8000000000000001` preserve all payload bits,
including signed zeros, NaNs and noncanonical encodings. Decimal conversion
is deliberately absent until a correctly rounded converter exists.

The IR and text format accept F80 losslessly. Backends reject F80 forms they do
not implement, including in foreign signatures and aggregate layouts. AArch64
always rejects this foreign format.

## FP environment contract

The x86 design assigns a stack home to every F80 SSA value and uses short
x87 sequences. No x87 value survives an IR instruction except an ABI result
in ST0. Calls consume return ST0 immediately, including unused results.
Returns expose exactly ST0; calls and tail jumps otherwise have empty x87
stacks. Payload moves use integer copies to preserve noncanonical
encodings and avoid unintended exceptions. Arithmetic/conversions may raise
x87 exceptions and obey the inherited x87 control word (precision control,
rounding control, masks); the default SysV environment has extended precision.
No change to MXCSR or the x87 control word is implicit. Unmasked exceptions may
interrupt a sequence with temporary stack operands present, as in C-generated
code; completed sequences balance TOP. This is a value-level compiler contract,
not an exact raw x87 instruction/environment or MMX-state implementation.

## ABI

ABI lowering distinguishes X87/X87UP classification from argument placement:
long double and its wrappers are MEMORY arguments but ST0 results. Mixed
or larger aggregates use MEMORY. No synthetic XMM return is permitted.
Varargs long double consumes an aligned 16-byte overflow-area object and
never consumes gp_offset/fp_offset. Tail calls require the existing incoming
stack-area checks and return-layout agreement.

## Supported x86 operations

The x86 backend implements stack homes, raw constants and exact-width memory
transport, scalar and X87/X87UP aggregate ABI transport, all scalar float
arithmetic/unary operations, IEEE min/max, copy-sign, explicit FMA through
`fmal`, ordered/unordered comparisons, selects, block arguments/loops, scalar
direct/indirect calls and tails, and SysV overflow-area varargs. F32/F64
extension/truncation uses x87 loads/stores; integer conversions preserve the
existing saturating/truncate/NaN-zero contract. Integer conversion and explicit
rounding temporarily select the required RC and restore the original CW.
Unsigned u64 construction normalizes its exact payload with integer operations;
u64 results in [2^63,2^64) are the explicit significand directly. Neither path
passes through binary64 or depends on reduced precision control.

The F80 homes reserve 16 bytes each; control-word staging reuses the existing
16-byte parallel-copy temporary between instructions. There is no metadata
operation group or dedicated frame-save slot. Existing aggregate tail
restrictions, i128/float conversion restrictions, and AArch64 rejection remain.

## Related APIs

Raw x87 operations and owned-stack contracts are documented in the other F80
references in this directory. They do not follow automatically from scalar
F80 value support. Decimal conversion and AArch64 F80 lowering are unsupported.

## Raw x87 references

| Area | Contract |
| --- | --- |
| Values and arithmetic | [Constants](constants.md), [basic operations](basic.md), [binary arithmetic](binary.md), [comparisons](compare.md), [numerical operations](numerical.md) |
| Memory and encodings | [DC forms](dc.md), [BCD](bcd.md), [legacy images](legacy-images.md) |
| Machine-state ownership | [Owned stack](owned-stack.md), [unmasked regions](unmasked-region.md) |
