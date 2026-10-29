# x86 intrinsic selection

This reference describes current implementation contracts and limits. Use
[testing](../../development/testing.md) for validation commands; per-run results belong outside Git.
The [vendor compatibility design](#design-for-vendor-intrinsic-compatibility)
defines the proposed expansion beyond the implemented bounded headers.

<a id="c-x86-intrinsics-bounded-c-x86-intrinsic-subset"></a>
## Bounded C x86 intrinsic subset

Keel owns small `xmmintrin.h`, `emmintrin.h`, `smmintrin.h`, `avxintrin.h`,
`avx2intrin.h`, `tmmintrin.h`, `fmaintrin.h`, `f16cintrin.h`,
`wmmintrin.h` and `immintrin.h` headers. They are embedded in the compiler;
no Clang resource directory is needed. These headers are for the
`x86_64-unknown-linux-gnu` target. This is a useful, finite source-compatibility
subset, **not complete `immintrin.h` compatibility** or general C
auto-vectorization. Other vendor headers and intrinsics remain unsupported.
The names describe x86 operations, not a CPU vendor restriction. The supported
subset also runs on AMD CPUs when the required CPU features and OS vector state
are available. AMD-specific extensions such as XOP and FMA4 are outside this
header contract.

Use `--target x86_64-unknown-linux-gnu --cpu-level v3` for AVX2 programs.
SSE-only programs can use `--cpu-level v1`. Including the umbrella header does
not enable ISA features or emit unused inline functions. Feature legality stays
with the existing typed IR/backend: 256-bit transport requires AVX; operations
may expand to legal narrower/scalar instructions when AVX2 is disabled. A header
name does not promise a single matching machine instruction. There are no
per-function `target` attributes or runtime CPU dispatch in this subset.

```c
#include <immintrin.h>
void add8(float *out, const float *a, const float *b) {
    _mm256_storeu_ps(out, _mm256_add_ps(_mm256_loadu_ps(a), _mm256_loadu_ps(b)));
}
```

The shared `wmmintrin.h` provides `_mm_clmulepi64_si128` and the
checked `__builtin_ia32_pclmulqdq128`, using existing `__m128i`. This requires
`--target-features +pclmulqdq` even at v3; header inclusion alone is legal without
it. It retains a bounded crypto-header contract. See the
[PCLMULQDQ contract](intrinsics.md) for controls, semantics and finite validation.

The optional [legacy AES-NI subset](intrinsics.md) adds six `__m128i`
wrappers: `_mm_aesenc_si128`, `_mm_aesenclast_si128`, `_mm_aesdec_si128`,
`_mm_aesdeclast_si128`, `_mm_aesimc_si128` and `_mm_aeskeygenassist_si128`.
They require explicit `--target-features +aes` at every CPU level, use checked
builtins, and have no scalar crypto fallback. The key-assist control is an
integer constant expression in 0..255. `__has_builtin` reports frontend
recognition independently of CPU enablement. AES definitions reside in the
AES-owned `__keel_aes.h` fragment of the shared `wmmintrin.h` header.

<a id="c-x86-intrinsics-supported-names"></a>
### Supported names

The following families are the supported contract. Braced alternatives describe
separate names, not arbitrary suffix combinations.

- Types: `__m128`, `__m128d`, `__m128i`, `__m256`, `__m256d`, `__m256i`.
  Integer container types have signed 64-bit C lanes; use intrinsics or explicit
  vector casts to choose another lane width. Nominal typedef identity is not a
  promise of every implementation detail of another compiler's private types.
- Packed floating arithmetic: `_mm_{add,sub,mul,div}_{ps,pd}` and
  `_mm256_{add,sub,mul,div}_{ps,pd}`. Bit operations:
  `_mm_{and,or,xor,andnot}_{ps,pd}` and their `_mm256_` equivalents.
- Floating constructors: `_mm_{setzero,set1,set}_{ps,pd}` and their `_mm256_`
  equivalents. `set` arguments run from highest to lowest lane, as in vendor
  headers; lane zero occupies the lowest address. `_mm_cvtss_f32` reads lane zero.
- Floating memory: `_mm_{load,loadu,store,storeu}_{ps,pd}` and their `_mm256_`
  equivalents. These access exactly 16 or 32 bytes. The aligned spellings retain
  their source alignment precondition; the implementation also uses byte-safe
  copies and does not require a fault for a misaligned pointer.
- Packed integers, for both `_mm_` and `_mm256_`: `{add,sub}_epi{8,16,32,64}`,
  `mullo_epi{16,32}`, `{cmpeq,cmpgt}_epi{8,16,32,64}`,
  `set1_epi{8,16,32,64x}`, `set_epi{32,64x}`. Integer arithmetic wraps at its lane
  width. Comparisons use signed lanes and return all-one/zero lane masks.
- SSE2 saturation: `_mm_{adds,subs}_{epi8,epi16,epu8,epu16}` clamps each
  mathematical sum/difference to its signed/unsigned lane range. This is an
  eight-wrapper, 128-bit subset; see the saturation contract below.
- SSE2 packed shifts: `_mm_{slli,srli}_epi{16,32,64}` and
  `_mm_srai_epi{16,32}` with immediate controls, plus
  `_mm_{sll,srl}_epi{16,32,64}` and `_mm_sra_epi{16,32}` with vector counts.
  These are 128-bit operations; see the bounded shift contract below.
- AVX2 packed shifts: the sixteen `_mm256_` equivalents of those immediate and
  shared-count shifts, plus `_mm{,256}_{sllv,srlv}_epi{32,64}` and
  `_mm{,256}_srav_epi32`. Shared counts use an `__m128i` even for 256-bit data;
  per-lane counts have the same width as the data. See the shift contract below.
- SSE2 numeric conversions: `_mm_cvtepi32_ps`, `_mm_cvttps_epi32` and
  `_mm_cvtps_epi32`; see the bounded architectural conversion contract below.
- Integer bit operations: `_mm_{and,or,xor,andnot}_si128` and
  `_mm256_{and,or,xor,andnot}_si256`; `andnot(a,b)` means `~a & b`.
  `_mm_setzero_si128`, `_mm256_setzero_si256`,
  `_mm_{load,loadu,store,storeu}_si128`, and their `_mm256_*_si256` equivalents
  are supported. Unaligned memory spellings do not impose vector alignment or
  incompatible scalar aliasing requirements; each copies exactly its width.
- All six same-width bit casts among `ps`, `pd`, and `si128` under `_mm_cast`,
  and among `ps`, `pd`, and `si256` under `_mm256_cast`.
  `_mm256_castps256_ps128`, `_mm256_castpd256_pd128`, and
  `_mm256_castsi256_si128` select the low half. Casts with an undefined upper
  half are not included. The three numeric conversions below are separate from
  these bit casts.
- `_MM_SHUFFLE`, `_mm_shuffle_ps`, `_mm_shuffle_pd`, `_mm_shuffle_epi32`,
  `_mm256_shuffle_ps`, `_mm256_shuffle_epi32`, and
  `_mm256_permute4x64_epi64`. AVX 32-bit shuffles operate independently within
  each 128-bit half; the 64-bit permutation can cross halves. Controls must be
  integer constant expressions. Vector operands are evaluated once.
- `_mm_{extract,insert}_epi{16,32}`; 16-bit extraction zero extends.
  `_mm256_{extractf128,insertf128}_{ps,pd,si256}` and
  `_mm256_{extracti128,inserti128}_si256`. These wrappers also accept runtime
  lane controls as an extension: low 3/2/1 bits select a 16-bit lane, 32-bit lane,
  or 128-bit half respectively. Each argument is evaluated once. Immediate-only
  vendor behavior is not a diagnostic compatibility promise here.
- SSE comparisons `_mm_cmp{eq,lt,le,gt,ge,neq}_{ps,pd}`.
  AVX `_mm256_cmp_{ps,pd}` supports only the constant quiet predicates
  `_CMP_EQ_OQ` (0), `_CMP_UNORD_Q` (3), `_CMP_NEQ_UQ` (4), `_CMP_ORD_Q` (7),
  `_CMP_LT_OQ` (17), `_CMP_LE_OQ` (18), `_CMP_GE_OQ` (29), `_CMP_GT_OQ` (30).
  Other predicates, including signaling variants, are diagnosed. True lanes
  contain all bits set in the floating return type. NaNs follow the predicate's
  ordered/unordered value semantics. This subset does not establish a strict
  floating-environment contract for signaling NaNs, exception flags/traps, or
  differences between quiet and signaling SSE instructions.

The SSE/AVX header functions use C vectors, scalar lane access, and fixed-size
`__builtin_memcpy`; saturation, shifts, shuffles and AVX comparisons use checked builtins.
The three numeric conversions also use checked builtins and dedicated architectural IR.
All lower through the existing typed IR, optimization, ABI, feature gating and
instruction selection. The explicit PCLMUL builtin uses its dedicated typed
intrinsic and lowering described in the linked contract.

### Integer family and fused/conversion wrappers

The integer headers also expose these families:

- SSE2 `_mm_{min,max}_{epi16,epu8}`.
- SSE4.1 `_mm_{min,max}_{epi8,epi32,epu16,epu32}` and `_mm_blendv_epi8`.
- SSSE3 `_mm_{abs,sign}_epi{8,16,32}` through `tmmintrin.h`.
- AVX2 `_mm256_{min,max}_{epi8,epi16,epi32,epu8,epu16,epu32}`,
  `_mm256_{abs,sign}_epi{8,16,32}` and `_mm256_blendv_epi8`.

These 38 wrappers use generic integer IR. The 128-bit forms may lower to
baseline instructions rather than requiring the vendor header's named ISA
family. The 256-bit forms require AVX transport and may use narrower/scalar
operations without AVX2. They do not promise vendor feature-diagnostic parity
or one matching instruction. Negation uses unsigned lane arithmetic, so absolute
value and sign retain the signed-minimum bit pattern. Byte blends select by the
mask byte's sign bit, not by whether the whole byte is nonzero. Each argument
is evaluated once.

`fmaintrin.h` exposes 32 FMA wrappers: `_mm_{fmadd,fmsub,fnmadd,fnmsub}`
with `ps`, `pd`, `ss` and `sd` suffixes; the `_mm256_` packed `ps`/`pd`
forms; and `_mm{,256}_{fmaddsub,fmsubadd}_{ps,pd}`. They use the existing
architectural FMA IR and require FMA permission. Scalar `ss`/`sd` updates only
the low lane and preserves the remaining lanes from the first argument. The
operations retain the existing effectful floating-point contract; they are not
expanded into separately rounded multiplication and addition or a libm call.

`f16cintrin.h` exposes `_mm{,256}_cvtph_ps`, `_mm{,256}_cvtps_ph`,
`_cvtsh_ss` and `_cvtss_sh` through the existing F16C IR. These require F16C
permission. Packed conversion to half returns an `__m128i`; the 128-bit float
source form zeroes its upper 64 result bits. The rounding argument must be an
integer constant expression in 0..255. Bits 7:3 are ignored and the IR receives
bits 2:0; bit 2 selects MXCSR rounding. The `_MM_FROUND_*` constants and combined
rounding aliases are defined in `smmintrin.h`. In particular, the `NO_EXC`
constant's bit 3 does not suppress F16C exceptions. Header inclusion itself does
not grant CPU features or require the host to support the instructions.

### Reviewed builtin inventory

[`vendor-bindings.json`](../../../tools/x86-isa/vendor-bindings.json) owns the
95 reviewed frontend bindings for AES, packed shifts, saturation, SSE2 numeric
conversions, FMA and F16C. Its generated module supplies both parser lookup and
`__has_builtin` recognition. The metadata records current semantic intrinsic
IDs, types, controls, required features and ownership. Generated assertions
preserve IDs and check reviewed signatures against the existing IR. Semantic
lowering and CPU legality remain with the existing typed operations.

[`vendor-references.json`](../../../tools/x86-isa/vendor-references.json) pins
the existing Keel contracts and LLVM/Clang 21.1.0 reference commit, licence and
six header hashes. The new wrappers are independently implemented against
those references. Keel-private bridge names are marked separately from vendor
builtin spellings. This is a bounded inventory, not complete Intel/GNU/Clang
builtin or vendor-header compatibility. See the [generator reference](../../../tools/x86-isa/README.md)
for regeneration and optional downloaded-header hash checks.

<a id="c-x86-intrinsics-builtins-and-diagnostics"></a>
### Builtins and diagnostics

`__builtin_shufflevector(a,b,i0,...)` requires two matching supported C vector
types and exactly one result index per input lane. Indices must be integer
constant expressions in `[0,2*lanes)`, indexing the concatenated inputs.
Result shape and C element type equal the inputs. Different result widths,
`-1` undefined lanes, mixed input types, dynamic controls and out-of-range
indices are rejected. Both inputs are evaluated once. This applies to the
existing 16-byte vectors on all supported targets as well as x86 32-byte vectors.

`__builtin_ia32_cmpps256(a,b,predicate)` and
`__builtin_ia32_cmppd256(a,b,predicate)` implement the bounded AVX comparisons
listed above, requiring matching `f32x8`/`f64x4` C vectors on x86-64.
`__has_builtin` recognizes the shuffle builtin everywhere and these two
comparison builtins only on x86-64; this reports frontend support, not enabled
CPU features. No general GNU/Clang `__builtin_ia32_*` compatibility is claimed.

Unsupported sizes/types, unequal-width vector casts, unsupported comparison
predicates, and illegal target features fail with diagnostics. Ordinary vector
expressions support bounded lossless scalar broadcasts for arithmetic, integer
bitwise operations and comparisons, including compound assignments; see
[GNU vector mode](../../frontends/c/extensions.md#gnu-vector-mode-scalar-broadcasts) for conversion checks
and remaining scalar-shift/cross-domain limits. Constructor functions remain
available for explicit element conversions.
Masked/gather/scatter memory, streaming memory/fences, scalar-lane FP intrinsics
beyond FMA, saturation beyond the bounded SSE2 subset, horizontal operations,
shifts beyond the reviewed SSE2/AVX2 subset, conversions beyond the reviewed SSE2
and F16C forms, AVX-512,
full vendor header coverage and strict FP-environment compatibility are outside
this header contract even where the backend already implements such operations.
The existing C restrictions on vector varargs, unions and by-value x86 aggregates
remain in force; see [GNU vector mode](../../frontends/c/extensions.md).

<a id="c-x86-intrinsics-bounded-sse2-packed-shifts"></a>
### Bounded SSE2 and AVX2 packed shifts

The sixteen shift wrappers above use the existing architectural
`x86_sse2_ps{ll,rl,ra}{w,d,q}128` intrinsics (arithmetic right shift has no
64-bit form). Logical shifts zero every lane when the unsigned count is at
least the lane width; arithmetic right shifts sign-fill instead. Counts are
never reduced modulo the width. These semantics differ from ordinary C vector
shifts, whose out-of-range counts are undefined.

Immediate controls must be defined integer constant expressions in 0..255.
Negative controls, 256 and larger controls, runtime controls and unsupported
constant-expression proofs are diagnosed. This deliberately bounded diagnostic
contract does not promise vendor acceptance/truncation of other integer controls.
The implementation reuses the AES/PCLMUL bounded integer-constant-expression proof.
The data expression is evaluated once. An immediate wrapper does not promise
an immediate machine encoding: the current architectural IR uses an i64 count.

The runtime forms take `__m128i` data and count arguments, evaluating each once.
Only the unsigned low 64 bits of the count vector are consumed; the upper 64
bits are ignored. A low half with its sign bit set is a large positive count,
not a negative C shift. Inputs and results retain the existing 16-byte vector
ABI. No memory access or FP operation is introduced by the shift itself;
composing load/store wrappers retains their exact 16-byte footprint.

The private `__builtin_keel_sse2_{imm,count}_{psllw,pslld,psllq,psrlw,psrld,psrlq,psraw,psrad}`
bridge requires the matching 128-bit integer lane shape on x86-64. The `imm`
form checks the control proof; the `count` form requires an unsigned long long
scalar count. `__has_builtin` reports these names only on x86-64, independently
of CPU selection. This does not add general vendor shift builtin compatibility.
Strict v1 already includes SSE2; the existing backend preflight and release
emission guard remain authoritative. ARM inclusion and direct builtin use are
diagnosed. No backend, optimizer, FP policy or ABI changes accompany this surface.

AVX2 adds sixteen `_mm256_` immediate/shared-count wrappers through the existing
`x86_avx2_vps{ll,rl,ra}{w,d,q}256` intrinsics. Data and results are 32 bytes;
the shared count is still an `__m128i` and only its unsigned low 64 bits matter.
Immediate controls use the same 0..255 proof and diagnostic contract as SSE2.
The private `__builtin_keel_avx2_{imm,count}_{psllw,pslld,psllq,psrlw,psrld,psrlq,psraw,psrad}`
bridges require the matching 256-bit lane shape and an unsigned scalar count.

The ten per-lane wrappers `_mm{,256}_{sllv,srlv}_epi{32,64}` and
`_mm{,256}_srav_epi32` use the existing AVX2 variable-shift intrinsics. Each
32/64-bit count lane is an unsigned count for its corresponding data lane.
All count bits matter; counts at or above the lane width zero or sign-fill the
result. No 16-bit per-lane or 64-bit arithmetic-right form is admitted here.
Their private `__builtin_keel_avx2_{psllvd,psllvq,psrlvd,psrlvq,psravd}{128,256}`
bridges require two matching signed C vector types. Public wrappers cast their
integer containers to those types and evaluate each operand once.

All these AVX2 shifts require explicit AVX2 permission, including 128-bit
per-lane forms. They do not use the software fallback policy of generic integer
header expressions. The existing 16/32-byte vector ABI and load/store footprints
apply. Validation covers vector calls, forced spills, large counts, independent
scalar/vendor oracles, unaligned protected-page boundaries and aliased outputs.

<a id="c-x86-intrinsics-bounded-sse2-numeric-conversions"></a>
### Bounded SSE2 numeric conversions

`emmintrin.h` exposes exactly three full-width 128-bit conversion functions:

- `__m128 _mm_cvtepi32_ps(__m128i a)`: four signed i32 lanes to f32,
  rounded using MXCSR.RC. Inexact inputs, including integers beyond 2^24,
  set the precision status flag.
- `__m128i _mm_cvttps_epi32(__m128 a)`: four f32 lanes to signed i32,
  truncating toward zero independently of MXCSR.RC.
- `__m128i _mm_cvtps_epi32(__m128 a)`: four f32 lanes to signed i32,
  rounded using MXCSR.RC, including nearest-even ties.

These use `x86_sse_cvtdq2ps128`, `x86_sse_cvttps2dq128` and
`x86_sse_cvtps2dq128`, respectively. Their existing impure, pinned barrier
semantics and direct legacy SSE2 lowering preserve architectural MXCSR
observations and status effects even with unused results. No portable
saturating conversion, scalar C cast or constrained numerical conversion is
substituted. With invalid exceptions masked, NaNs, infinities and out-of-range
rounded results produce x86 integer-indefinite `0x80000000` in the affected
lane and set invalid status. Valid negative INT32_MIN has the same result bits
without invalid status. Fractional valid conversions set precision status;
DAZ/FTZ and seeded status bits follow the hardware instruction. Wrappers never
change MXCSR controls or clear flags. This narrow contract does not extend the
ordinary C arithmetic/comparison FP-environment contract.

Each operand is evaluated once. Public nominal types remain `__m128` and
`__m128i`; the private `__builtin_keel_sse2_{cvtdq2ps,cvttps2dq,cvtps2dq}`
bridge checks exactly one signed `int` i32x4 or `float` f32x4 C vector on
x86-64. It returns f32x4 or signed `int` i32x4; the wrapper bitcasts the
integer container. `__has_builtin` recognizes these names on x86-64 regardless
of feature selection, and reports false on ARM. These private names do not
promise GNU vendor builtin compatibility. Definitions live in the private
`__keel_sse2_conversions.h` fragment. V1 already includes SSE2; the existing
backend preflight, release legality guard and assembler checks remain in force.
No backend, optimizer, ABI, feature/default-width or FP-policy change is made.

<a id="c-x86-intrinsics-bounded-sse2-saturation"></a>
### Bounded SSE2 saturation

`_mm_adds_epi8`, `_mm_adds_epi16`, `_mm_adds_epu8`, `_mm_adds_epu16`,
`_mm_subs_epi8`, `_mm_subs_epi16`, `_mm_subs_epu8`, `_mm_subs_epu16`
accept and return `__m128i`, evaluating each argument once. Byte/word lanes
clamp to -128..127 / -32768..32767 for signed operations and 0..255 /
0..65535 for unsigned operations. Subtraction preserves operand order.
These functions do not emulate saturation with overflowing narrow C arithmetic.
They bitcast to the appropriate lane type and call the existing typed
`x86_sse2_p{add,sub}{s,us}{b,w}128` architectural intrinsics. Their existing
`x86_packed_pair` metadata selects legacy SSE2 instructions (tier 0), and the
existing packed-pair preflight/lowering and release emission checks remain in
force. No optimizer, ABI, FP or backend changes are involved.

The private `__builtin_keel_sse2_p{add,sub}{s,us}{b,w}` names each require
exactly two matching 16-byte C vectors. Their element type must be signed char,
short, unsigned char or unsigned short, respectively. Plain char is distinct
from signed char. Wrong signedness, nominal element type, width, shape, arity,
scalar/FP arguments and ARM targets are diagnosed. Results retain the operand
C type. `__has_builtin` reports these names only on x86-64, independently of
CPU selection; these are Keel bridges, not GNU builtin compatibility promises.
Header inclusion alone emits no additional functions or enabled features.

<a id="x86-aes-ni-legacy-128-bit-aes-ni"></a>
## Legacy 128-bit AES-NI

Keel supports the six explicit legacy AES-NI operations on
`x86_64-unknown-linux-gnu`. Enable the independent `aes` feature with
`--target-features +aes` for `asm`, `compile` or `cc`, or
`Features::for_level(level).with_features("+aes")` through `FeatureSelector`.
It is disabled in the historical default and strict v1/v2/v3 profiles.
`-aes` disables it again. `+aes` does not enable AVX or any other feature;
PCLMULQDQ has a separate feature contract. No fallback bypasses disabled AES.
The deployment CPU must support CPUID.01H:ECX.AES[25] and the normal OS-enabled
XMM state contract. Keel does not insert a runtime probe or dispatch.

<a id="x86-aes-ni-architectural-operations"></a>
### Architectural operations

| IR intrinsic | Parameters → result | Legacy instruction |
|---|---|---|
| `x86_aesenc128` | `i64x2, i64x2 → i64x2` | `AESENC` |
| `x86_aesenclast128` | `i64x2, i64x2 → i64x2` | `AESENCLAST` |
| `x86_aesdec128` | `i64x2, i64x2 → i64x2` | `AESDEC` |
| `x86_aesdeclast128` | `i64x2, i64x2 → i64x2` | `AESDECLAST` |
| `x86_aesimc128` | `i64x2 → i64x2` | `AESIMC` |
| `x86_aeskeygenassist128` | `i64x2, i32 → i64x2` | `AESKEYGENASSIST` |

The integer vector is a 128-bit container, with byte zero at the lowest address.
Rounds consume the state first and round key second. Encryption applies
ShiftRows, SubBytes, MixColumns and AddRoundKey; the last round omits
MixColumns. Decryption uses the Equivalent Inverse Cipher: InvShiftRows,
InvSubBytes, InvMixColumns and AddRoundKey; its last round omits InvMixColumns.
AESIMC applies InvMixColumns to its input. These are pure bit transformations;
they do not access implicit memory or alter MXCSR.

AESKEYGENASSIST requires an `iconst.i32` in 0..255, checked by the IR verifier
and backend preflight. If the four input little-endian doublewords are
`X0,X1,X2,X3`, its output doublewords from low to high are
`SubWord(X1), RotWord(SubWord(X1)) XOR imm8, SubWord(X3),
RotWord(SubWord(X3)) XOR imm8`. RotWord is a 32-bit rotate right by eight;
the immediate is zero-extended into the low byte of its doubleword.

The semantic authority is Intel's
[AES New Instructions Set white paper](https://www.intel.com/content/dam/doc/white-paper/advanced-encryption-standard-new-instructions-set-paper.pdf),
particularly the instruction pseudocode and byte/doubleword ordering on
pages 15–19. Algorithm and known-answer vectors follow
[NIST FIPS 197](https://nvlpubs.nist.gov/nistpubs/fips/nist.fips.197.pdf),
including Appendix C.1's AES-128 encryption and decryption example.

The typed assembler accepts legacy XMM register sources and m128 sources,
including extended registers and supported ordinary addressing/segment forms.
Round destinations are destructive XMM operands; AESIMC and AESKEYGENASSIST
replace their destination. Legacy memory sources access exactly 16 bytes and
require 16-byte alignment. Register intrinsics retain explicit loads, including
unaligned loads, rather than folding them into an alignment-requiring source. Mandatory
66, REX extension bits, 0F38 DB–DF and 0F3A DF/imm8 use the existing typed
legacy table and encoder. Wrong register classes, explicit memory widths and
invalid immediate controls diagnose. IR lowering stages inputs in reserved
scratch XMM registers before writing the destination, handling live values,
coalesced operands and spills without changing the existing ABI or load/store
footprints. Both assembler and ordinary emission feature guards apply to text,
typed and chunked objects, in debug and release. ARM backends reject these IR
operations.

<a id="x86-aes-ni-bounded-c-surface"></a>
### Bounded C surface

`<wmmintrin.h>` and `<immintrin.h>` expose these six wrappers using the existing
`__m128i` container:

- `_mm_aesenc_si128(a,b)`, `_mm_aesenclast_si128(a,b)`
- `_mm_aesdec_si128(a,b)`, `_mm_aesdeclast_si128(a,b)`
- `_mm_aesimc_si128(a)`
- `_mm_aeskeygenassist_si128(a,control)`

The AES-owned definitions live in `<__keel_aes.h>`, a fragment of the shared
`wmmintrin.h` surface. Each macro evaluates each vector operand once and maps
to the corresponding checked `__builtin_ia32_<instruction>128` builtin.
Operands must have the matching signed long-long 16-byte vector type used by
`__m128i`; mixed vector types diagnose. AESKEYGENASSIST's control must be an
integer constant expression in 0..255, within the frontend’s bounded
64-bit constant evaluator (128-bit/pointer intermediates diagnose). The same bounded immediate check used by PCLMUL rejects invalid
shift counts, signed overflow, unrepresentable float casts and non-ICE floating
expressions before folding. The parser canonicalizes it to the
required IR constant; dynamic, floating and out-of-range controls diagnose.
Header inclusion enables no features and emits no unused functions.
`__has_builtin` reports frontend recognition on x86, independently of the CPU
feature selection; it returns false on ARM, where direct builtin use and header
inclusion diagnose.

This contract adds no VEX/EVEX/VAES, AVX-512, automatic AES recognition,
per-function target dispatch, AES modes, GCM or crypto library. It makes no
whole-program constant-time or security claim.

<a id="x86-aes-ni-testing"></a>
### Testing

```sh
cargo test -p keel-codegen --test x86_aes
cargo test -p keel-c --test x86_aes --test x86_intrinsics
cargo test -p keel --test x86_aes
cargo test -p keel-codegen --release --lib feature_guard_tests
```

- `keel-codegen --test x86_aes` checks all six operations against an independent
  GF(256) software oracle (algebraic S-box construction, inverse rows and column
  transforms), the FIPS 197 AES-128 example, 128 basis cases, zero/all-one edges,
  random inputs and all 256 key-assist controls. It runs all three selection
  tiers, both allocators and normal and empty register pools, with live and
  identical operands, unaligned inputs, direct/indirect calls through callbacks
  that clobber XMM1-XMM15, MXCSR and output sentinels, and both guard-page edges.
  Separate assembled m128-source kernels check exact 16-byte read/store
  footprints. Malformed IR (wrong arity/types, an i64 key-assist control) must
  be diagnosed by backend preflight.
- The encoding matrix compares text bytes with Clang and, when `KEEL_GNU_AS` is
  set, GNU `as`, including extended registers, base/index/scale/displacements,
  RIP-relative relocations (including the key-assist immediate's displacement
  addend), FS memory forms and immediate boundaries. Typed formatting/parsing,
  AT&T parsing and text/typed/chunked object parity are also checked.
- CLI and C tests check strict-level rejection, explicit acceptance, frontend
  recognition, ICE/type/foreign diagnostics, unused-header behavior and single
  evaluation, and execute all six wrappers natively against the same oracle.
- Feature matrices cover neither, AES only, PCLMUL only and both, at strict
  v1/v2/v3.

Native execution is guarded by AES/SSE2 detection and runs on x86-64 Linux
with both Clang and GCC as harness compilers. On macOS arm64 the tests
cross-emit x86 ELF objects and cover encoding, frontend and feature diagnostics
only. These are finite tests on an AES-capable host, not exhaustive ISA
certification or execution on a CPU that lacks AES.

AES intrinsic IDs are 768-773; PCLMUL keeps ID 767.

<a id="x86-pclmulqdq-optional-legacy-pclmulqdq"></a>
## Optional legacy PCLMULQDQ

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

<a id="x86-pclmulqdq-exact-operation"></a>
### Exact operation

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

<a id="x86-pclmulqdq-bounded-c-surface"></a>
### Bounded C surface

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
[separate optional contract](intrinsics.md). Full vendor crypto-header
compatibility, other CLMUL widths, automatic polynomial recognition and CRC
algorithm frameworks remain outside this contract.

<a id="x86-pclmulqdq-testing"></a>
### Testing

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

<a id="x86-query-bindings-generated-descriptor-query-and-scalar-port-io-bindings"></a>
## Generated descriptor-query and scalar port-I/O bindings

`tools/x86-isa/query-bindings.json` owns reviewed metadata for 42 existing
intrinsics at IDs 687 through 728. `query_bindings.py` generates names,
signatures, purity, memory effects, target classification patterns and the
three public helper tables used by shared native emitters.

| Family | Bindings | Generated helper |
|---|---:|---|
| SGDT/SIDT, SLDT/STR/SMSW memory/register observations | 14 | `x86_descriptor_query()` |
| LAR/LSL and VERR/VERW register/memory queries | 16 | `x86_selector_query()` |
| IN/OUT, widths 8/16/32, DX or immediate port | 12 | `x86_port_io()` |

The schema retains exact signatures and helper return shapes. Descriptor
memory write footprints are separate from encoded operand widths. Register
observations retain their old i64 containers. LAR/LSL retain the old destination
and their separate I32 ZF result. Immediate port APIs retain an i16 parameter;
existing compiler verification still requires a constant port in 0..255.

Every binding remains impure, x86-only and a pinned compiler memory barrier.
The generator rejects attempts to weaken that policy, invalid opcode/width
combinations, incorrect signatures, wrong memory footprints and unknown fields.
These declarations are reviewed compiler policy, not semantics inferred from
instruction bytes or CPL metadata.

<a id="x86-query-bindings-shared-code-and-registry-boundary"></a>
### Shared code and registry boundary

`crates/keel-ir/src/generated/x86_query_intrinsics.rs` supplies metadata and
helper implementations. Generic memory classification and AArch64/native
exhaustive classification use its generated patterns. The same metadata emitter
serves the existing 45 system declarations; their generated file is unchanged.

The descriptor, selector and scalar port-I/O emitters remain shared handwritten
Rust. Their staging order, result publication, ZF capture, zero extension,
allocator interaction and rematerialization behavior are unchanged. No new
instruction, intrinsic ID, encoder, operand rule or execution contract is added.

Enum declarations and `ALL` entries now come from the
[global intrinsic registry](../../../tools/x86-isa/README.md#intrinsic-registry-and-family-declarations).
Generated const assertions and the registry's family join check every query
ID. The frozen inventory checks numeric IDs and iteration order independently.
The registry cleanup removes the former 774-entry handwritten prefix and
contiguous system-tail assumption. Query metadata and lowering stay separate.

The structural coverage ledger still tracks its original 45 system lowering
declarations. It does not yet join these 42 query/I/O bindings. Empty compiler
links for their forms remain unknown coverage, rather than unsupported claims.

<a id="x86-unmasked-fp-unmasked-fp-contract-and-remaining-hardware-exposure"></a>
## Unmasked FP: contract and remaining hardware exposure

This page describes how Keel's raw x86 floating-point intrinsics behave when FP
exceptions are unmasked, and which families still lack unmasked validation. Full
x86 profile support is not claimed, and native Linux signal-delivery validation
has not been completed.

<a id="x86-unmasked-fp-sseavx-contract"></a>
### SSE/AVX contract

[simd-ir.md](../../compiler/simd-ir.md) distinguishes portable numerical FP from architectural
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

<a id="x86-unmasked-fp-other-families"></a>
### Other families

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

<a id="x86-unmasked-fp-x87"></a>
### x87

[f80/numerical.md](f80/numerical.md) requires empty x87 tags, all six CW
exception masks set, and no pending unmasked exception; PC/RC are inherited and
CW is preserved. Its operations perform input FLD, snapshot FNSTSW immediately
after the principal opcode, then store/discard results and restore empty tags
and the entry TOP. Input loading and result storage are therefore distinct
exceptional stages. The returned status includes the temporary physical TOP; it
is not a fault-context ABI.

[basic](f80/basic.md), [binary](f80/binary.md), [compare](f80/compare.md),
[dc](f80/dc.md), [bcd](f80/bcd.md), and [constants](f80/constants.md) also
restrict their raw contracts to a masked CW. [stages](f80/README.md) separately
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

<a id="x86-unmasked-fp-remaining-work"></a>
### Remaining work

- Native Linux execution of the unmasked x87 region packet (216 GCC/Clang object
  runs, eight shards each). Cross-compilation and byte/stage checks do not
  establish kernel delivery, per-form hardware trap obligations, or native
  FIP/FOP attribution.
- Separate forced-assembly terminal proofs for the FMA/F16C, SSE3/round,
  dot-product, and MMX-value families. Raw MMX tag images there are diagnostic
  and are not equated with SSA state.
- Arbitrary nonempty x87 entry and signal-context restart are outside these
  contracts.

## Design for vendor intrinsic compatibility

This section defines the expansion design. The bounded header contract above
remains the implemented interface. Assembly coverage, accepted builtin names and
runtime-qualified intrinsic semantics are separate milestones.

### Compatibility boundary

The first compatibility target is C source using Intel-style intrinsic headers
on `x86_64-unknown-linux-gnu`, checked against pinned LLVM/Clang headers and GCC
where their interfaces overlap. A release must name the exact reference commits,
header hashes, licences and covered names. “Full” means all entries in that
reviewed manifest, including macro wrappers and composite intrinsics; it does
not mean all spellings in every vendor release. New vendor releases require an
explicit manifest update.

C++ overloads/templates, Microsoft-specific headers and calling conventions,
32-bit x86, SVML and other external vector-math libraries need separate source,
ABI or runtime contracts. They must appear as explicit exclusions in the
manifest rather than silently counting as compatible. APX instruction coverage
is primarily an encoding and compiler-allocation concern; it does not imply a
new vendor intrinsic for every opcode.

Reference interfaces:

- [Intel Intrinsics Guide](https://www.intel.com/content/www/us/en/docs/intrinsics-guide/index.html)
  supplies names and semantic descriptions, including operations implemented by
  instruction sequences. It is not the header parser or an encoding oracle.
- [Clang language extensions](https://clang.llvm.org/docs/LanguageExtensions.html)
  describe feature queries, builtin queries and vector language extensions.
- [Clang target attributes](https://clang.llvm.org/docs/AttributeReference.html#target)
  describe the source interface for function-specific target permissions.

### One reviewed binding inventory

Add a versioned vendor binding inventory beside the existing inputs in
`tools/x86-isa/`. Keep it separate from the ISA form catalogue: a C wrapper may
map to generic IR, one semantic intrinsic, or a sequence of operations. Reuse
`intrinsic-registry.json` for stable IDs only when a new semantic IR intrinsic
is needed. Do not allocate an IR ID for each header alias.

Each vendor binding records:

| Field | Contract |
| --- | --- |
| Identity | Header, C spelling, builtin aliases, reference revision and source location |
| Type | Exact parameter/result types, overload rules, signedness, alignment and ABI representation |
| Arguments | ICE requirements, legal control values, ignored bits, pointer qualifiers and evaluation count |
| Permissions | Required CPU features, target modes and architectural/OS state preconditions |
| Effects | Memory reads/writes and access widths, flags, FP environment, faults, ordering and state changes |
| Lowering | Generic IR, existing semantic intrinsic, reviewed new intrinsic, or documented external runtime call |
| Evidence | Admission cases, independent semantic oracle and feature/state qualification requirements |

Generate the embedded header index, builtin-name recognition, `__has_builtin`
answers, signatures, immediate diagnostics and binding documentation from that
inventory. Migrate the existing AES, shift, saturation and conversion name
lists first; `expr.rs` and `features.rs` must not maintain parallel name lists.
Use reviewed wrapper templates instead of mechanically copying LLVM builtin
bodies into headers that Keel cannot compile.

Generation must reject missing/duplicate identities, unknown features,
unresolved type names, overlapping aliases and unsupported lowering recipes.
Keep unsupported bindings explicit in the inventory and exclude them from
positive `__has_builtin` answers. Header inclusion must remain legal without
enabling every feature named by its declarations. Recognizing a builtin name
does not grant CPU permission or establish that an instruction can execute.

### Frontend and type work

Preserve nominal container types (`__m128i`, `__m256i`, eventually `__m512i`)
while reviewing each builtin's actual lane type. Establish the required 512-bit value
and mask types, half/bfloat element types, casts and overloaded builtin rules
before admitting the families that depend on them. A pointer to unaligned data
must not become an aligned load merely because the wrapper uses a vector type.

Validate immediate controls in the frontend against each binding's rules.
Distinguish an out-of-range immediate from an instruction that masks or ignores
control bits. Macro wrappers must evaluate non-constant operands the documented
number of times. Keep constant-expression acceptance separate from optimizer
constant propagation, including at `-O0`.

Legacy MMX values must retain the existing explicit x87/MMX state contracts. AMX configuration and
tile operations need a tile-state contract, with tile IDs, configuration
validity, release, faults and call clobbers. An 8192-bit assembler tile register
is not an ordinary 8192-bit SSA vector or a C-callable argument. Start AMX vendor
wrappers with checked constant tile IDs and explicit effects; introduce tile SSA
only after its allocation, lifetime and calling-boundary rules are defined.

### Function-specific features and dispatch

Add explicit function feature permissions in shared IR rather than using
optimization/tuning attributes as a proxy. Resolve a function's effective
features from the module baseline plus its declared ordered edits, using the
existing feature prerequisite validator. The backend's function selector,
legality checks, builtin lowering and assembly output must use that same set.
Preserve it through IR printing/parsing and LLVM import where supported.

A baseline function may make an ordinary call to a feature-specific function;
it may not inline a body requiring unavailable features. Enforce this in both
optional inlining and mandatory `always_inline` expansion, with a diagnostic
when mandatory expansion cannot satisfy the feature contract. Global flags must
not be widened to make such a call compile. A compatible call also needs a
compatible argument/result ABI; wide-vector transport must be checked at both
ends of a call, including indirect calls and cross-module declarations.

Compile-time feature macros describe the translation unit's baseline. Builtin
recognition queries describe admitted frontend interfaces. A future
`__builtin_cpu_supports` implementation must report executable capabilities,
including required OS-enabled extended state; CPU feature bits alone are
insufficient for AVX, AVX-512 or AMX. Its results do not make an arbitrary branch
a proof that illegal instructions can be placed in a baseline function.
Automatic multiversion resolvers and dispatch remain separate from explicit
feature-specific calls and queries.

### Semantic lowering

Prefer existing generic vector/integer IR when it preserves the full contract.
Target intrinsics own operations whose semantics cannot be expressed there.
Attach reviewed effects before allowing CSE, DCE, hoisting or constant folding;
replace the current binary pure/pinned classification only where richer effects
are required, using the shared memory and FP-state contracts.

Masked AVX-512 operations require typed predicates, merge/zero results, opmask
allocation and defined lane mapping. Masked memory must suppress the specified
inactive-lane accesses and faults. Computing an unmasked operation followed by a
select is not a valid general implementation. Rounding, SAE, MXCSR effects,
NaN behavior and exception delivery also belong to individual contracts.
Assembler decorators alone do not supply these semantics.

Retain the current SysV vector ABI policy and extend it deliberately for wide
values and mask types. Unsupported argument/result transport must diagnose even
when the body itself has legal instructions. Compiler-generated calls must
account for caller/callee SIMD and tile state. Any future APX allocation needs
separate EGPR pools, scratch and spill rules, call clobbers and ABI handling;
assembler acceptance of r16–r31 must not widen the current allocator implicitly.

### Implementation order and acceptance

1. Inventory and migrate the existing bounded bindings without changing their
   names, IDs, behavior or feature requirements. Generate recognition and header
   embedding from one source; keep handwritten semantic lowering where needed.
2. Finish existing SSE through AVX2 family gaps and scalar crypto/query wrappers.
   Add the language support required by their real vendor headers. Keep feature
   permissions explicit at every optimization level.
3. Add per-function feature permissions and cross-feature inlining/ABI checks.
   Then implement runtime CPU/OS-state queries and explicit dispatch tests.
4. Add the 512-bit and mask type/ABI/allocation foundation, then AVX-512 families
   in dependency order. Each family needs encoding, IR, frontend and semantic
   evidence before it is included in the compatibility claim.
5. Add AMX wrappers and state management after assembly support, with explicit
   tile effects and suitable runtime qualification. Treat APX compiler
   allocation and missing encoding families as their own backend work.

For every admitted binding, test declaration/header inclusion, positive calls
at `-O0` and optimized settings, required immediate diagnostics, target/feature
rejection, result types and ABI layout. Validate semantic families using pinned
Clang/GCC or independent scalar/assembly oracles, including boundary controls,
NaNs, aliasing, guard pages, masks and visible state changes where applicable.
Use text/typed/chunked object parity and decode/reassembly for encoding paths.

Compile immutable reference oracles once per fixture and reuse them across
selector/allocator/spill matrices. Execute one image when byte parity has
already established equivalent output routes. Unsupported host features produce
explicit execution skips while cross-assembly checks still run. AMX/APX/AVX-512
byte agreement is not hardware execution evidence. Family runtime qualification
must identify the machine, CPU permissions and OS state it actually exercised.
