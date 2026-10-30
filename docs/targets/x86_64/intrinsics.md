# x86 intrinsics

## C intrinsic headers

Keel owns `xmmintrin.h`, `emmintrin.h`, `pmmintrin.h`, `smmintrin.h`, `avxintrin.h`,
`avx2intrin.h`, `tmmintrin.h`, `fmaintrin.h`, `f16cintrin.h`,
`wmmintrin.h`, `shaintrin.h`, `nmmintrin.h`, `crc32intrin.h` and `immintrin.h`. They are
embedded in the compiler, so no Clang resource directory is needed, and they
target `x86_64-unknown-linux-gnu`. They form a finite subset, not complete
`immintrin.h` compatibility. Other vendor headers and intrinsics are
unsupported. The names describe x86 operations; the subset also runs on AMD
CPUs with the required features and OS vector state. XOP and FMA4 are outside
it.

Use `--target x86_64-unknown-linux-gnu --cpu-level v3` for AVX2 programs and
`--cpu-level v1` for SSE-only programs. Including a header does not enable ISA
features or emit unused inline functions. Feature legality stays with the
typed IR and backend: 256-bit transport requires AVX, and operations may expand
to legal narrower or scalar instructions when AVX2 is disabled. A header name
does not promise one matching machine instruction.
[Function-specific permissions and runtime queries](#function-permissions-and-runtime-cpu-queries)
allow explicit dispatch from a baseline function.

```c
#include <immintrin.h>
void add8(float *out, const float *a, const float *b) {
    _mm256_storeu_ps(out, _mm256_add_ps(_mm256_loadu_ps(a), _mm256_loadu_ps(b)));
}
```

`wmmintrin.h` also exposes the optional [PCLMULQDQ](#optional-legacy-pclmulqdq)
and [AES-NI](#legacy-128-bit-aes-ni) wrappers, which need explicit feature
flags at every CPU level.

### Names and general rules

The headers themselves list the supported names; run
`tools/x86-isa/vendor_coverage.py` (below) to compare them with pinned vendor
headers. These rules apply to every wrapper:

- `__m128`, `__m128d`, `__m128i`, `__m256`, `__m256d` and `__m256i` are C
  vector types. Their `_u` typedefs keep the size and lane layout with
  alignment 1, and unaligned memory wrappers use them. Integer containers have
  signed 64-bit C lanes.
- `set` arguments run from the highest lane to the lowest; `setr` arguments
  start at lane zero, which is at the lowest address.
- Memory wrappers access exactly their named width. Aligned spellings keep
  their source alignment precondition but do not require a fault on a
  misaligned pointer.
- Integer arithmetic wraps at its lane width. Comparisons use signed lanes and
  return all-one or zero masks. `andnot(a, b)` computes `~a & b`.
- 256-bit operations with 32-bit lane controls, such as `_mm256_shuffle_ps`,
  work inside each 128-bit half unless the operation crosses halves, as
  `_mm256_permute4x64_epi64` does.
- Integer lane and half extract and insert wrappers also accept runtime
  indices, using the low index bits. This is a Keel extension.
- Lanes that vendor interfaces leave undefined, such as the upper half of a
  128-to-256-bit cast or any `_mm{,256}_undefined_*` value, are zero in Keel.
  Callers must not rely on this across compilers.
- Each vector operand is evaluated once. Immediate controls must be integer
  constant expressions in the range each wrapper states.

Wrappers use C vectors, lane access and fixed-size `__builtin_memcpy`, or
checked builtins with dedicated typed IR for architectural operations. All of
them go through the ordinary IR, optimization, ABI, feature gating and
instruction selection.

### Integer family and fused/conversion wrappers

The integer headers also expose these families:

- SSE2 `_mm_{min,max}_{epi16,epu8}`.
- SSE4.1 `_mm_{min,max}_{epi8,epi32,epu16,epu32}` and `_mm_blendv_epi8`.
- SSSE3 `_mm_{abs,sign}_epi{8,16,32}` through `tmmintrin.h`.
- AVX2 `_mm256_{min,max}_{epi8,epi16,epi32,epu8,epu16,epu32}`,
  `_mm256_{abs,sign}_epi{8,16,32}` and `_mm256_blendv_epi8`.

These 38 wrappers use generic integer IR. The 128-bit forms may lower to
baseline instructions without the vendor header's named ISA family; the
256-bit forms need AVX transport. Absolute value and sign keep the
signed-minimum bit pattern. Byte blends select by the mask byte's sign bit.

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
bits 2:0; bit 2 selects MXCSR rounding. The `_MM_FROUND_*` constants are
defined in `smmintrin.h`. The `NO_EXC` bit 3 does not suppress F16C exceptions.

### Lane construction and broadcasts

The headers provide `_mm{,256}_setr_{ps,pd}`, the reversed integer constructors
`setr_epi{8,16,32}`, `_mm256_setr_epi64x`, and byte/word `set_epi{8,16}`
constructors. Reversed constructors place the first argument in lane zero.
`_mm{,256}_{movehdup,moveldup}_ps` duplicate odd/even lanes inside each
128-bit half; `_mm{,256}_movedup_pd` duplicates each half's low double.

`_mm256_{set,setr}_m128{,d,i}` combine two halves with the documented argument
order. `_mm256_zext{ps,pd,si}128_{ps,pd,si}256` copy the low half and zero the
high half.

Register broadcasts `_mm{,256}_broadcast{b,w,d,q}_epi{8,16,32,64}` duplicate the
low integer lane. `_mm{,256}_broadcastss_ps` and
`_mm{,256}_broadcastsd_pd` duplicate the low floating lane.
`_mm256_broadcastsi128_si256` and its `_mm_broadcastsi128_si256` alias repeat a
128-bit integer half. Memory broadcasts `_mm_broadcast_ss`,
`_mm256_broadcast_{ss,sd,ps,pd}` read exactly 4, 8 or 16 bytes, including
unaligned addresses. All these operations preserve lane bits, including NaN
payloads. Generic lane lowering may expand without the ISA named by the vendor
header; wide transport still requires AVX.

### Queries and specialized arithmetic

`_mm_test{z,c,nzc}_si128` require SSE4.1. `_mm256_test{z,c,nzc}_si256`
and `_mm{,256}_test{z,c,nzc}_{ps,pd}` require AVX, including the 128-bit
floating forms. Integer tests inspect every bit; floating tests inspect only
each lane's sign bit. `testz` checks that `a & b` has no selected bits, `testc`
checks `~a & b`, and `testnzc` returns one only when both intersections have a
selected bit. `_mm_test_all_ones`, `_mm_test_all_zeros`, and
`_mm_test_mix_ones_zeros` provide the corresponding convenience spellings.
These integer results do not change the FP environment, even for NaN payloads.

`_mm_dp_{ps,pd}` and `_mm256_dp_ps` use architectural dot products, retaining
MXCSR rounding and exception effects. The control selects contributing products
with its high nibble and destination lanes with its low nibble; 256-bit `ps`
works separately in each 128-bit half. `_mm_minpos_epu16` returns the unsigned
minimum and the first matching lane index, and zeroes the remaining lanes.
`_mm{,256}_mpsadbw_epu8` computes eight sliding four-byte absolute-difference
sums per 128-bit half. The wide form uses distinct three-bit controls for its
two halves. Dot-product and SAD controls are in 0..255. Narrow forms require
SSE4.1; wide dot product requires AVX and wide multiple SAD requires AVX2.

### Lane rearrangements and conveniences

Immediate blends, `alignr`, whole-register byte shifts, high/low word shuffles,
AVX immediate/variable permutations and cross-half permutations preserve exact
lane bits. Controls retain the vendor operation's 128-bit-half boundaries;
`permute2f128`, `permute2x128`, `permute4x64`, and `permutevar8x32` can move
values across halves where their operation specifies it. Byte shifts and
`alignr` zero their out-of-range source bytes rather than wrapping counts.
Immediate controls are checked constant expressions with the range defined by
the individual wrapper. Generic lowering can use narrower operations.

Scalar bit transfers, `_mm_set_{ss,sd}`, `_mm_set_{ps,pd}1`, move-low/high
helpers, `_mm_move_epi64`, signed-less-than integer aliases and AVX scalar
extraction are available. Scalar-to-integer-vector constructors zero the
remaining bits; extraction returns the selected low bits without a numeric
floating conversion.

### Floating arithmetic and comparisons

Scalar `_mm_{add,sub,mul,div}_{ss,sd}` update the low lane and copy upper lanes
from their first argument. `_mm_sqrt_ss` uses its argument's low lane;
`_mm_sqrt_sd(a,b)` computes from `b` and copies upper lanes from `a`.
`_mm{,256}_{min,max}_{ps,pd}` and `_mm_{min,max}_{ss,sd}` use architectural
operand-order semantics, including unordered operands and signed-zero ties.
They are not replacements for C `fmin`/`fmax`. `_mm{,256}_{rcp,rsqrt}_ps`
and `_mm_{rcp,rsqrt}_ss` use hardware approximation instructions, not exact
reciprocal or square-root expressions. `_mm{,256}_movemask_{ps,pd}` packs
lane sign bits into an integer without evaluating floating values.

Named SSE comparisons retain their quiet/signaling predicate behavior. The
AVX `_mm_cmp_{ps,pd,ss,sd}` and `_mm256_cmp_{ps,pd}` accept constant predicates
in 0..31, including signaling forms; all `_CMP_*` constants are defined.
True result lanes contain all ones. Scalar comparisons preserve unused lanes
from the first operand and evaluate only the low lanes. Architectural
comparison operations retain MXCSR effects even when the result is unused.

COMI/UCOMI wrappers return Clang-style C relations: unordered comparisons
return false for equality and ordered relations, and true for inequality.
Their instruction still controls exception behavior: COMI signals for quiet
or signaling NaNs, while UCOMI signals only for signaling NaNs. GCC's wrappers
can expose different unordered boolean results through direct condition-code
tests. Keel does not claim simultaneous Clang/GCC equality for those cases.
Basic scalar arithmetic follows the C frontend's ordinary FP optimization
contract; architectural comparison, min/max, estimate, square-root, fused and
rounding operations retain their dedicated IR contracts.

### Numeric conversion expansion

The headers expose the SSE2 and AVX packed `cvtepi32_{ps,pd}`,
`cvt{ps,pd}_epi32`, `cvtt{ps,pd}_epi32` and `cvt{ps,pd}_{pd,ps}` operations
for the actual supported source/result widths. These are architectural
conversions. Rounded forms read MXCSR rounding; truncating forms round toward
zero. Invalid or out-of-range floating-to-integer conversions produce the
architectural integer-indefinite bit pattern and invalid status, rather than
saturating. Narrow-result conversions zero unused result lanes as specified by
the instruction. Conversion inputs outside the selected low source lanes do
not contribute exceptions.

Scalar `_mm_cvt{ss,sd}_si{32,64}`, `_mm_cvtt{ss,sd}_si{32,64}`,
`_mm_cvtsi{32,64}_{ss,sd}`, `_mm_cvtsd_ss` and `_mm_cvtss_sd` preserve the
same rounding and status behavior. Two-input scalar conversions replace only
the first argument's low lane. Legacy SSE aliases `cvt_ss2si`, `cvtt_ss2si`
and `cvt_si2ss` name the same operations. SSE/SSE2 forms use the corresponding
baseline feature permission; the packed wide conversions require AVX.

The 128-bit `_mm_cvtepi32_ps`, `_mm_cvttps_epi32` and `_mm_cvtps_epi32` use the
pinned `x86_sse_cvtdq2ps128`, `x86_sse_cvttps2dq128` and `x86_sse_cvtps2dq128`
intrinsics, so MXCSR status effects stay even when the result is unused. With
invalid masked, NaNs, infinities and out-of-range results give
`0x80000000` and set invalid; a valid INT32_MIN gives the same bits without
invalid. DAZ, FTZ and seeded status bits follow the hardware. The wrappers
never change MXCSR controls or clear flags.

Integer widening covers signed/unsigned 8→16/32/64, 16→32/64 and 32→64
conversions at 128-bit and 256-bit result widths. They extend only the low
source lanes needed to fill the result, ignoring the rest. These wrappers use
generic integer IR and can expand without SSE4.1 or AVX2; 256-bit transport
still requires AVX. Widening is separate from packed saturating narrowing.

### Memory, streaming and machine state

Scalar and partial loads/stores access exactly their named scalar or partial
width. Loaded unused lanes are zeroed where the vendor operation specifies it;
merge loads preserve the first operand's other lanes. Reversed and duplicate
loads/stores preserve bits without arithmetic. `lddqu` spellings use exact
unaligned 16/32-byte transfers. `_mm256_loadu2_m128{,d,i}` and
`_mm256_storeu2_m128{,d,i}` access two distinct 16-byte ranges. The first
pointer names the high half and the second the low half; stores write the low
half first. Source vectors are captured before aliasing destination writes.

`_mm{,256}_maskload_{ps,pd}` and `_mm{,256}_maskstore_{ps,pd}` require AVX.
The integer `epi32`/`epi64` forms require AVX2. A mask lane's sign bit enables
its memory access. Inactive loads return zero and inactive stores leave memory
unchanged; inactive lanes suppress accesses and faults. These use architectural
memory operations, not a full load followed by a select. `_mm_maskmoveu_si128`
uses SSE2 byte-mask non-temporal stores with its existing architectural
memory/fault contract.

The 128/256-bit AVX2 `i32gather`/`i64gather` forms cover `ps`, `pd`, `epi32`
and `epi64`, with masked counterparts. Indices are signed and scale must be a
constant 1, 2, 4 or 8. Masked-off lanes preserve the source lane and suppress
memory accesses and faults. Mixed index/result widths retain their
architectural active-lane count and unused-result-lane behavior. These
interfaces do not model signal-handler restart after a partially completed
faulting gather.

Streaming stores and `_mm{,256}_stream_load_si{128,256}` use architectural
non-temporal operations, including their alignment and memory-type conditions.
`_mm_{sfence,lfence,mfence}` preserve their ordering effects, and `_mm_pause`
emits the processor hint. `_mm_prefetch` maps the named `_MM_HINT_*` controls
to the existing advisory prefetch builtin, which may emit no instruction.
It does not promise a cache access or ordering effect.

`_mm_getcsr`, `_mm_setcsr`, and `_MM_GET/SET_*` field helpers read/write MXCSR,
including rounding, exception status/masks, FTZ and DAZ. Callers must supply a
valid supported MXCSR bit pattern. `_mm256_zeroall` and `_mm256_zeroupper`
require AVX; code generation preserves live language values across their
register clobbers, so later code may reload vector state.

SSE3 `_mm_monitor` and `_mm_mwait` are available only with explicit `+monitor`
permission. Their assembler/codegen boundary is covered; native execution and
OS/privilege permission are not established by accepting the wrapper or CPU
feature. They must not be executed solely because SSE3 is enabled.

### SSE4.2 strings and CRC32C

`nmmintrin.h` forwards to the shared SSE4 definitions in `smmintrin.h`.
`_mm_cmpistr{a,c,i,m,o,s,z}` and `_mm_cmpestr{a,c,i,m,o,s,z}` provide implicit
and explicit signed-length string comparisons. The `_SIDD_*` constants select
byte/word signedness, aggregation, polarity and mask/index format. Controls
are integer constant expressions in 0..255; the high unused control bit retains
its architectural ignored-bit behavior. Explicit lengths use signed 32-bit
inputs and architectural absolute-value saturation, including `INT_MIN`.
Index, mask and flag wrappers use their corresponding architectural results.
Each vector and length argument is evaluated once.

`crc32intrin.h` exposes `_mm_crc32_u{8,16,32,64}`. These update CRC32C using
the instruction's reflected Castagnoli polynomial; they are not the IEEE CRC32
polynomial. No initial/final complement is added. The 64-bit input form consumes
the accumulator's low 32 bits and zero extends the 32-bit result. String and
CRC wrappers require SSE4.2 and do not grant permission on header inclusion.

### Vendor name coverage

Use the pinned headers to inspect public wrapper names without counting names
mentioned only in comments, documentation, or calls inside another wrapper:

```sh
python3 tools/x86-isa/vendor_coverage.py --reference-dir /path/to/pinned/headers
```

The checker reports definitions and macros in the seven SSE–AVX2 headers plus
`crc32intrin.h`, and compares them with compiler-owned headers and private
fragments. The pinned non-MMX public-name inventory has no missing candidates.
That is a name check, not proof of signature, feature, immediate, ABI or
semantic conformance. MMX entries are reported separately. C++ overloads,
32-bit x86, unlisted AVX-512 intrinsics, AMX and unlisted headers are outside
the subset.

### Function permissions and runtime CPU queries

On `x86_64-unknown-linux-gnu`, function declarations and definitions accept
`__attribute__((target("avx2,no-fma")))`. Names use the existing feature policy;
GNU spellings such as `sse4.1` normalize to Keel's feature names. Ordered edits
apply to the module baseline and must leave valid prerequisites. Unknown names,
architecture/tuning selections and attributes on non-function objects diagnose.
`__has_attribute(target)` advertises this interface on x86-64. Predefined feature
macros continue to describe the translation unit's baseline.

A baseline function can call a feature-specific implementation through a scalar
or pointer interface. Each function's selector and legality checks use its own
resolved permissions. A wide-vector argument/result still needs compatible
transport permissions at both ends of the call; a feature attribute does not
supply that ABI permission to its caller. These requests survive IR text
printing and parsing.

Inlining must preserve permissions in both ordinary optimization and mandatory
`always_inline` expansion. The check proves compatibility for every possible
module baseline from the caller's and callee's explicit feature edits, so it can
keep a call that a particular baseline would allow. Mandatory inlining
diagnoses when the proof fails. Inherited
header bodies inline into the caller's feature context, including explicit
feature disables, and their operations are checked against that context.

`__builtin_cpu_supports("avx2")` returns an `int`, zero or one. Its argument must
be a supported string literal. Supported names are `sse`, `sse2`, `sse3`,
`ssse3`, `sse4.1`, `sse4.2`, `avx`, `avx2`, `aes`, `pclmul`, `popcnt`, `bmi`,
`bmi2`, `cx16`, `movbe`, `lzcnt`, `xsave`, `fma`, `f16c` and `monitor`.
Unknown names, runtime strings and non-x86 targets diagnose. Recognition queries
for `__builtin_cpu_supports` and `__builtin_cpu_init` are independent of the
selected baseline.

Each call checks the maximum CPUID leaf before reading the selected feature
bits. `avx`, `avx2`, `fma` and `f16c` also require CPUID's XSAVE, OSXSAVE and AVX
bits and XCR0 bits 1 and 2. The guarded XCR0 intrinsic executes XGETBV only after
checking CPUID leaf availability and CPU/OS XSAVE permission. `xsave` requires
both XSAVE and OSXSAVE. `monitor` reports its CPU feature bit; it does not prove
OS enablement or privilege to execute MONITOR/MWAIT.

`__builtin_cpu_init()` takes no arguments and returns void. Queries read the
machine directly, with no shared cache or constructor dependency, so initialization
emits no code. A positive query does not grant compilation permissions to an
arbitrary branch. Put optional instructions in an attributed function and call
it after the runtime check, as in this pointer-interface example:

```c
__attribute__((target("avx2"), noinline))
void add_avx2(int *out, const int *a, const int *b);
void add_scalar(int *out, const int *a, const int *b);
void add(int *out, const int *a, const int *b) {
    if (__builtin_cpu_supports("avx2")) add_avx2(out, a, b);
    else add_scalar(out, a, b);
}
```

Library users keep the baseline `FeatureSelector` for codegen and pass
`keel_codegen::x86_features::module_features(&module, baseline)` to object
assembly. Automatic multiversion resolvers, `target_clones`, `__builtin_cpu_is`,
C++ compatibility and AVX-512/AMX runtime queries remain outside this interface.

### Reviewed builtin inventory

[`vendor-bindings.json`](../../../tools/x86-isa/vendor-bindings.json) owns the
reviewed frontend builtin bindings, and
[`vendor-references.json`](../../../tools/x86-isa/vendor-references.json) pins
the reference headers. See the [generator reference](../../../tools/x86-isa/README.md)
for regeneration. Keel-private bridge names are separate from vendor builtin
spellings, and no general GNU or Clang `__builtin_ia32_*` compatibility is
claimed.

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
CPU features.

Unsupported sizes/types, unequal-width vector casts, unsupported comparison
predicates, and illegal target features fail with diagnostics. Ordinary vector
expressions support bounded lossless scalar broadcasts for arithmetic, integer
bitwise operations and comparisons, including compound assignments; see
[GNU vector mode](../../frontends/c/extensions.md#gnu-vector-mode-scalar-broadcasts) for conversion checks
and remaining scalar-shift/cross-domain limits. Constructor functions remain
available for explicit element conversions. AVX-512 support is limited to the
operations listed here. The C restrictions on vector varargs, unions and by-value x86
aggregates apply; see [GNU vector mode](../../frontends/c/extensions.md).

<a id="c-x86-intrinsics-bounded-sse2-packed-shifts"></a>
### Packed shifts

The sixteen shift wrappers above use the existing architectural
`x86_sse2_ps{ll,rl,ra}{w,d,q}128` intrinsics (arithmetic right shift has no
64-bit form). Logical shifts zero every lane when the unsigned count is at
least the lane width; arithmetic right shifts sign-fill instead. Counts are
never reduced modulo the width. These semantics differ from ordinary C vector
shifts, whose out-of-range counts are undefined.

Immediate controls must be integer constant expressions in 0..255; other
values and runtime controls are diagnosed. An immediate wrapper does not
promise an immediate machine encoding: the IR uses an i64 count. The runtime
forms consume only the unsigned low 64 bits of the count vector, so a low half
with its sign bit set is a large positive count. The private
`__builtin_keel_sse2_{imm,count}_*` bridges exist only on x86-64.

AVX2 adds sixteen `_mm256_` immediate/shared-count wrappers through the existing
`x86_avx2_vps{ll,rl,ra}{w,d,q}256` intrinsics. Data and results are 32 bytes;
the shared count is still an `__m128i` and only its unsigned low 64 bits matter.
Immediate controls use the same 0..255 rule as SSE2.

The ten per-lane wrappers `_mm{,256}_{sllv,srlv}_epi{32,64}` and
`_mm{,256}_srav_epi32` use the existing AVX2 variable-shift intrinsics. Each
32/64-bit count lane is an unsigned count for its corresponding data lane.
All count bits matter; counts at or above the lane width zero or sign-fill the
result. There is no 16-bit per-lane or 64-bit arithmetic-right form. All
these AVX2 shifts, including the 128-bit per-lane forms, require AVX2
permission and have no software fallback.

<a id="c-x86-intrinsics-bounded-sse2-saturation"></a>
### Saturation

`_mm_adds_epi8`, `_mm_adds_epi16`, `_mm_adds_epu8`, `_mm_adds_epu16`,
`_mm_subs_epi8`, `_mm_subs_epi16`, `_mm_subs_epu8`, `_mm_subs_epu16`
accept and return `__m128i`, evaluating each argument once. Byte/word lanes
clamp to -128..127 / -32768..32767 for signed operations and 0..255 /
0..65535 for unsigned operations. Subtraction keeps operand order. The wrappers call the typed
`x86_sse2_p{add,sub}{s,us}{b,w}128` intrinsics, which select legacy SSE2
instructions. The private `__builtin_keel_sse2_p{add,sub}{s,us}{b,w}` bridges
require two matching 16-byte vectors of signed char, short, unsigned char or
unsigned short; plain char is distinct from signed char. They exist only on
x86-64.

The eight `_mm256_` equivalents use the existing
`x86_avx2_vp{add,sub}{s,us}{b,w}256` intrinsics and require AVX2 permission.
Their 32-byte operands and results follow the same lane-wise saturation rules.

### Packed narrowing, products, averages and byte selection

These wrappers call existing architectural operations, with no scalar fallback
when their required feature is disabled:

- `_mm{,256}_{packs,packus}_epi{16,32}` interprets input lanes as signed and
  clamps to signed or unsigned lanes of half the width. AVX2 packs concatenate
  the two narrowed inputs separately inside each 128-bit half. The 128-bit
  `_mm_packus_epi32` requires SSE4.1; the other 128-bit packs require SSE2.
- `_mm{,256}_avg_epu{8,16}` computes the unsigned average rounded upward without
  narrow overflow.
- `_mm{,256}_madd_epi16` adds pairs of signed word products and wraps the result
  to 32 bits, including the all-INT16_MIN case. `_mm{,256}_maddubs_epi16`
  multiplies unsigned first-input bytes by signed second-input bytes and clamps
  each pair sum to a signed word.
- `_mm{,256}_mulhi_epi16` and `_mm{,256}_mulhi_epu16` return the high 16 bits of
  signed or unsigned products. `_mm{,256}_mulhrs_epi16` rounds signed products
  by adding 16384 before an arithmetic shift by 15 and retaining 16 bits. The
  INT16_MIN-times-INT16_MIN result is `0x8000`, without saturation.
- `_mm{,256}_shuffle_epi8` selects each result byte using the corresponding
  control byte's low four bits within its 128-bit half. Control bit 7 zeros the
  result; bits 6:4 are ignored.
- `_mm{,256}_movemask_epi8` returns an `int` with each input byte's sign bit in
  the corresponding result bit. The 256-bit form can set bit 31.
- `_mm{,256}_mul_epi32` and `_mm{,256}_mul_epu32` multiply the even 32-bit
  lanes into full 64-bit signed/unsigned products. Odd input lanes are ignored.
  The signed 128-bit form requires SSE4.1; the unsigned form requires SSE2.
- `_mm{,256}_sad_epu8` sums unsigned byte absolute differences in independent
  eight-byte groups. Each sum occupies a zero-extended 64-bit result lane. The
  128-bit form requires SSE2.

The 128-bit mixed-byte multiply-add, rounded multiply and byte shuffle require
SSSE3. The signed 128-bit widening product requires SSE4.1. Other 128-bit
products, averages and sign masks require SSE2. All listed 256-bit forms require
AVX2.

### Horizontal arithmetic and unpacking

The twelve integer horizontal wrappers use the existing SSSE3/AVX2 packed-pair
intrinsics. Each input contributes adjacent lane pairs, with the first input's
results before the second's within each 128-bit half. `_hadd_` and `_hsub_`
wrap at the 16/32-bit lane width. `_hadds_` and `_hsubs_` clamp to signed
16-bit range. The 128-bit forms require SSSE3; the 256-bit forms require AVX2.

The eight floating horizontal wrappers use architectural SSE3/AVX operations,
not reassociated vector expressions. They follow the same lane-pair layout,
require SSE3 at 128 bits or AVX at 256 bits, and retain MXCSR rounding and
status effects even when the result is unused. Definitions of the 128-bit forms
live in `pmmintrin.h`, which is also included through the higher-level headers.
Inclusion alone remains legal at baseline.

The 24 integer/floating unpack wrappers interleave the selected low or high
half of their inputs inside each 128-bit block. They use typed shuffles and
copy bits, including signed zeros and signaling-NaN payloads, without
floating arithmetic or MXCSR changes. Narrow unpacks are legal at baseline;
wide unpacks require AVX transport.

### Packed square root and alternating arithmetic

`_mm{,256}_sqrt_{ps,pd}` computes each lane's square root through the existing
architectural SSE/AVX intrinsics. The 128-bit forms are legal at baseline; the
256-bit forms require AVX. They retain MXCSR rounding and status effects,
including invalid status for negative nonzero inputs and signaling NaNs, even
when the result is unused. Signed zero and NaN results follow the instruction.

`_mm{,256}_addsub_{ps,pd}` subtracts corresponding input lanes at even indices
and adds them at odd indices. The 128-bit forms require SSE3 and the 256-bit
forms require AVX. They use architectural operations with rounding and status
effects, rather than ordinary vector expressions that could discard unused
results.

### Rounding and variable floating blends

`_mm{,256}_round_{ps,pd}` rounds packed floating lanes to integral floating
values. `_mm_round_ss(a,b,mode)` and `_mm_round_sd(a,b,mode)` round only the low
lane of `b` and copy the remaining lanes from `a`. Unused lanes are copied as
bits, including signaling-NaN payloads, without contributing exceptions.

The control must be an integer constant expression in 0..15. Bits 1:0 select
nearest-even, downward, upward or toward-zero rounding. Bit 2 instead selects
MXCSR.RC, ignoring bits 1:0. Bit 3 suppresses the precision exception, while
signaling NaNs can still raise invalid. The wrappers retain status effects even
when their results are unused. `_mm{,256}_{ceil,floor}_{ps,pd}` and
`_mm_{ceil,floor}_{ss,sd}` use the corresponding fixed rounding controls and
retain the same upper-lane and exception behavior. The 128-bit forms require
SSE4.1; packed 256-bit forms require AVX.

`_mm{,256}_blendv_{ps,pd}(a,b,mask)` selects `b` when a mask lane's sign bit is
set, otherwise `a`. Other mask bits have no effect. The 128-bit forms require
SSE4.1 and the 256-bit forms require AVX. Selection copies bits without floating
arithmetic or status effects. Each public operand is evaluated once, including
scalar rounding operands and masks. There is no software fallback when a
required feature is disabled.

## Legacy 128-bit AES-NI

The six legacy AES-NI operations need the independent `aes` feature:
`--target-features +aes` for `asm`, `compile` or `cc`, or
`Features::for_level(level).with_features("+aes")` with `FeatureSelector`. No
CPU level enables it, and `-aes` disables it again. `+aes` enables nothing
else. The deployment CPU must report CPUID.01H:ECX.AES[25]; Keel inserts no
runtime probe or fallback.

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

Legacy memory sources access exactly 16 bytes and require 16-byte alignment.
Register intrinsics keep explicit loads, including unaligned loads, instead of
folding them into an alignment-requiring source. Lowering stages inputs in
reserved scratch XMM registers before writing the destination. ARM backends
reject these operations.

### C wrappers

`<wmmintrin.h>` and `<immintrin.h>` expose these six wrappers using the existing
`__m128i` container:

- `_mm_aesenc_si128(a,b)`, `_mm_aesenclast_si128(a,b)`
- `_mm_aesdec_si128(a,b)`, `_mm_aesdeclast_si128(a,b)`
- `_mm_aesimc_si128(a)`
- `_mm_aeskeygenassist_si128(a,control)`

The AES-owned definitions live in `<__keel_aes.h>`, a fragment of the shared
`wmmintrin.h` surface. Each macro evaluates each vector operand once and maps
to the corresponding checked `__builtin_ia32_<instruction>128` builtin.
Operands must have the signed long-long 16-byte vector type of `__m128i`.
AESKEYGENASSIST's control must be an integer constant expression in 0..255
under the [bounded constant evaluator](#c-wrapper). `__has_builtin` reports
frontend recognition on x86 regardless of CPU features and is false on ARM,
where the builtins and header are diagnosed.

There is no VAES, automatic AES recognition or dispatch, AES mode, GCM or
crypto library, and no constant-time or security claim.

### Tests

```sh
cargo test -p keel-codegen --test x86_scalar_intrinsics x86_aes::
cargo test -p keel-c --test x86_intrinsics
cargo test -p keel --test x86_aes
cargo test -p keel-codegen --release --lib feature_guard_tests
```

The codegen test checks all six operations against an independent GF(256)
software oracle and the FIPS 197 AES-128 example. Native execution needs an
AES-capable x86-64 Linux host; on macOS arm64 the tests cover encoding,
frontend and feature diagnostics only.

## AVX512F wrappers

`immintrin.h` provides 512-bit zero and broadcast constructors, equal-width
casts, aligned and unaligned loads/stores, dword/qword add/subtract, dword
low multiply, and integer bitwise operations. Unmasked floating add, subtract,
multiply, divide and square root use the raw masked operations with every
lane enabled. Each argument is evaluated once. All memory accesses cover
exactly 64 bytes.

### Scalar masks

`immintrin.h` defines `__mmask16` for sixteen dword lanes and `__mmask8`
for eight qword lanes. Bit zero controls the lowest-addressed lane. The
wrappers require `avx512f` permission in the enclosing function.

| Wrappers | Behavior |
| --- | --- |
| `_mm512_cmp_epi32_mask`, `_mm512_cmp_epi64_mask` | Signed comparison to a scalar mask |
| `_mm512_cmp_epu32_mask`, `_mm512_cmp_epu64_mask` | Unsigned comparison to a scalar mask |
| `_mm512_mask_add_epi32`, `_mm512_mask_add_epi64` | Add active lanes; preserve `src` elsewhere |
| `_mm512_mask_sub_epi32`, `_mm512_mask_sub_epi64` | Subtract active lanes; preserve `src` elsewhere |
| `_mm512_mask_loadu_epi32`, `_mm512_mask_loadu_epi64` | Load active lanes; preserve `src` elsewhere |
| `_mm512_mask_storeu_epi32`, `_mm512_mask_storeu_epi64` | Store active lanes only |

Comparison predicates must be integer constant expressions in 0..7:
equal, less than, less or equal, false, not equal, greater or equal,
greater than, and true, respectively. Masked memory accepts unaligned
addresses. Inactive lanes do not access memory. The wrappers use Keel-private
builtins with reviewed typed signatures; they do not claim vendor builtin
compatibility.

The floating wrappers use the same scalar masks: `ps` has sixteen lanes
and `pd` has eight. `_mm512_mask_cmp_ps_mask` and
`_mm512_mask_cmp_pd_mask` accept an input mask followed by two vectors and a
constant predicate in 0..31. Their unmasked `_mm512_cmp_ps_mask` and
`_mm512_cmp_pd_mask` forms enable every lane. Predicates retain the x86
ordered/unordered and signaling distinctions.

`_mm512_mask_{add,sub,mul,div}_{ps,pd}` and
`_mm512_mask_sqrt_{ps,pd}` preserve the merge source in inactive lanes.
Inactive lanes suppress floating exceptions. Active arithmetic observes
MXCSR; these raw operations remain pinned through optimization.
`_mm512_mask_loadu_{ps,pd}` and `_mm512_mask_storeu_{ps,pd}` access only
active lanes and accept unaligned addresses. All these wrappers require
AVX512F permission.

### Indexed memory

AVX512F supports eight gather and eight scatter forms. Dword or qword
signed indices address dword/qword integer or single/double floating elements.
The data and index vectors have the widths required by each instruction;
qword-index dword-data forms use eight data lanes. Scale is an integer constant
1, 2, 4 or 8. Scalar mask bits select active lanes. Inactive lanes suppress
memory access, while inactive gather lanes preserve the merge source.
SSA masks and indices remain unchanged after the operation. A fault may leave
partial memory effects, as on the instruction itself. The intrinsics do not
provide a signal-handler resume ABI or expose partially cleared hardware masks.

### Compression and expansion

The typed `x86_avx512_compress{d,q,ps,pd}` intrinsics take a merge vector,
scalar mask and input vector. They pack selected input lanes into the low
result lanes in source order. Result lanes above `popcount(mask)` preserve
the corresponding merge lanes. `x86_avx512_expand{d,q,ps,pd}` has the same
signature and consumes the packed input prefix into selected result lanes;
inactive result lanes preserve the merge source.

`x86_avx512_compressstore{d,q,ps,pd}(ptr, value, mask)` writes the packed
prefix. `x86_avx512_expandload{d,q,ps,pd}(ptr, merge, mask)` reads a packed
prefix and returns the expanded vector. Memory forms access exactly
`popcount(mask)` contiguous elements, even when mask bits select high lanes.
A zero mask suppresses every memory access. Addresses may be unaligned.
Dword/single forms use sixteen lanes and `i16` masks; qword/double forms use
eight lanes and `i8` masks. All vectors are 512 bits. These bit transfers
preserve floating NaN payloads and require AVX512F without AVX2 or AVX512VL.

## SHA-256

`shaintrin.h`, also included by `immintrin.h`, exposes
`_mm_sha256msg1_epu32`, `_mm_sha256msg2_epu32` and `_mm_sha256rnds2_epu32`.
The wrappers accept and return `__m128i`; the typed builtins use `i32x4`.

| IR intrinsic | Signature |
| --- | --- |
| `x86_sha256msg1` | `i32x4, i32x4 → i32x4` |
| `x86_sha256msg2` | `i32x4, i32x4 → i32x4` |
| `x86_sha256rnds2` | `i32x4, i32x4, i32x4 → i32x4` |

These pure operations require the independent `sha` feature in the enclosing
function. Header inclusion grants no feature permission. The backend stages
explicit operands before loading SHA256RNDS2's implicit XMM0 operand, and
preserves other live values in XMM0, including its upper YMM/ZMM lanes.

## Optional legacy PCLMULQDQ

PCLMULQDQ is an independent x86-64 extension that is not part of psABI v3, and
no CPU level enables it. The modifier is `pclmulqdq`, with alias `pclmul`:

```sh
keel cc --target x86_64-unknown-linux-gnu --cpu-level v1 \
  --target-features +pclmulqdq input.c -o output.s
```

`compile`, `cc` and `asm` share this modifier. `-pclmulqdq` explicitly disables
it again. Library callers use `Features::for_level(level).with_features("+pclmulqdq")`
with `FeatureSelector`, and the same features when assembling objects.
The deployment CPU must report CPUID.01H:ECX.PCLMULQDQ[1]. An explicit
`__builtin_cpu_supports("pclmul")` check can select a function with
`target("pclmulqdq")`. ARM targets reject the intrinsic,
feature selection and owned x86 header.

### Operation

Only legacy `66 0F 3A 44 /r ib` is supported:

```asm
pclmulqdq xmm8, xmm15, 255
pclmulqdq xmm8, xmmword ptr [r13+r12*4-128], 16
```

The destination is also the first source. The second source is XMM or m128,
and a memory operand reads 16 bytes even when the control selects its low
half. The control is a constant in 0..255.

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
It is a pure register operation. Lowering stages both operands in reserved
SIMD scratch registers before destroying the first, and it folds no loads.

### C wrapper

```c
#include <wmmintrin.h>
__m128i product(__m128i a, __m128i b) {
    return _mm_clmulepi64_si128(a, b, 0x11);
}
```

`wmmintrin.h` exposes `_mm_clmulepi64_si128` as a macro. It calls the checked builtin
`__builtin_ia32_pclmulqdq128(a,b,imm8)`, accepting matching 64-bit-lane integer
vectors. All 256 controls are legal. Unsupported/dynamic/out-of-range controls
and mismatched vector types are diagnosed. The bounded constant evaluator accepts
integer expressions through 64-bit integers,
including enum constants, casts, `sizeof`, conditionals and short-circuit logic.
A floating literal is accepted only as the immediate operand of a representable
integer cast; floating arithmetic/comparisons are outside this ICE subset.
Evaluated signed overflow, invalid shift counts, division overflow/zero and
unrepresentable floating casts are rejected before folding. i128 and pointer
intermediates and comma expressions are rejected. AES-NI uses the same
evaluator.

`__has_builtin(__builtin_ia32_pclmulqdq128)` reports frontend support on x86-64
regardless of CPU features and is false on ARM. Other CLMUL widths and
automatic polynomial recognition are unsupported.

### Tests

```sh
cargo test -p keel-asm --test x86_extensions x86_pclmul::
cargo test -p keel-codegen --test x86_scalar_intrinsics x86_pclmul::
cargo test -p keel-c --test x86_intrinsics pclmul::
cargo test -p keel --test x86_cpu_level
cargo test -p keel-codegen --lib feature_guard_tests
```

Run the feature guard in `--release` as well. The native harness runs on
x86-64 Linux with PCLMUL and compares against an independent XOR-and-shift
reference.

Primary semantics references: [Intel SDM, volume 2B](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf),
[Intel carryless multiplication paper](https://cdrdv2-public.intel.com/836172/clmul-wp-rev-2-02-2014-04-20.pdf),
and [Clang's PCLMUL header](https://clang.llvm.org/doxygen/____wmmintrin__pclmul_8h.html).

## Raw integer division

`x86_div_u8/u16/u32/u64` and `x86_div_s8/s16/s32/s64` have the signature
`(high: Iw, low: Iw, divisor: Iw) -> (quotient: Iw, remainder: Iw)`. The
dividend is the 2w-bit concatenation `high:low`. Signed forms read it as two's
complement and the divisor as signed w bits; they truncate the quotient toward
zero and give the remainder the dividend's sign. The high half need not be a
sign extension of the low half.

Each lowers to exactly one DIV or IDIV with a register divisor. A zero divisor
or a quotient outside the destination range raises #DE at that instruction;
there is no guard, UD2 or helper call. Both results are published only after
the instruction succeeds. The operations are pinned memory barriers, including
when both results are dead, and are baseline x86-64. AArch64, LLVM and
Cranelift reject them. Generic division and remainder are unchanged, including
signed `MIN % -1 = 0`.

All three inputs are captured in reserved RDX, RAX and RCX before either output
is written. For w=8, `high:low` is packed into AX; the quotient is AL and the
remainder is extracted from AX without an AH operand. Scratch registers are not
a public ABI, and result bits above w are not part of the API. DIV and IDIV
leave CF, OF, SF, ZF, AF and PF undefined, and no flags value is returned.
Linux reports a fault as SIGFPE with FPE_INTDIV and the saved RIP at the
instruction; resumption is not part of the contract. Semantics follow the DIV
and IDIV entries of the
[Intel SDM Vol. 2A](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf).

## Unmasked SSE and AVX exceptions

The raw SSE and AVX conversion, miscellaneous and comparison intrinsics have
`Intrinsic::info().pure == false`, so `InstData::is_pinned` pins them,
`InstData::memory_effects` classifies them as `Barrier` and
`InstData::may_trap` allows traps. These are compiler barriers, not hardware
fences. The raw families have no all-masks-set precondition, so they need no
separate IDs or lowering for an unmasked environment. Portable FP has no
exception-effect contract; see [SIMD IR](../../compiler/simd-ir.md).

The raw FMA3, F16C, SSE3 and SSE4.1 FP, and MMX-sized value intrinsics are
also impure and pinned, but their per-form exception behavior under unmasked
controls is not validated. The x87 unmasked path is the
[bounded x87 execution region](f80/README.md#unmasked-execution-region); the
other raw x87 intrinsics require all exceptions masked. Arbitrary nonempty x87
entry and signal-context restart are outside these contracts.
