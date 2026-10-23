# Bounded C x86 intrinsic subset

Keel owns small `xmmintrin.h`, `emmintrin.h`, `smmintrin.h`, `avxintrin.h`,
`avx2intrin.h`, `wmmintrin.h` and `immintrin.h` headers. They are embedded in the compiler;
no Clang resource directory is needed. These headers are for the
`x86_64-unknown-linux-gnu` target. This is a useful, finite source-compatibility
subset, **not complete `immintrin.h` compatibility** or general C
auto-vectorization. Other vendor headers and intrinsics remain unsupported.

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
[PCLMULQDQ contract](x86-pclmulqdq.md) for controls, semantics and finite validation.

The optional [legacy AES-NI subset](x86-aes-ni.md) adds six `__m128i`
wrappers: `_mm_aesenc_si128`, `_mm_aesenclast_si128`, `_mm_aesdec_si128`,
`_mm_aesdeclast_si128`, `_mm_aesimc_si128` and `_mm_aeskeygenassist_si128`.
They require explicit `--target-features +aes` at every CPU level, use checked
builtins, and have no scalar crypto fallback. The key-assist control is an
integer constant expression in 0..255. `__has_builtin` reports frontend
recognition independently of CPU enablement. AES definitions reside in the
AES-owned `__keel_aes.h` fragment of the shared `wmmintrin.h` header.

## Supported names

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

