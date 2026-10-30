# x86 code generation

This page states the x86-64 backend's selection rules and their limits. Each
rule is an x86 backend choice: it changes no frontend, optimizer, ABI or FP
semantics, and memory accesses keep their original widths, positions and
alignment unless a section says otherwise.

## AVX selection

AVX is opt-in through `x86::Features { avx: true, ..Default::default() }`, the
feature-aware assembler APIs, `FeatureSelector` and `--target-features avx`.
The compiler selects VEX forms for scalar fadd, fsub, fmul, fdiv and fsqrt, and
for packed f32x4 and f64x2 vfadd, vfsub, vfmul and vfdiv. Binary instructions
write the allocated destination directly, including when it aliases the second
source, which removes two-address copies. Packed arithmetic keeps the written
source order; only AND, OR and XOR sources may swap to reach a shorter VEX2
encoding. There is no FMA contraction or reassociation, and min/max, NaN
handling, sign-bit operations, rounding and saturating conversions keep their
SSE paths. Scalar upper XMM lanes are not IR values.

Functions that use 256-bit vectors emit VZEROUPPER before each call and each
return, except when a 256-bit value is passed as an argument or returned in
YMM. Functions limited to 128-bit VEX instructions do not dirty upper YMM state
and get no VZEROUPPER.

When AVX is enabled without AVX2, 256-bit transport (moves, loads and stores)
stays legal, while integer widening to a 256-bit result goes through the shared
lane legalizer so that every result lane is initialized.

## Wide vector registers

With AVX2, 256-bit integer `viadd` emits `vpaddb/w/d/q destination, left, right`
directly into the allocated result. Wide functions exclude SIMD registers 12-15
from allocation; spilled operands reload into the distinct scratches FS0 and
FS1 before the result is written. VEX integer instructions read both sources
before writing, so either source may share the destination when its lifetime
allows. Strict v1 and v2 reject wide SSA transport.

## 512-bit value transport

With `avx512f`, `i8x64`, `i16x32`, `i32x16`, `i64x8`, `f32x16` and
`f64x8` are 64-byte
SSA values. XMM, YMM and ZMM views share physical registers. Loads, stores,
spills and reloads preserve all 64 bytes, including unaligned addresses.
Frames containing 64-byte aligned slots use a stable, realigned frame base.
Per-function target features determine legality during optimization and codegen.
Accepting a 512-bit value does not enable every AVX-512 arithmetic operation;
portable operations without a native recipe use lane legalization.

AVX512F lowers `i32x16` and `i64x8` add, subtract, bitwise AND/OR/XOR,
scalar splats and 512-bit vector bitcasts directly to ZMM instructions.
Dword multiplication, dword/qword signed and unsigned min/max, and variable
shifts also use native AVX512F instructions. Shift counts are masked per lane
to retain the IR modulo-width semantics. Qword multiplication uses lane lowering. These
recipes require no AVX2 permission. Three-operand arithmetic accepts overlapping
inputs and destinations; forced spills use separate reload scratch registers.
Byte and word arithmetic continues through portable lane lowering.

`f32x16` and `f64x8` add, subtract, multiply, divide and square root use
unmasked packed ZMM instructions under AVX512F. Source order matches the
scalar operations, including subtraction and division. These recipes do not
require FMA permission or combine multiply and add. Other floating operations
use portable lane lowering. C accepts `float` and `double` vectors with
`vector_size(64)`; `immintrin.h` defines `__m512` and `__m512d`.

## Shuffles

AVX512F selects arbitrary two-source dword and qword shuffles across all 512
bits, including floating vectors. Indices address the concatenation of both
inputs. Native quarter extraction and insertion preserve scalar lane bits,
including NaN payloads, without requiring SSE4.1 or AVX2. Scalar insertion stages
one 128-bit quarter before replacing it in the original ZMM value.

The 128-bit shuffle selector uses SSE2 encodings for:

- every one-source dword permutation;
- two-source dword shuffles that take the two low lanes from one source and
  the two high lanes from the other, in either order;
- alternating-source dword shuffles, using two `pshufd` and a `punpckldq`;
- all qword permutations, keeping the two-source `shufpd` window;
- byte and word permutations that keep aligned groups of four bytes.

