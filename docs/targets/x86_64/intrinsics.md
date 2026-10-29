# x86 intrinsic selection

This reference describes current implementation contracts and limits. Use
[testing](../../development/testing.md) for validation commands; per-run results belong outside Git.
The [vendor compatibility design](#design-for-vendor-intrinsic-compatibility)
defines the proposed expansion beyond the implemented bounded headers.

<a id="c-x86-intrinsics-bounded-c-x86-intrinsic-subset"></a>
## Bounded C x86 intrinsic subset

Keel owns `xmmintrin.h`, `emmintrin.h`, `pmmintrin.h`, `smmintrin.h`, `avxintrin.h`,
`avx2intrin.h`, `tmmintrin.h`, `fmaintrin.h`, `f16cintrin.h`,
`wmmintrin.h`, `nmmintrin.h`, `crc32intrin.h` and `immintrin.h` headers. They are embedded in the compiler;
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
name does not promise a single matching machine instruction.
[Function-specific permissions and runtime queries](#function-permissions-and-runtime-cpu-queries)
allow explicit dispatch from a baseline function.

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
  Their `_u` typedefs retain the same size and lane representation with
  alignment 1. Unaligned integer memory interfaces use those pointer types;
  ordinary containers retain their 16/32-byte alignment.
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
- SSE2/AVX2 saturation: `_mm{,256}_{adds,subs}_{epi8,epi16,epu8,epu16}` clamps
  each mathematical sum/difference to its signed/unsigned lane range. See the
  saturation contract below.
- SSE2 packed shifts: `_mm_{slli,srli}_epi{16,32,64}` and
  `_mm_srai_epi{16,32}` with immediate controls, plus
  `_mm_{sll,srl}_epi{16,32,64}` and `_mm_sra_epi{16,32}` with vector counts.
  These are 128-bit operations; see the bounded shift contract below.
- AVX2 packed shifts: the sixteen `_mm256_` equivalents of those immediate and
  shared-count shifts, plus `_mm{,256}_{sllv,srlv}_epi{32,64}` and
  `_mm{,256}_srav_epi32`. Shared counts use an `__m128i` even for 256-bit data;
  per-lane counts have the same width as the data. See the shift contract below.
- Numeric conversions include SSE2 and AVX packed integer/float/double forms,
  scalar signed-integer conversions, scalar float/double conversions and
  signed/unsigned integer widening. See the numeric conversion contracts below.
- Integer bit operations: `_mm_{and,or,xor,andnot}_si128` and
  `_mm256_{and,or,xor,andnot}_si256`; `andnot(a,b)` means `~a & b`.
  `_mm_setzero_si128`, `_mm256_setzero_si256`,
  `_mm_{load,loadu,store,storeu}_si128`, and their `_mm256_*_si256` equivalents
  are supported. Unaligned memory spellings do not impose vector alignment or
  incompatible scalar aliasing requirements; each copies exactly its width.
- Packed integer narrowing: `_mm{,256}_{packs,packus}_epi{16,32}`.
  Averages: `_mm{,256}_avg_epu{8,16}`. Products:
  `_mm{,256}_{madd,maddubs,mulhi,mulhrs}_epi16` and
  `_mm{,256}_mulhi_epu16`. Byte shuffles: `_mm{,256}_shuffle_epi8`.
  Sign masks: `_mm{,256}_movemask_epi8`. Their architectural contracts and
  feature permissions are described below.
- Widening products: `_mm{,256}_mul_epi32` and `_mm{,256}_mul_epu32`.
  Byte sums of absolute differences: `_mm{,256}_sad_epu8`.
- Packed square roots: `_mm{,256}_sqrt_{ps,pd}`. Alternating arithmetic:
  `_mm{,256}_addsub_{ps,pd}` subtracts even lanes and adds odd lanes.
- Rounding: `_mm{,256}_{round,ceil,floor}_{ps,pd}` and
  `_mm_{round,ceil,floor}_{ss,sd}`. Variable floating blends:
  `_mm{,256}_blendv_{ps,pd}`. See the control and upper-lane contracts below.
- Horizontal integer arithmetic: `_mm{,256}_{hadd,hsub}_epi{16,32}` and
  `_mm{,256}_{hadds,hsubs}_epi16`. Horizontal floating arithmetic:
  `_mm{,256}_{hadd,hsub}_{ps,pd}`. Unpacks:
  `_mm{,256}_unpack{lo,hi}_epi{8,16,32,64}` and
  `_mm{,256}_unpack{lo,hi}_{ps,pd}`. Each 256-bit operation works separately
  inside its two 128-bit halves; see the horizontal/unpack contract below.
- All six same-width bit casts among `ps`, `pd`, and `si128` under `_mm_cast`,
  and among `ps`, `pd`, and `si256` under `_mm256_cast`.
  `_mm256_castps256_ps128`, `_mm256_castpd256_pd128`, and
  `_mm256_castsi256_si128` select the low half. Casts with an undefined upper
  half are also available; Keel chooses zero for their unspecified upper lanes.
  Numeric conversions are separate from these bit casts.
- `_MM_SHUFFLE`, `_mm_shuffle_ps`, `_mm_shuffle_pd`, `_mm_shuffle_epi32`,
  `_mm256_shuffle_ps`, `_mm256_shuffle_epi32`, and
  `_mm256_permute4x64_epi64`. AVX 32-bit shuffles operate independently within
  each 128-bit half; the 64-bit permutation can cross halves. Controls must be
  integer constant expressions. Vector operands are evaluated once.
- Integer extraction/insertion: `_mm{,256}_{extract,insert}_epi{8,16,32,64}`.
  Narrow byte/word extraction zero extends; `_mm_extract_ps` returns the
  selected float lane's integer bits. `_mm_insert_ps` selects source and
  destination lanes and applies its zero mask with an ICE control in 0..255.
  `_mm256_{extractf128,insertf128}_{ps,pd,si256}` and
  `_mm256_{extracti128,inserti128}_si256` manipulate halves. Integer lane and
  half wrappers also accept runtime indices as an extension, using the low
  index bits. Each argument is evaluated once. This extension does not promise
  immediate-diagnostic parity with vendor compilers.
- Floating comparisons include the named SSE packed/scalar forms, all 32 AVX
  predicates through `_mm_cmp_{ps,pd,ss,sd}` and `_mm256_cmp_{ps,pd}`, and the
  scalar COMI/UCOMI relations. See the floating comparison contract below.

The SSE/AVX header functions use C vectors, scalar lane access, and fixed-size
`__builtin_memcpy`; saturation, shifts, shuffles and AVX comparisons use checked builtins.
Architectural numeric conversions also use checked builtins and dedicated IR.
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

### Lane construction and broadcasts

The headers provide `_mm{,256}_setr_{ps,pd}`, the reversed integer constructors
`setr_epi{8,16,32}`, `_mm256_setr_epi64x`, and byte/word `set_epi{8,16}`
constructors. Reversed constructors place the first argument in lane zero.
`_mm{,256}_{movehdup,moveldup}_ps` duplicate odd/even lanes inside each
128-bit half; `_mm{,256}_movedup_pd` duplicates each half's low double.

`_mm256_{set,setr}_m128{,d,i}` combine two halves with the documented argument
order. `_mm256_zext{ps,pd,si}128_{ps,pd,si}256` copy the low half and zero the
high half. The matching `_mm256_cast*128_*256` spellings also choose zero for
the upper half, whose contents vendor interfaces leave unspecified.

Register broadcasts `_mm{,256}_broadcast{b,w,d,q}_epi{8,16,32,64}` duplicate the
low integer lane. `_mm{,256}_broadcastss_ps` and
`_mm{,256}_broadcastsd_pd` duplicate the low floating lane.
`_mm256_broadcastsi128_si256` and its `_mm_broadcastsi128_si256` alias repeat a
128-bit integer half. Memory broadcasts `_mm_broadcast_ss`,
`_mm256_broadcast_{ss,sd,ps,pd}` read exactly 4, 8 or 16 bytes, including
unaligned addresses. All these operations preserve lane bits, including NaN
payloads, and evaluate each argument once. Generic lane lowering may expand
without the ISA named by the vendor header; wide transport still requires AVX.

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
two halves. Dot-product and SAD controls must be integer constant expressions
in 0..255. Narrow forms require SSE4.1; wide dot product requires AVX and wide
multiple SAD requires AVX2. Each vector operand is evaluated once.

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
floating conversion. Every `_mm{,256}_undefined_*` constructor chooses a zero
vector. Vendor interfaces leave those bits unspecified, so callers must not
rely on this choice for cross-compiler equivalence.

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

### Remaining source compatibility boundary

Use the pinned headers to inspect public wrapper names without counting names
mentioned only in comments, documentation, or calls inside another wrapper:

```sh
python3 tools/x86-isa/vendor_coverage.py --reference-dir /path/to/pinned/headers
```

The checker reports definitions and macros in the seven SSE–AVX2 headers plus
`crc32intrin.h`, and compares them with compiler-owned headers and private
fragments. The pinned non-MMX public-name inventory has no missing candidates.
That is an admission result, not proof of complete signature, feature,
immediate, ABI or semantic conformance. MMX entries are reported separately.
C++ overloads, 32-bit x86, AVX-512, AMX and unlisted headers remain separate
contracts. Do not commit per-run output.

Presence in the header inventory does not grant target features or runtime
execution safety. Function attributes grant compilation permissions; explicit
runtime queries let callers choose an executable implementation.
Unmasked-exception and trap-delivery coverage remains bounded by the explicit
[FP contracts](#unmasked-fp-contract-and-remaining-hardware-exposure) below.

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
printing/parsing. The LLVM bridge currently supports Apple ARM only; x86 LLVM
admission is outside this interface. Its existing ARM attribute policy is unchanged.

Inlining must preserve permissions in both ordinary optimization and mandatory
`always_inline` expansion. The current check proves compatibility for every
possible module baseline from the caller's and callee's explicit feature edits.
It can therefore retain a call that would be safe under a particular baseline.
Mandatory inlining diagnoses when this conservative proof fails. Inherited
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

Library users keep the baseline `FeatureSelector` for codegen, then pass
`keel_codegen::x86_features::module_features(&module, baseline)` to object
assembly. That union admits all checked function bodies and the guarded XGETBV
sequence. It is an encoding permission set and must not replace the selector's
baseline. Automatic multiversion resolvers, `target_clones`, `__builtin_cpu_is`,
C++ compatibility and AVX-512/AMX runtime queries remain outside this interface.

### Reviewed builtin inventory

[`vendor-bindings.json`](../../../tools/x86-isa/vendor-bindings.json) owns the
310 reviewed frontend bindings for AES, packed integer operations and shifts,
architectural floating arithmetic, rounding and blends, SSE2 numeric conversions,
FMA and F16C.
Its generated module supplies both parser lookup and
`__has_builtin` recognition. The metadata records current semantic intrinsic
IDs, types, controls, required features and ownership. Generated assertions
preserve IDs and check reviewed signatures against the existing IR. Semantic
lowering and CPU legality remain with the existing typed operations.

[`vendor-references.json`](../../../tools/x86-isa/vendor-references.json) pins
the existing Keel contracts and LLVM/Clang 21.1.0 reference commit, licence and
eleven header hashes. The new wrappers are independently implemented against
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
AVX-512, scatter operations, full vendor-header coverage and unrestricted
FP trap/restart compatibility remain outside this C header boundary.
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
### Bounded SSE2 and AVX2 saturation

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

The eight `_mm256_` equivalents use the existing
`x86_avx2_vp{add,sub}{s,us}{b,w}256` intrinsics and require AVX2 permission.
Their 32-byte operands and results follow the same lane-wise saturation rules.
The private semantic bridges accept signed C byte/word vectors as bit containers
for both signed and unsigned operations; the intrinsic determines arithmetic
signedness. The existing unsigned 128-bit bridge contracts remain unchanged.

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
AVX2. Each public operand is evaluated once. Private
semantic bridges use the intrinsic's exact signed C lane shapes and result
shape, even for operations that interpret input bits as unsigned. These names
are Keel bridges, not GNU builtin compatibility claims. The 40 wrappers are
checked against independent scalar and vendor-header oracles, including upper
halves, signed extrema, packing boundaries, aliases and forced spills.

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
copy bits, including signed zeros and signaling-NaN payloads, without evaluating
floating-point arithmetic or changing MXCSR status. Each input expression is
evaluated once. Narrow unpacks remain legal at baseline; wide unpacks require
AVX transport and may use narrower/scalar operations without AVX2. They do not
promise one matching instruction or vendor feature-diagnostic parity.

Validation reuses the existing integer and floating intrinsic packets. Integer
horizontal operations and unpacks have independent scalar lane oracles. Floating
horizontal arithmetic compares results and masked status effects with vendor
instructions, plus scalar SSE pair operations for finite inputs. Floating
unpacks have byte-exact oracles for finite and nonfinite inputs. The packet
covers rounding modes, DAZ/FTZ, seeded flags, dead arithmetic results, aliases,
upper halves, selection/allocation routes and forced spills.

### Architectural packed square root and alternating arithmetic

`_mm{,256}_sqrt_{ps,pd}` computes each lane's square root through the existing
architectural SSE/AVX intrinsics. The 128-bit forms are legal at baseline; the
256-bit forms require AVX. They retain MXCSR rounding and status effects,
including invalid status for negative nonzero inputs and signaling NaNs, even
when the result is unused. Signed zero and NaN results follow the instruction.

`_mm{,256}_addsub_{ps,pd}` subtracts corresponding input lanes at even indices
and adds them at odd indices. The 128-bit forms require SSE3 and the 256-bit
forms require AVX. They use architectural operations with rounding and status
effects, rather than ordinary vector expressions that could discard unused
results. All eight wrappers evaluate each operand once and use the existing
16/32-byte vector ABI. The shared floating packet checks vendor results and
masked status effects, scalar SSE arithmetic oracles, aliases and forced spills
under all rounding modes, DAZ/FTZ and clear/seeded flags. These additions do not
extend the ordinary C arithmetic/comparison floating-environment contract.

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
scalar rounding operands and masks. These are architectural wrappers, with no
software fallback when a required feature is disabled.

The binding generator supplies the reviewed control bounds, normalization mask
and diagnostic for both rounding and F16C; the parser uses one checked control
path. This preserves F16C's separate 0..255 admission and ignored-bit handling.
The shared floating packet compares vendor and scalar instruction oracles,
including ties, signed zeros, nonfinite values, precision suppression, seeded
flags, scalar upper lanes, mask sign-bit controls, aliases and forced spills.

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
automatic AES dispatch, AES modes, GCM or crypto library. It makes no
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
The deployment CPU must support CPUID.01H:ECX.PCLMULQDQ[1]. An explicit
`__builtin_cpu_supports("pclmul")` check can select a function with
`target("pclmulqdq")`. ARM targets reject the intrinsic,
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

[F80 numerical operations](f80/README.md#numerical-operations) requires empty x87 tags, all six CW
exception masks set, and no pending unmasked exception; PC/RC are inherited and
CW is preserved. Its operations perform input FLD, snapshot FNSTSW immediately
after the principal opcode, then store/discard results and restore empty tags
and the entry TOP. Input loading and result storage are therefore distinct
exceptional stages. The returned status includes the temporary physical TOP; it
is not a fault-context ABI.

The other [raw x87 value intrinsics](f80/README.md#raw-x87-value-intrinsics) also
restrict their raw contracts to a masked CW. [The F80 environment contract](f80/README.md#fp-environment-contract) separately
allows unmasked exceptions to interrupt ordinary F80 sequences with temporary
operands; that value-level statement does not override the raw numerical
precondition. [Legacy images](f80/README.md#legacy-images-and-the-owned-region) admits ordinary masked
execution and flag-only unmasked CW seeds without pending exceptions, but
excludes pending-unmasked entry for waiting forms and leaves no-WAIT pending
capture/delivery unvalidated.

The [bounded x87 execution region](f80/README.md#unmasked-execution-region) is the unmasked x87
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

The implemented [function permission and CPU query contract](#function-permissions-and-runtime-cpu-queries)
provides ordered IR feature edits, C `target` attributes, legality and inlining
checks, and explicit runtime dispatch with CPU/OS-state checks for the bounded
SSE–AVX2 interface. Module feature unions admit object encoding while selectors
retain the baseline and resolve each function separately.

Automatic multiversion resolvers, C++ interfaces, richer architecture/tuning
attributes and runtime AVX-512/AMX queries need separate contracts. Extending
inlining beyond the current proof for every possible baseline would require an
explicit effective module feature context throughout the optimizer. A runtime
branch alone remains insufficient permission to emit optional instructions in
a baseline function.

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
3. Extend the implemented function permissions and bounded runtime queries only
   after defining additional ABI, state, inlining or multiversion contracts.
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