## Builtins and diagnostics

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
[GNU vector mode](gnu-vector-mode.md#scalar-broadcasts) for conversion checks
and remaining scalar-shift/cross-domain limits. Constructor functions remain
available for explicit element conversions.
Masked/gather/scatter memory, streaming memory/fences, scalar-lane FP intrinsics,
saturation beyond the bounded SSE2 subset, horizontal operations, shifts beyond
the bounded SSE2 subset,
conversions beyond the three SSE2 forms below, FMA, AVX-512,
full vendor header coverage and strict FP-environment compatibility are outside
this header contract even where the backend already implements such operations.
The existing C restrictions on vector varargs, unions and by-value x86 aggregates
remain in force; see [GNU vector mode](gnu-vector-mode.md).

## Bounded SSE2 packed shifts

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
The implementation reuses the reviewed AES/PCLMUL bounded ICE proof unchanged.
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

`keel-c --test x86_shifts` separates frontend/IR/object checks from conditional
native Linux x86 execution. On LinuxHost, debug and release both passed with GCC
16.2.0 and Clang 23.1.1 against their vendor headers and an independent unsigned
lane reference. Runtime counts cover every value 0..255, 256, 65536, 2^32,
2^63 and UINT64_MAX, with a nonzero ignored upper count half. Lane patterns
include zero, all ones, signed minimum/maximum, one and mixed bits. Immediate
forms cover 0, width-1, width, width+1 and 255. Baseline/fast, trees/full and
optimized/full with empty register pools execute values, single evaluation,
direct/indirect calls and callbacks. Guard pages and sentinels check both page
edges, unaligned pointers and aliased input/output with exact 16-byte accesses.

Independent `llvm-objdump` decoding of the six native debug/release objects
found all eight legacy SSE2 shift mnemonics. The complete finite mnemonic
inventories contained only baseline integer/SSE/SSE2 instructions. This decoding
and native execution supplement the typed feature checks; they do not establish
execution on a CPUID-masked v1-only host. Existing C headers, AES and PCLMUL
regressions passed on LinuxHost in both profiles. Local Apple ARM64 frontend/object,
header/AES/PCLMUL and vector ABI/broadcast regressions also passed; local x86
object emission alone is not native x86 execution.

Reproduce under the shared build lock (add `--release` for release):

```sh
python3 suite/fast_opt_locked.py /tmp/keel-c-shifts-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-c \
  --test x86_shifts --test x86_intrinsics --test x86_aes --test pclmul
```

The 2026-10-23 native source snapshot is `/tmp/keel-c-shifts-20261023` on
LinuxHost. Logs are `/tmp/keel-c-shifts-native-{debug,debug-final,release}.log`;
final objects and harnesses are `/tmp/keel-c-shifts-{3124846,3125706}`.
Disassembly and inventories are retained beside those objects and in
`/tmp/keel-c-shifts-disassembly.json`; the decoder is
`/tmp/keel-c-shifts-decode.py`. Builds/execution used the shared suite lock and
cargo default cache with link caching disabled. No timing conclusions were drawn.

An independent review reapplied candidate `9f1fd535` (implementation
`76a18e1b`) onto main `4611704b` in an isolated worktree. No blocking source
semantic defect was found. The review added private-bridge arity/type and all-name
recognition checks on x86/ARM, further defined/undefined ICE boundaries,
both header inclusion orders with AES/PCLMUL, and effective-feature checks.
A direct frontend-to-codegen path exercises the existing legality checks without
an intervening explicit IR verification call. The native harness additionally
checks output aliasing the runtime count buffer.

Fresh review runs passed `x86_shifts`, `x86_intrinsics`, `x86_aes` and `pclmul`
in debug and release: 13 tests per profile on Apple ARM64 (frontend, IR and
object checks plus applicable native controls), and 15 per profile on LinuxHost
(including GCC 16.2.0/Clang 23.1.1 shift execution). All runs used the shared lock
and the documented cargo settings. Local logs are
`/tmp/keel-shifts-review-final-{debug,release}.log`; LinuxHost logs and lock records
are `/tmp/keel-shifts-independent-final-{debug,release}.{log,jsonl}`. The reviewed
native snapshot is `/tmp/keel-shifts-independent-review-36692`; debug/release
objects and harnesses are `/tmp/keel-c-shifts-{3224215,3225064}`. Independent
`llvm-objdump` decoding found all eight legacy shifts in each of the six objects;
the full finite mnemonic inventories contain only baseline integer/SSE/SSE2
instructions. Object hashes, inventories and decoder are retained in
`/tmp/keel-shifts-independent-disassembly.json` and
`/tmp/keel-shifts-independent-decode.py` on LinuxHost. These are bounded
frontend/lowering, encoding and native results; the newer execution host does
not establish v1-only CPU behavior. No timing or speedup claim is made.

AVX2 256-bit shifts, per-lane counts, byte shifts, rotates and the remainder of
the vendor shift families remain queued. Bounded saturation and conversion
wrappers are documented separately here.

An independent review reapplied implementation `1f41f4eb` and candidate
`d4fa1b77` onto clean main `0eb22fa9` in its own worktree. No blocking source
semantic defect was found. Review commit `1f3e6ace` closes verification gaps:
each wrapper must lower to its specific architectural intrinsic with legacy
SSE2 tier-0 packed-pair metadata; distinct operand calls must each appear once
in source order; both header inclusion orders must coexist with shifts,
AES and PCLMUL while retaining optional-crypto feature diagnostics. Existing
negative checks cover every private builtin's arity, signedness, nominal lane
type, shape and target, including explicit ARM diagnostics.

Fresh review runs passed all five suites above in debug and release: 16 tests
per profile locally on Apple ARM64, and 18 per profile on LinuxHost, including
GCC 16.2.0 and Clang 23.1.1 native saturation and shift execution. Local x86
objects establish lowering/encoding, not native execution. The native widened
scalar oracle uses bounded `int` arithmetic and unsigned byte copies; signed
lane interpretation is explicit, avoiding narrow signed overflow or
out-of-range signed conversions. Its exhaustive byte pairs and finite word
edges/random cases exercise all eight wrappers and both signedness modes.
The existing emission-guard unit test additionally passed locally in both
profiles. All runs used the prescribed lock, cargo, offline/locked flags, two
jobs and disabled link caching; no toolchain or host configuration changed.

The frozen tested source is commit `1f3e6ace` (the final review documentation
is separate), archived on both hosts as `/tmp/keel-sat-review-source.tar`,
SHA-256 `5e6b1cfa7b0bec154af5194081bf85d166f403913f026aa1e3cdd3c5fedc1cc8`.
LinuxHost extracted it at `/tmp/keel-sat-review-79612096`. Fresh logs and lock
records are `/tmp/keel-sat-review-native-{debug,release}.{log,jsonl}`;
local logs/records use `/tmp/keel-sat-review-local-{debug,release}.{log,jsonl}`
and `/tmp/keel-sat-review-guard-{debug,release}.{log,jsonl}`.
LinuxHost artifacts are `/tmp/keel-sat-review-native-{debug,release}`, including
copies of the exact test executables embedding the compiler as
`frozen-test-compiler`. `/tmp/keel-sat-review-provenance.json` records source,
compiler/test-executable, actual rustc, GCC, Clang, decoder, kernel, harness,
object and linked-executable SHA-256 identities. The independent decoder is
`/tmp/keel-sat-review-audit.py`; listings accompany the six objects. It examined
8,076 instructions, finding all eight saturation mnemonics in each object and
only the baseline inventory listed above. This is finite encoding evidence,
not a general independent feature-decoder proof. Native execution used a
newer CPU, not a v1-only or CPUID-masked host. No timing claim is made. Main,
the original worktree and the separate numeric-conversion work were untouched.

## Bounded SSE2 numeric conversions

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

`keel-c --test x86_conversions` separates frontend/IR and strict-v1 object
checks from conditional native Linux x86 execution. The native harness compares
integer-only independent bit/flag oracles with separately compiled GCC/Clang
vendor-header wrappers. External calls across translation units, no LTO, and
volatile assembly MXCSR reads/writes ensure the observed conversion executes
between environment setup and observation. The candidate's unused-result forms
are checked against the same status oracle. Two identical i32-to-f32 operations
separated by an external rounding-mode change verify ordering and retention.

The finite corpus covers signed extrema, exact/inexact integers near and beyond
2^24, ties, negative zero, positive/negative subnormals, NaNs (quiet/signaling),
infinities, and signed-i32 boundaries/out-of-range values. Every RC/DAZ/FTZ
combination runs with clear, invalid-seeded, precision-seeded and all-seeded
status, plus 256 deterministic random vectors per operation/environment.
Results and full MXCSR are checked separately, with all exceptions masked and
entry MXCSR restored. Native tests also cover direct/indirect calls, callbacks,
live inputs/results, single evaluation, output/input aliases, unaligned pointers,
byte sentinels and both protected-page edges for exact 16-byte accesses.
Unmasked exception/trap delivery is **not tested or newly claimed** here.

On 2026-10-23, debug/release gates passed on Apple ARM64 (14 tests/profile,
frontend/IR/cross-object plus applicable existing native controls) and LinuxHost
Linux x86-64 (17 tests/profile, including native conversions with GCC 16.2.0
and Clang 23.1.1). Each conversion harness executed baseline/fast, trees/full,
and optimized/full with empty register pools. The same runs passed existing
`x86_intrinsics`, `x86_shifts`, `x86_aes` and `pclmul` regressions.

Independent `llvm-objdump` decoding found legacy `cvtdq2ps`, `cvttps2dq` and
`cvtps2dq` in all six debug/release candidate objects; full mnemonic inventories
contain only baseline integer/SSE/SSE2 instructions. Encoding inspection and
native execution are separate evidence. LinuxHost is a newer CPU, so this does not
establish execution on a CPUID-masked v1-only machine or exhaustive FP behavior.
No timing/performance claim is made.

Reproduce under the shared build lock (add `--release` for release):

```sh
python3 suite/fast_opt_locked.py /tmp/keel-c-conversions-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-c \
  --test x86_conversions --test x86_intrinsics --test x86_shifts \
  --test x86_aes --test pclmul -- --nocapture
```

LinuxHost source snapshot: `/tmp/keel-c-conversions-20261023`.
Logs/lock records: `/tmp/keel-conversions-native-debug-final.{log,jsonl}` and
`/tmp/keel-conversions-native-release.{log,jsonl}`. Candidate objects, vendor
sources, harnesses and executables: `/tmp/keel-c-conversions-3229213` (debug)
and `/tmp/keel-c-conversions-3230199` (release). Disassembly is beside each
object; hashes/inventories and decoder are `/tmp/keel-conversions-disassembly.json`
and `/tmp/keel-conversions-decode.py`. Local logs/lock records use
`/tmp/keel-conversions-local-debug-final.{log,jsonl}` and
`/tmp/keel-conversions-local-release.{log,jsonl}`. Builds/native execution held
the shared suite lock, used cargo's default cache and kept native-link caching off.

Independent review reapplied candidate `faf36ba0` in its own worktree and
rebased onto main `3138975a`, preserving all eight saturation and sixteen shift
wrappers. The frontend dispatch, builtin recognition, shared lowering arm and
maintained documentation conflicts retain both implementations. No blocking
production semantic defect was found. Review found that every negative builtin
case declared an unsupported eight-byte vector, allowing unrelated declaration
errors to hide bad-call acceptance. The tests now prove the declarations valid
and require the intended arity/type diagnostic. They also check the exact typed
intrinsic signatures/opcode metadata, pinned/barrier/trapping classification,
and text/typed/incremental-chunk object byte parity at strict v1 and an effective
v3 profile with AVX and optional crypto disabled.

The original live-value callback did not physically clobber SIMD registers.
Review now places an external call clearing all sixteen XMM registers between
conversion and capture; direct and indirect callbacks also clobber them before
returning. All three conversions are repeated across an external MXCSR change
that clears status, checking retention/order as well as resulting bits/status.
Each boundary input additionally runs in isolation across all four lanes, so
adjacent invalid or inexact inputs cannot hide spurious flags on valid INT32_MIN
or exact conversions. These are focused test corrections; production backend,
optimizer, ABI, FP policy, widths and target features are unchanged.

Fresh final combined runs passed `x86_conversions`, `x86_saturation`,
`x86_shifts`, `x86_intrinsics`, `x86_aes` and `pclmul` in debug and release:
17 tests per profile locally on Apple ARM64 and 20 per profile on LinuxHost. Local
x86 objects establish frontend/lowering/encoding, not x86 execution. LinuxHost GCC
16.2.0 and Clang 23.1.1 independently execute the vendor and integer-only
bit/status oracles in each selection/allocation configuration. The unchanged
production backend additionally passed `x86_legacy_raw_fp` and
`x86_raw_fp_conversions` on LinuxHost (six tests per profile), and the existing
emission-guard unit test passed locally in both profiles. All builds/tests used
the shared lock, prescribed cargo offline/locked/two-job commands and disabled link
caching. No timing conclusions were drawn.

The final tested source is `6b3ef4d9908dac1b936cff98910d0c2c9fd346c6`; this
review evidence documentation is a subsequent commit. Its frozen archive is
`/tmp/keel-conversion-review-source.tar` on both hosts, SHA-256
`2bba776ba3a73cf37e436c2e520aaedb63b30c368ef6637ef7944532628d5da8`.
LinuxHost extracted it at `/tmp/keel-conversion-review-final-97495733`.
Final logs/lock records are `/tmp/keel-conversion-review-final-{debug,release}.{log,jsonl}`
on LinuxHost and `/tmp/keel-conversion-review-final-local-{debug,release}.{log,jsonl}`
locally. Backend FP logs use `/tmp/keel-conversion-review-fp-{debug,release}.{log,jsonl}`
on LinuxHost; local guard logs use `/tmp/keel-conversion-review-guard-{debug,release}.{log,jsonl}`.
Native objects, harnesses, linked executables and copies of the exact Rust test
executables embedding the compiler are below
`/tmp/keel-conversion-review-final-{debug,release}/keel-c-conversions-*`.
`/tmp/keel-conversion-review-provenance.json` on LinuxHost records source,
compiler/test-executable, rustc, GCC, Clang, decoder, kernel, harness, object and
linked-executable identities. The independent audit is
`/tmp/keel-conversion-review-audit.py`; disassembly listings sit beside artifacts.
It decoded 2,206 instructions across six Keel objects, finding each of the three
legacy conversion mnemonics and checking their mandatory prefixes/opcode bytes.
The complete finite mnemonic inventories contain only baseline integer/SSE/SSE2
instructions. Decoding each linked GCC/Clang executable also confirms actual
vendor conversion instructions and all sixteen physical XMM clears in the
clobber helper. This is bounded encoding/native evidence, not an exhaustive
feature-decoder proof, v1-only CPU execution, unmasked-trap validation or a
performance claim.

FP64, scalar-lane, 256-bit/AVX2, unsigned and undefined-upper-lane conversion
wrappers remain unsupported. There is no general numeric vector-cast extension.

## Validation and reproduction

`crates/keel-c/tests/x86_intrinsics.rs` verifies all six wide shapes, C layout,
invalid controls and feature-disabled compilation, header inclusion at SSE-only
feature levels, and object emission of every defined inline function. A C
fixture and generated kernels test values, shuffles, signedness, dynamic lanes,
bit casts, single evaluation, direct/indirect calls and callbacks with ten wide
arguments (including aligned stack transport). Three selection/allocation/
optimization configurations emit real x86 ELF objects. Clang cross-compiles
independent oracle and harness objects on ARM hosts; that alone is not native
runtime validation. On native Linux x86 with AVX2, the test links and executes
with available GCC and Clang against their own vendor headers.

The native harness uses protected pages before and after each memory buffer,
plus byte sentinels, to test both 16- and 32-byte footprints, unaligned pointers,
and copies at both page boundaries. Export artifacts for an external x86 host
with `KEEL_C_X86_EXPORT_DIR=/tmp/keel-c-x86-<unique>`; all bulk artifacts stay
outside Git. A shared suite-lock invocation is:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-c-vector-build.jsonl \
  cargo test --locked -p keel-c --test x86_intrinsics --test gnu_types --test gnu_vector_abi
```

For an exported artifact directory on an AVX2-capable Linux x86 host, run under
that host's shared suite lock, with each available compiler:

```sh
clang -mavx2 -O1 oracle.c harness.c keel-baseline.o -o run-clang-baseline
./run-clang-baseline
gcc -mavx2 -O1 oracle.c harness.c keel-baseline.o -o run-gcc-baseline
./run-gcc-baseline
```

Repeat for `keel-trees-fast.o` and `keel-optimized.o`. Runtime results and compiler
versions from the implementation's validation are recorded below.

On 2026-10-23, the ARM64 macOS host passed the existing 16-byte Clang ABI
harness and emitted the new x86 objects. LinuxHost (native x86-64 Linux) linked and
executed the exported wide/intrinsic harness successfully with Clang 23.1.1
and GCC 16.2.0, for all three Keel configurations. This included all eight
supported quiet predicates for both floating widths, NaNs and signed zero,
wide globals/lanes, ten-argument calls and protected-page/sentinel memory checks.
Builds and native execution held `/tmp/keel-perf-20261019.lock`; Rust builds
used the shared default cargo cache, with native-link caching left disabled.

The strict-v3 userspace audit also runs this native GCC/Clang harness and checks
ordinary wide C multiply/add through the public `cc --cpu-level v3` text/object
paths, including disabled-AVX diagnostics and absence of implicit contraction.
See [the bounded v3 join](userspace-isa-status.md#strict-v3-userspace-contract-and-audit)
for exact feature, native and object-decoding evidence and remaining limits.
This adds validation, not additional header names or FP-environment guarantees.


## Bounded SSE2 saturation

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

`keel-c --test x86_saturation` verifies frontend checks and explicit strict-v1
text/typed/chunked object parity on every host. Native execution runs only on
Linux x86-64. Three configurations are baseline/fast, trees/fast and optimized
full/full with empty allocation pools. GCC and Clang compile independent system
vendor-header oracles alongside a widened scalar reference with explicit clamps.
The harness checks all 65,536 ordered byte pairs for every byte operation in
every lane, word boundary pairs, every word bit pattern paired with selected
edges, and 4,096 deterministic random vector pairs per operation. Direct and
indirect calls, inputs live across a callback, same-input and output/input
aliases, single evaluation, unaligned 16-byte copies, byte canaries and both
protected-page edges are covered. This is not exhaustive 16-bit pair testing.
AVX2 widths, narrowing packs and other saturation families remain deferred.

Reproduce with the existing toolchain and shared build lock:

```sh
KEEL_C_SAT_EXPORT_DIR=/tmp/keel-c-saturation-unique \
  python3 suite/fast_opt_locked.py /tmp/keel-c-saturation-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-c \
  --test x86_saturation --test x86_shifts --test x86_intrinsics \
  --test x86_aes --test pclmul
```

Add `--release` for release preflight and parity. Artifacts and runtime evidence
remain under `/tmp`; cross-object success alone is not native validation.

On 2026-10-23 the implementation was rebased from clean `0beb8f7d` onto
`0eb22fa9` after the independently reviewed shift subset landed. All sixteen
shift wrappers and their private bridge conventions were preserved. The five
C suites in the command above passed locally on Apple ARM64 (15 tests per
profile, cross-object checks) and natively on LinuxHost with Clang 23.1.1 and GCC
16.2.0 (17 tests per profile), in debug and release. Both native compilers ran
all three saturation configurations in each profile. The existing emission
feature-guard unit test also passed locally in debug/release, retaining optional
ISA rejection in both text and structured paths.

LinuxHost source is `/tmp/keel-c-saturation-20261023`; regression logs are
`/tmp/keel-c-saturation-{debug,release}.log`, with final saturation-only reruns
at `/tmp/keel-c-saturation-final-{debug,release}.log`. Objects, generated kernels,
GCC/Clang harnesses and executables are in
`/tmp/keel-c-saturation-native-{debug,release}`. The independent
`llvm-objdump` inventory in `/tmp/keel-c-saturation-disassembly.json` and decoder
`/tmp/keel-c-saturation-decode.py` examined six objects (8,076 instructions),
finding all eight legacy saturation opcodes in each. The remaining decoded
mnemonics were `push`, `pop`, `mov`, `movups`, `movaps`, `sub`, `add`, `lea`,
`call`, `ret` and `nop`; no AVX or optional ISA instruction appeared. This finite
inventory supplements strict-feature assembly and text/typed/chunked parity;
it is not an exhaustive independent feature decoder proof. The host supports
newer ISA levels, so execution does not establish operation on a v1-only or
CPUID-masked CPU. Independent review and parent integration remain separate.
No timing claim is made. All builds/native tests used the shared lock,
`--offline --locked -j2`, existing pins and shared default cargo cache with
`CARGO_CACHE_LINKS=0`.
