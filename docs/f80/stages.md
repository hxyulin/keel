# F80 implementation stages

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

## Implemented x86 stage

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

## Custom (sparse) allocation plans

Sparse F80 functions expand every original spill unit from eight to sixteen
bytes and rewrite the whole allocation consistently: all fallbacks, parameter
bindings, instruction bindings, and before/after/edge transfers. Stack home
identity and custom dataflow are preserved. Copy-width propagation follows F80
transfer components through otherwise unbound temporary stack slots; every such
home receives 128-bit internal copies and two units of frame capacity. External
payload loads/stores remain exactly ten bytes. Frame overflow and invalid
original capacities are rejected before emission.

The following fail explicitly until type-aware register transitions are
supported: sparse F80 register bindings, transfer components crossing
registers, register fallbacks with sparse transfers, and transfer homes shared
with wide vectors. Allocators that try to put F80 in a register fail closed.

When a copysign or equal min/max result shares its second input's home, the
second sign/exponent is captured in reserved RCX before any payload write; the
copy's RAX/RDX scratch cannot destroy that capture. Non-overlapping plans keep
the ordinary instruction sequence.

Frame addresses default to the 64-bit scalar spill transport width. Only
`copy_slot` applies the conservative untyped full-copy metadata. Typed FP/vector
reloads and definitions choose their own widths; GPR, pointer and integer-half
accesses stay 64-bit regardless of a home's earlier F80 use. Full stack
transfers still copy the complete F80 payload in their 16-byte capacity.
Dedicated AVX256 constant staging uses its full copy width.

## Testing

```sh
cargo test -p keel-codegen --test f80 --test f80_allocation --test f80_coalescing
```

The tests cover 108 configurations (v1/v2/v3; baseline/trees/full selection;
fast/full allocation; three register-pressure pools; optimized/raw IR), and
require typed and text assembly to produce byte-identical ELF. A header-free
Clang Linux cross oracle independently uses stack MEMORY arguments and ST0
returns, including the single-F80 aggregate. `f80_allocation` covers custom
sparse plans (redundant parameter/instruction bindings, full-payload
before/after/edge copies, transit-only temporary homes, mixed
integer/pointer/F80 frame mapping, and negative register/capacity cases).
`f80_coalescing` covers first- and second-input result sharing and temporal
reuse of homes across F80/integer/F32/F64/pointer values.

Setting `KEEL_F80_EXPORT_DIR="$PWD/target/f80-packet"` makes these tests also
export object files and a C driver for native execution with GCC and Clang on
x86-64 Linux. The driver checks 8/10/16-byte format distinctions,
extrema/subnormals, precision beyond F64, arithmetic, NaN and signed-zero
comparisons/min/max, conversions under all RC modes, precision control, CW
restoration, TOP balance at external calls and after returns, spills, loop/phi
swaps, direct/indirect/tail calls, aggregate/varargs interoperability,
unaligned/overlapping exact-width payload moves, padding sentinels, guard-page
memory boundaries, a dead volatile-load fault, and an unmasked divide fault.
The built-in configurations and the custom sparse-plan identity objects have
passed this native run with both GCC and Clang; the coalescing objects have not
yet been run natively.

## Notes for C test drivers

- Under reduced x87 precision control, a C caller expression such as
  `0x1p63L+1` can be rounded before the call (Clang emits `flds` of 2^63 then
  `fiaddl` of 1). Use the exact literal `0x1.0000000000000002p63L` to pass the
  payload with significand `0x8000000000000001`, exponent `0x403e`.
- To observe x87 state at callback entry, sample status from a raw assembly shim
  before the C body runs. Inline `fnstsw` in a C callback can observe the body's
  own return-value load (`fldt 8(%rsp)`) rather than the ABI entry state. Such a
  shim must preserve AL (the variadic register-save count), the argument stack,
  and alignment, and should carry CFI for any temporary push.
