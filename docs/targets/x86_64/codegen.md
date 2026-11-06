# x86 code generation

This page states the x86-64 backend's registers, frame, selection rules and
their limits. Each rule is an x86 backend choice: it changes no frontend,
optimizer, ABI or FP semantics, and memory accesses keep their original
widths, positions and alignment unless a section says otherwise. The backend
is `crates/keel-codegen/src/x86_64.rs` and the modules under
`crates/keel-codegen/src/x86_64/`. Shared selection and allocation rules are in
[IR and backend design](../../compiler/design.md#instruction-selection).

## Registers and frame

The module docs of `crates/keel-codegen/src/x86_64.rs` give the register
roles and the frame layout. The rules that differ from AArch64:

- With 16 general registers, never allocating argument registers would leave
  five. So rsi, rdi, r8 and r9 are allocatable, and a call's arguments move
  into place as one parallel copy (`f(b, a)` with `a` in rdi and `b` in rsi is
  a swap, broken with r11). Several results move the same way, into their
  return registers before `ret` and out of them after a call.
- Instructions are two-address (`add d, b` overwrites `d`), and many use fixed
  registers (`div` in rdx:rax, shift counts in cl). rax, rcx and rdx are
  therefore never allocated and serve as scratch. r10 holds indirect call
  targets and is never allocated.
- The fixed frame is addressed from rbp, and only outgoing arguments from
  rsp. A dynamic allocation that moves rsp leaves every slot's address alone.
- Under the default `pic` [code model](#code-models), code is position
  independent. Defined symbols are `rip`-relative, and others go through the
  GOT (`@GOTPCREL`) and the PLT for calls. The `small` and `kernel` models use
  absolute addresses instead. Thread-locals use local-exec (`fs:` + `@tpoff`)
  when defined in the module, and initial-exec (`@gottpoff`) otherwise.
- The default CPU level is v1; `--cpu-level` and `--target-features` select
  more. Traps are `ud2`.
- Not supported: exceptions, i128↔float conversions, i128 overflow multiply
  and `mulhi`, and `switch` on i128.

The register-assignment design keeps r10 and r11 reserved, for 12
allocatable general registers. Frameless scalar C leaves (beyond the size goal
below) land only if the suite shows no retired-instruction regression.

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
to retain the IR modulo-width semantics. Qword multiplication uses native
`vpmullq` with AVX512DQ and lane lowering with AVX512F alone. These recipes
require no AVX2 permission. Three-operand arithmetic accepts overlapping
inputs and destinations; forced spills use separate reload scratch registers.
Byte and word arithmetic continues through portable lane lowering.

`f32x16` and `f64x8` add, subtract, multiply, divide and square root use
unmasked packed ZMM instructions under AVX512F. Source order matches the
scalar operations, including subtraction and division. Negation, absolute
value and copy-sign operate on sign bits, preserving NaN payloads. Ceiling,
floor, truncation and ties-to-even rounding use fixed rounding controls.

Portable 512-bit floating minimum and maximum propagate NaNs. Minimum
selects negative zero on a zero tie; maximum selects positive zero. Comparisons
produce canonical all-ones or zero integer lanes. Floating selects treat any
nonzero integer mask lane as true and preserve the selected value's bits.
These recipes use AVX512F without AVX512DQ or AVX2 permission.

Explicit 512-bit `fma` uses AVX512F and rounds once. It does not require the
independent FMA permission used by scalar, 128-bit and 256-bit instructions.
Ordinary multiply and add remain separate. Other floating operations use
portable lane lowering. C accepts `float` and `double` vectors with
`vector_size(64)`; `immintrin.h` defines `__m512` and `__m512d`.

Signed and unsigned dword widening from `i32x8` or `i32x16` to `i64x8`
selects the low or high eight source lanes. Qword narrowing from `i64x8` to
`i32x8` or `i32x16` truncates each lane; unused `i32x16` lanes become zero.
These conversions require AVX512F without AVX2 or AVX512VL.

Integer `i32x16` and `i64x8` reductions use full-width shuffle trees for
wrapping addition, bitwise AND/OR/XOR, and signed/unsigned minimum/maximum.
Every lane contributes; these recipes require AVX512F without AVX2.

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

Named 128-, 256- and 512-bit arguments share SIMD0–7 with scalar floating
arguments. Overflow slots align to the vector width, and results use SIMD0.
Unnamed 128-bit variadic values use the SIMD save area when available, then
16-byte aligned overflow storage. Unnamed 256- and 512-bit values always use
32- and 64-byte aligned overflow storage. 64-bit vectors are rejected in
`va_arg`.

A single-vector aggregate wrapper uses one XMM, YMM or ZMM register when
registers remain available. MEMORY aggregates use byval/sret. Wide wrappers
with empty members require explicit `x86_vector_abi(gnu)` or
`x86_vector_abi(clang)` when the producers disagree: GCC uses the vector
register, while Clang uses MEMORY. Untagged ambiguous wrappers are rejected.
The same classification governs direct, indirect and tail calls and returns.
Tagged 512-bit aggregate `va_arg` requires a matching variadic signature and
an uninterrupted local `va_start` provenance proof.

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
the separate load. Selection without trees and AArch64 are unchanged. Each fold is
a named [cover](../../compiler/design.md) that `--isel all,-<name>` turns off:
`x86-compare-memory`, `x86-binary-memory`, `indexed-address`,
`offset-address`, `x86-packed-wide-load`, `x86-mask-test`,
`x86-signed-byte-load` and `x86-packed-low-load`, in the order listed below.

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

## Covers

Every cover is off at Baseline selection and on at Covers (the `-O0`
default), Trees and Full; selection-stage covers also need tree selection. See
[covers](../../compiler/design.md#covers) for the registry and `--isel`
toggles. `--isel all,-<name>` drops one, and `--codegen-report` counts the
roots each one claimed. x86 runs the shared covers `offset-address`,
`indexed-address`, `zero-test`, `compare-branch` and `packed-abs`, and these.
On x86, `compare-branch` also turns a compare-and-swap flag used only by the
next `brif` into a `jne` on the ZF that `lock cmpxchg` leaves.

| Cover | Stage | Pattern |
|---|---|---|
| `x86-division-remainder` | selection | `x - (x / y) * y` reads the division's remainder |
| `x86-lea-mul3` | selection | `x * 3 + c` as one LEA (see below) |
| `x86-binary-memory`, `x86-compare-memory` | selection | Adjacent load as the memory RHS (see [memory operand folding](#memory-operand-folding)) |
| `x86-mask-test` | backend | i32 `and` with a literal mask tested against zero, as TEST |
| `x86-select-compare` | backend | Single-use scalar compare re-emitted at its select (see [scalar compare and select](#scalar-compare-and-select)) |
| `x86-packed-positive`, `x86-packed-sign` | backend | i64x2 positive filters, clamps and sign selects |
| `x86-packed-low-load`, `x86-packed-wide-load`, `x86-signed-byte-load` | backend | Widening loads |

### LEA for multiply by three plus a constant

`x86-lea-mul3` selects `iadd(imul(x, 3), C)`, with either operand order at
both nodes, for modular i32 and i64 only. The product must have exactly one
resolved use, belong to the root block and have no distinct explicit source
line, and the root result must not be an alias. i32 factor and displacement
constants are reduced modulo 2^32; an i64 displacement must fit signed disp32.
Factors five and nine are not selected.

The emitter loads the input once and emits `LEA r32/r64, [r64 + r64*2 +
disp32]`. The 32-bit destination truncates the effective address and zeroes
its upper half, as other i32 results do. LEA neither accesses memory nor
touches flags. `tests/fixtures/bridge_madd.ir`, extracted from the optimized
x86-64 output for `recurrences.c`, checks that tree modes select LEA and
Baseline keeps IMUL and ADD. On x86-64 Linux a test also links the direct
functions against a Clang `-O0` oracle and runs them. No runtime gain is
claimed for this cover.

```sh
cargo test -p keel-codegen --test x86_scalar_selection -- x86_lea::
```

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

A single-block function that contains only a direct `return_call`, where each
argument forwards the matching entry parameter, is a single `jmp` when the
arguments are already placed for the callee. That holds for an identical
signature, and for `cc(c)` and `cc(fast)` signatures with the same scalar or
pointer parameter and result types whose callee needs no extension the caller
lacks. Debug files, variadic signatures, `returns_twice` and exception
personalities keep the normal path.

A frameless function leaves `rsp` and `rbp` unchanged. Its CFA is `rsp + 8`,
with the return address at `CFA - 8`, and the assembler still emits an FDE.
Wide-vector stack alignment and vector spills keep frames.

Parallel copies use reserved `r11` for integers and `xmm14`/`ymm14`/`zmm14` when SIMD
locations take part, keeping every lane through a copy cycle. Memory-to-memory
transfers use `xmm15`/`ymm15`/`zmm15`. Ordinary 128-bit vectors do not reserve a stack
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
- For a large i64 constant whose complement fits in 32 bits, compare the
  assembler's exact encodings of `movabs` and `mov r32, complement; not r64`.
  Choose the latter only when smaller. Both forms preserve flags, including
  when rematerializing an operand before a conditional move. Constants that
  already fit a zero-extended or sign-extended 32-bit move keep that form.

Leaf-frame removal can shorten frame-pointer backtraces, and these choices can
cost runtime. Call sites keep SysV alignment. The last `-O` preset selects the
codegen goal; `--opt-tier` and `-p` override IR passes independently. For
example, `-Os --opt-tier none` keeps size emission, whereas `--opt-tier size`
alone changes only IR passes. The goal is not serialized in IR; apply it again
when compiling an IR file. Library callers set `EmissionOptions::goal` to
`CodegenGoal::Size`; the default is `CodegenGoal::Speed`. AArch64 has no
goal-specific emission.

## Adding an operation

After the IR steps in
[adding an operation](../../compiler/design.md#adding-an-operation):

1. Add the lowering to the family module: `x86_64/lower_scalar.rs`,
   `lower_memory.rs`, `lower_control.rs` or `lower_vector.rs`, or
   `x86_64/intrinsic_family.rs` for a target-independent intrinsic. The family
   matches end in a wildcard that panics, so the compiler does not point to a
   missing arm. Feature-gated forms (AVX, AVX-512, BMI) check the function's
   `x86::Features`.
2. Reject shapes the lowering does not handle in `unsupported` in
   `x86_64.rs`, which `plan::preflight` calls, rather than panicking later.
3. Read every operand before writing the destination, since a result may
   share an operand's register.
4. Add a fused form only as a [cover](#covers) or a `patterns.txt` rule, so
   Baseline keeps the plain lowering as its reference.
5. Add a program under `crates/keel-codegen/tests/programs/` and run
   `cargo test -p keel-codegen --test run` and `--test asm_snapshots`
   (`BLESS=1` rewrites the snapshots). Native execution needs x86-64 Linux;
   elsewhere, export the programs with `KEEL_TEST_EXPORT_DIR` as described in
   [testing](../../development/testing.md#native-x86-64-programs-on-another-host).

## Tests

```sh
cargo test -p keel-codegen --test x86_scalar_selection x86_c_leaf::
cargo test -p keel-codegen --test fast_cc
cargo test -p keel-codegen --test vector_x86 x86_unsigned_byte_compare::
```