Vector legalization and final emission share this classification. Destructive
two-source shuffles stage both inputs in reserved scratch. The recipes move
bits only, so NaN payloads, signed zeros and MXCSR are unchanged. Other
cross-source dword masks and byte or word masks that split dword groups use the
lane legalizer, and 64-bit vectors keep their existing path.

With AVX2, equal-operand `i32x8` and `f32x8` shuffles whose 128-bit halves use
the same four-dword control select `vpshufd ymm, ymm, imm8`; for example
`[3,2,1,0,7,6,5,4]` selects immediate 27. Cross-half masks, different half
controls, distinct sources and other element widths use the general `vpshufb`
path. AVX without AVX2 uses the lane fallback.

## Vector lowering and calling convention

128-bit values use packed SSE/SSE2 operations or exact scalar lane expansion.
With AVX, 256-bit values use packed operations, conversions, integer
reductions and lane recipes, with scalar legalization where needed. XMM and
YMM views share physical registers. Spill homes and copies keep exact widths.
Functions that need 32-byte local alignment realign the frame and use a
reserved stable base; incoming arguments and unwind state keep the original
frame reference. SysV has no callee-saved SIMD registers, so values live
across calls are preserved by the caller.

Named 128- and 256-bit arguments share SIMD0–7 with scalar floating
arguments; overflow slots are 16- or 32-byte aligned, and results use SIMD0.
Unnamed 128-bit variadic values use the SIMD save area when available, then
16-byte aligned overflow storage; unnamed 256-bit values always use 32-byte
aligned overflow storage. 64-bit vectors are rejected in `va_arg`.
Single-vector aggregate wrappers use the SSE/SSEUP class, MEMORY aggregates
use byval/sret, and GNU empty-member 256-bit layouts that Clang and GCC
classify differently are rejected.

Gather and sign-masked memory operations keep inactive-lane fault
suppression. The byte-mask non-temporal store needs a fully writable 16-byte
destination and emits `SFENCE`. Architectural intrinsics require their
declared features; see [x86 intrinsics](intrinsics.md).

A select with a canonical comparison mask and an exact all-zero splat on
either arm emits AND or ANDNOT, reading operands before defining an aliased or
spilled result. Loaded arbitrary masks keep lane-truth normalization, and
floating negative zero keeps the general bit-preserving select.

## SIMD arithmetic and comparisons

The unsigned `i8x16` compare builds its sign-bit bias with
`pcmpeqd; psllw 7; packsswb`, which yields `0x80` in all sixteen lanes in three
instructions.

`vimul` on `i32x4` selects `PMULLD` when the effective feature set includes
SSE4.1 (v2, v3, or v1 with `+sse41`). Otherwise it uses two `PMULUDQ`
operations, dword shuffles for the even and odd products, and `PUNPCKLDQ`. The
low 32 bits of each product give the IR's modular result for both signed and
unsigned operands.

With SSE4.1, `swiden_low.i64x2` from `i32x4` selects one register
`pmovsxdq`. The source is read before the destination is defined, and spilled
inputs and results use their full 16-byte homes. This selector never folds a
load; the load covers below keep priority.

`vreduce_add` on i64x2 and i32x4 uses SSE2 only. i64x2 swaps qwords with
`pshufd 0x4e` and adds with `paddq`. i32x4 adds opposite halves with the same
shuffle, then adjacent dwords with `pshufd 0xb1`, and extracts lane zero.
Wrapping addition makes this tree equal to a sequential sum for either
signedness. The input is staged in reserved xmm14 and permutations use xmm15,
so allocated inputs are never modified; allocations that overlap SIMD
registers 13 to 15 are rejected. Other reductions, i8 and i16 lanes, 64-bit and
256-bit vectors and FP reductions keep their existing lowering.

## BMI selection

With BMI2, nonconstant SHL, USHR and SSHR use SHLX, SHRX and SARX, so the count
stays in its allocated GPR instead of moving to RCX. The hardware masks 32- and
64-bit counts. For i8 and i16, the count is copied to reserved RCX and masked
modulo 8 or 16, and right-shift inputs are zero- or sign-extended in reserved
RAX. The destination may alias either source. Constant 32- and 64-bit ROTL and
ROTR use RORX; a left rotation negates the masked count. Constant shifts and
variable or narrow rotates keep the baseline lowering.

