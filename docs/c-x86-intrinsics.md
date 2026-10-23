# Bounded C SSE/AVX intrinsic subset

Keel owns small `xmmintrin.h`, `emmintrin.h`, `smmintrin.h`, `avxintrin.h`,
`avx2intrin.h` and `immintrin.h` headers. They are embedded in the compiler;
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
  half, and numeric lane conversions, are not included.
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

Header functions use C vectors, scalar lane access, and fixed-size
`__builtin_memcpy`; shuffles and AVX comparisons use the checked builtins below.
All lower through the existing typed IR, optimization, ABI, feature gating and
instruction selection. No intrinsic-specific machine-code path is introduced.

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
saturation, horizontal operations, general shifts, conversions, FMA, AVX-512,
full vendor header coverage and strict FP-environment compatibility are outside
this header contract even where the backend already implements such operations.
The existing C restrictions on vector varargs, unions and by-value x86 aggregates
remain in force; see [GNU vector mode](gnu-vector-mode.md).

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
