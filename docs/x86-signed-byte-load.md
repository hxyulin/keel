# Adjacent signed byte load to i32

The x86-64 backend folds an adjacent `load.i8` + `sext.i32` pair into a single
`movsx r32, byte ptr [address]` instead of a separate byte load and register
extension.

```text
v1 = load.i8 v0
v2 = sext.i32 v1      ; -> movsx eax, byte ptr [rdi]
```

## Selection contract

The only admitted shape is an ordinary nonvolatile `load.i8` followed by
`sext.i32`, with exactly one resolved use of the loaded byte. Both results must
remain their own canonical definitions.

- The pair must be in one block, with identical complete source locations
  (file, line, column and scope, including absence of a location).
- Only intervening integer constants at that same location are skipped;
  constrained, matched, condition-owned or removed constants are boundaries.
  Any other instruction separates the pair. Loads, stores, calls, atomics,
  fences, raw regions and potentially faulting operations are never crossed.
- Existing covers, matches, constraints and condition ownership on either
  endpoint reject the pair.
- Raw IR use counts resolve aliases and include uses outside the pair.

The cover runs after existing address and packed-load selection and before
register allocation. It transfers the load's complete selected operand list to
the conversion, preserving every constant-address, indexed-address and symbolic
address leaf. Existing compare/binary/packed memory covers remain separate.

Not affected (these keep their existing paths): word/dword loads, other
extension result widths, unsigned extension, shared loads, volatile loads and
atomic loads.

## Emission

Emission resolves or reloads the address before defining the destination, then
emits `movsx r32, byte ptr [address]`. The memory operand is exactly m8.

- Address/destination aliasing is safe: `movsx edi, [rdi]` and the spilled
  `movsx eax, [rax]` case read the address before writing the destination.
- Indexed spilled leaves use the existing distinct scratch registers.
- The i32 result completes through ordinary `def_done`, including the full
  scalar spill home. There is no byte-sized result-spill shortcut.
- The 32-bit destination write clears the upper 32 bits; a following
  `uext.i64` keeps its separate unsigned-i32 semantics.

The fold adds no feature requirement, FLAGS lifetime, ABI, FP or MXCSR
operation. The original byte access position and width are preserved; no
dereferenceability, `notrap` or alias assumption is introduced. The
[userspace ISA](userspace-isa-status.md), [C intrinsic](c-x86-intrinsics.md)
and [GNU vector/ABI](gnu-vector-mode.md) contracts are unchanged.

## Testing

```sh
cargo test -p keel-codegen --test x86_signed_byte_load
cargo test -p keel-codegen --release --test x86_signed_byte_load
```

The suite covers:

- A selection/allocation matrix over v1/v2/v3, Baseline/Trees/Full selection,
  Fast/Full allocation and normal, one-GPR and empty register pools
  (54 configurations). Text, typed and chunked objects must agree exactly, and
  operand-dependent feature requirements must fit the requested profile.
  Positive chains contain exactly one m8 MOVSX and no residual m8 MOVZX.
- A Clang assembler oracle for direct, address/destination-alias,
  indexed/high-register and negative-displacement encodings.
- Negatives: shared loads, volatile/atomic accesses, other widths/results,
  unsigned conversion, intervening loads/stores/fences/calls, nonconstant
  arithmetic, raw FLAGS ownership, division, cross-block pairs, debug-location
  boundaries and endpoint constraints.

Setting `KEEL_SEXT8_EVIDENCE=<dir>` exports the objects and a Linux C harness.
Run it on x86-64 Linux with `python3 <dir>/run_native.py --cc gcc` (or
`--cc clang`). The harness checks every byte value at both edges of a guarded
page, direct/displaced/indexed addresses, overlapping input/output, output
canaries, calls that physically clobber caller-saved registers, normal and
forced spills, unchanged MXCSR, i32 ABI returns, symbolic loads and the
following `uext.i64` case. Invalid pointers must fault at the expected address
before any later output write. The expected signed value is computed as
`b < 128 ? b : b - 256` to avoid out-of-range casts.

## Application impact

On QuickJS and SQLite, compiled at Full optimization for v1/v2/v3, the fold
activates on 67 loads in `quickjs.c` and 244 in `sqlite3.c`. Summed static
function size drops by 268 bytes (QuickJS) and 969 bytes (SQLite) at each
level. These are static counts, not dynamic execution counts, and no runtime
profitability measurement was made. The QuickJS test suite and SQLite query
suite produce output identical to native references at all three levels with
both GCC and Clang link drivers.