With BMI1, CTZ uses TZCNT. For i32 and i64 the hardware returns the width for
zero. For i8 and i16 a sentinel bit above the value makes zero return the
narrow width. i128 keeps its existing path. These selections never replace a
compare or consume arithmetic flags.

ANDN and BLSI/BLSMSK/BLSR fusion are not selected. Allocation follows the
unfused IR use graph, so reading through a producer could read a register that
was already reused. The compiler does not select MULX, PEXT or PDEP.

## Scalar compare and select

A single-use scalar comparison can feed a scalar integer or pointer select
directly, without materializing its Boolean result. The pair must be adjacent,
or an integer comparison may cross one i32/i64 negation. The latter covers
absolute-value selects and keeps the comparison inputs live until the select.
It does not cross memory accesses, source-location or allocation constraints,
and is disabled in functions with inline assembly. Memory-operand comparisons
keep their existing staging boundary.

## Memory operand folding

Tree and Full selection fold some loads into their consumer. Every fold keeps
the load's byte width and its position in the IR effect order. None assumes
alignment, `notrap` or dereferenceability, and none crosses another
instruction with effects. Volatile and atomic accesses, shared results,
cross-block pairs and functions with source locations or debug variables keep
the separate load. Baseline selection and AArch64 are unchanged.

- **Integer compare RHS.** An adjacent single-use i8, i16, i32 or i64 load that
  is the right operand of an integer comparison becomes
  `cmp register, [address]`. Operand order, and so every signed and unsigned
  condition, is kept. Compare/branch fusion moves the address leaves to the
  branch.
- **Arithmetic RHS.** `iadd`, `isub`, `and`, `or` and `xor` at i8 to i64, and
  `imul` at i16 to i64, may take an adjacent load as the right operand.
  Subtraction keeps `lhs - loaded_rhs`. Narrow results are zero-extended
  explicitly. When the result and lhs coalesce, that register is the
  accumulator; otherwise the accumulator is reserved RAX and the result is
  copied only after the memory read, so a destination that overlaps an address
  leaf cannot destroy the address first. A load shared by two consumers stays
  one materialized read.
- **Indexed addresses.** A single-use same-block `ptradd` with an i64 offset
  becomes `[base + index * scale]` at a scalar load or store of 1, 2, 4 or 8
  bytes. A single-use i64 shift supplies scales 1, 2, 4 or 8 after masking its
  count modulo 64. Address reloads use reserved RCX and RDX. Atomic, 128-bit
  and vector accesses are excluded.
- **Constant-offset vector loads.** An ordinary `load.i32x4` whose address is
  a sole-use, same-block `ptradd` with an i64 constant that fits signed disp32
  folds to `movdqu xmm, [base + disp]`. The AVX2
  `load.i32x4; swiden_low.i64x4` cover uses the same address as one m128
  `vpmovsxdq`. Other vector shapes, 256-bit loads, stores, `i128` (two m64
  accesses) and indexed vector addresses are excluded.
- **Mask tests.** An i32 `and` with a literal mask whose only use is an Eq or
  Ne compare against zero in the same block becomes `test r32, imm32`, feeding
  SETcc, Jcc or CMOV. Only constants may sit between them.
  `Selection::fold_x86_mask_tests` runs after the selector and before
  compare/branch/select fusion. Shared AND results, other predicates, nonzero
  comparisons, register masks and other widths keep the AND.
- **Signed byte loads.** An ordinary `load.i8` followed by `sext.i32`, with one
  use of the byte and identical source locations, becomes
  `movsx r32, byte ptr [address]`. The address is read before the destination
  is written, so they may alias.
- **Signed dword widening loads.** With SSE4.1, the adjacent chain
  `load.i64; vsplat.i64x2; bitcast.i32x4; swiden_low.i64x2` with single-use
  intermediates becomes `pmovsxdq xmm, m64`.

## Code models

`--code-model MODEL`, `--code-model=MODEL` and `-mcmodel=MODEL` select how
symbol addresses are materialized on `x86_64-unknown-linux-gnu`. They do not
change C layout, the ABI, CPU features, optimization or unwind emission.

