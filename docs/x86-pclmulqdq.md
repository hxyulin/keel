# Optional legacy PCLMULQDQ

PCLMULQDQ is an independent, disabled-by-default x86-64 userspace extension.
It is **not part of psABI v3**. Neither strict v1/v2/v3 nor the historical
legacy default enables it. The canonical modifier is `pclmulqdq`; `pclmul` is
an alias. For example:

```sh
keel cc --target x86_64-unknown-linux-gnu --cpu-level v1 \
  --target-features +pclmulqdq input.c -o output.s
```

`compile`, `cc` and `asm` share this modifier. `-pclmulqdq` explicitly disables
it again. Library callers use `Features::for_level(level).with_features("+pclmulqdq")`
with `FeatureSelector`, and the same features when assembling objects.
`Features::all_known()` includes this optional capability; level presets do not.
The deployment CPU must support CPUID.01H:ECX.PCLMULQDQ[1]. No runtime dispatch
or per-function target attributes are added. ARM targets reject the intrinsic,
feature selection and owned x86 header.

## Exact operation

Only legacy `66 0F 3A 44 /r ib` is supported:

```asm
pclmulqdq xmm8, xmm15, 255
pclmulqdq xmm8, xmmword ptr [r13+r12*4-128], 16
```

Intel syntax has destructive destination/first source, second source, imm8.
AT&T reverses these operands. The destination is XMM, the second source is XMM
or **m128**, and the control is a constant in 0..255. Extended XMM registers,
normal addressing and legacy segment/address prefixes use the shared encoder.
A memory operand accesses 16 bytes even when selecting its low 64-bit half.
It is never an invented eight-byte load. VEX/EVEX and VPCLMULQDQ are unsupported.

Bit 0 selects the first operand's low/high 64 bits; bit 4 selects the second
operand's low/high 64 bits. All other control bits are architecturally ignored.
The result is the complete 128-bit carryless GF(2) polynomial product of those
two 64-bit values: partial products combine with XOR, without carries.
This is neither ordinary integer multiplication nor a modularly reduced GF(2^128)
product. The operation uses no floating arithmetic and preserves MXCSR.

The append-only intrinsic ID 767 is `x86_pclmulqdq128`:

```text
func @product(i64x2,i64x2)->i64x2 {
block0(v0:i64x2,v1:i64x2):
 v2 = iconst.i32 17
 v3 = intrinsic x86_pclmulqdq128(v0,v1,v2)
 return v3
}
```

Both inputs and the result use the existing `i64x2` type/128-bit ABI. The third
argument must resolve to `iconst.i32` in 0..255; dynamic controls are diagnosed.
It is a pure register operation. The dedicated lowering stages both operands
in reserved SIMD scratch registers before destroying the first, then copies or
spills the result. It preserves aliased/live SSA inputs and values across calls.
There is no intrinsic memory folding in this implementation; existing loads,
stores and their footprints remain explicit. Feature preflight and the ordinary
emission guard apply to text, typed and chunked objects in debug and release.

## Bounded C surface

```c
#include <wmmintrin.h>
__m128i product(__m128i a, __m128i b) {
    return _mm_clmulepi64_si128(a, b, 0x11);
}
```

The small owned `wmmintrin.h` includes existing `emmintrin.h` types and exposes
only `_mm_clmulepi64_si128` as a macro. Each vector argument is evaluated once;
the control remains an integer constant expression. It calls the checked builtin
`__builtin_ia32_pclmulqdq128(a,b,imm8)`, accepting matching 64-bit-lane integer
vectors. All 256 controls are legal. Unsupported/dynamic/out-of-range controls
and mismatched vector types are diagnosed. The bounded constant evaluator accepts
integer expressions through 64-bit integers,
including enum constants, casts, `sizeof`, conditionals and short-circuit logic.
A floating literal is accepted only as the immediate operand of a representable
integer cast; floating arithmetic/comparisons are outside this ICE subset.
Evaluated signed overflow, invalid shift counts, division overflow/zero and
unrepresentable floating casts are rejected before folding. i128/pointer
intermediates and comma expressions are rejected.
Including the header on a feature-disabled x86 target emits no unused wrapper;
actually using the operation requires explicit feature enablement.

`__has_builtin(__builtin_ia32_pclmulqdq128)` reports frontend support on x86-64,
independently of selected CPU features; it is false on ARM. The bounded surface
is available through `wmmintrin.h` and `immintrin.h`. AES has a
[separate optional contract](x86-aes-ni.md). Full vendor crypto-header
compatibility, other CLMUL widths, automatic polynomial recognition and CRC
algorithm frameworks remain outside this contract.

## Testing

```sh
cargo test -p keel-asm --test x86_pclmul
cargo test -p keel-codegen --test x86_pclmul
cargo test -p keel-c --test pclmul
cargo test -p keel --test x86_cpu_level
cargo test -p keel-codegen --lib feature_guard_tests
```

Run the feature guard in `--release` as well.

- The assembler matrix compares bytes with Clang and GNU `as` for all 256
  immediates, register/m128 forms, low/extended XMMs, SIB/displacements,
  RIP-relative relocation addends, and segment and address-size prefixes. It also
  checks typed formatting, AT&T, malformed operands, feature exclusion and
  explicit disablement.
- Codegen compares complete text/typed/chunked objects across all three
  selectors, both allocators and normal/empty pools. An external call clobbers
  all XMM registers while the product and inputs are live.
- The C gate covers disabled-header inclusion, builtin support queries,
  diagnostics, single evaluation, the ICE rules above (including rejection of
  invalid shifts, signed overflow, unrepresentable floating casts and i128
  casts) and three frontend/optimization setups.

The native harness (x86-64 Linux, after checking PCLMUL availability) uses an
independent unsigned XOR/shift reference with no shift by 64. It covers
zero/one/all-one/top-bit values, every pair of basis bits, seeded random values,
all four selectors and noncanonical controls 238/239/254/255, same-input and
output aliases, unaligned/guarded-page buffers, sentinels and unchanged MXCSR.
A separate legacy m128 assembly gate checks valid page edges and requires
SIGSEGV when only eight bytes are accessible. Clang and GCC vendor headers pass
the same value harness as secondary oracles. Export codegen/C artifacts with
`KEEL_PCLMUL_EXPORT_DIR` / `KEEL_C_PCLMUL_EXPORT_DIR`.

These finite tests do not establish exhaustive ISA/ABI certification or
performance claims. Strict-feature exclusion is established by compiler and
assembler diagnostics and emission checks, not by execution on a machine
without newer extensions.

Primary semantics references: [Intel SDM, volume 2B](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf),
[Intel carryless multiplication paper](https://cdrdv2-public.intel.com/836172/clmul-wp-rev-2-02-2014-04-20.pdf),
and [Clang's PCLMUL header](https://clang.llvm.org/doxygen/____wmmintrin__pclmul_8h.html).