| Model | Symbol address materialization | Contract |
|---|---|---|
| `pic` (default) | RIP-relative definitions; GOT-bound external addresses | Position-independent userspace. Relative references must fit their encoded reach. |
| `small` | Zero-extended 32-bit absolute addresses (`R_X86_64_32`) | Static low-address image; addresses fit unsigned 32 bits and direct calls stay reachable. |
| `kernel` | Sign-extended 32-bit absolute addresses (`R_X86_64_32S`) | Static image in the top 2 GiB; direct calls stay reachable. |

`small` and `kernel` reject `--link-intent shared`, and `kernel` rejects
userspace TLS. `medium` and `large` are rejected; a large model would also need
distant calls, constants and tables. Linkers diagnose relocation overflow
rather than truncating.

## Two-word aggregate wrappers

The backend keeps small straight-line wrappers in SSA when their System V ABI
is identical to a pair of integer words, which removes argument snapshots and
return-buffer copies. Admission requires a single block, one direct call and a
complete two-i64 result in rax and rdx. Byval layouts must be exactly
`[2 x i64]` or `{i64, i64}`, and every original argument location must equal
the proposed i128 location, including register-pair rollback, stack offsets
and stack alignment. Only private stack homes and incoming byval copies may be
accessed, and every observed word must be initialized. External memory
accesses, captured addresses, several or indirect calls, variadic calls, frame
observers and debugger-visible functions use ordinary lowering. The temporary
i128 function never replaces the public module function.

## Frames

The SysV backend omits the frame for `cc(c)` SIMD leaves and `cc(fast)`
leaves. Scalar C leaves keep their frames, except under the size goal below.
An eligible function needs no stack storage, incoming stack arguments,
callee-saved registers, special-purpose parameters, TLS addresses, debug files
or exception personality. Calls, including helper calls for wide division and
software FMA, intrinsics, inline assembly and exception instructions keep the
frame.

A single-block function that contains only a direct `return_call` with an
identical signature, where each argument forwards the matching entry
parameter, also omits its frame and argument moves. Debug files, variadic
signatures, `returns_twice` and exception personalities keep the normal path.

A frameless function leaves `rsp` and `rbp` unchanged. Its CFA is `rsp + 8`,
with the return address at `CFA - 8`, and the assembler still emits an FDE.
Wide-vector stack alignment and vector spills keep frames.

Parallel copies use reserved `r11` for integers and `xmm14`/`ymm14` when SIMD
locations take part, keeping every lane through a copy cycle. Memory-to-memory
transfers use `xmm15`/`ymm15`. Ordinary 128-bit vectors do not reserve a stack
copy temporary; wide-vector, F80 and intrinsic recipes keep one.

## Size emission

`keel cc -Os` and `keel compile -Os` select size-oriented x86 emission in
addition to the Size IR pipeline:

- Omit implicit 16-byte function alignment and 32-byte loop alignment.
  Explicit function alignment stays a lower bound.
- Omit RBP setup in scalar C leaves with no calls, stack storage,
  callee-saved registers, stack arguments, debug information or stack
  observers.
- Omit unused call-alignment padding in leaves with no calls, local storage,
  stack observers, TLS helper, debug information or stack realignment.
- Use one-byte `leave` instead of `mov rsp, rbp; pop rbp` when no other
  callee-saved registers need restoring.
- Share one return epilogue when the function saves registers and has
  several returns. Each redirected return replaces at least seven bytes with a
  jump of at most five.

Leaf-frame removal can shorten frame-pointer backtraces, and these choices can
cost runtime. Call sites keep SysV alignment. The last `-O` preset selects the
codegen goal; `--opt-tier` and `-p` override IR passes independently. For
example, `-Os --opt-tier none` keeps size emission, whereas `--opt-tier size`
alone changes only IR passes. The goal is not serialized in IR; apply it again
when compiling an IR file. Library callers set `EmissionOptions::goal` to
`CodegenGoal::Size`; the default is `CodegenGoal::Speed`. AArch64 has no
goal-specific emission.

## Tests

```sh
cargo test -p keel-codegen --test x86_scalar_selection x86_c_leaf::
cargo test -p keel-codegen --test fast_cc
cargo test -p keel-codegen --test vector_x86 x86_unsigned_byte_compare::
```
