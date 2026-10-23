# Generic x86 SIMD selection

The generic 128-bit shuffle selector uses existing SSE2 encodings for:

- Every one-source dword permutation, including resolved `[x,x]` inputs.
- Two-source dword shuffles with two low lanes from one source and two high
  lanes from the other, in either source order.
- All qword permutations; the existing two-source `shufpd` window is retained.
- Byte/word permutations that preserve aligned groups of four bytes.

Classification is shared by vector legalization and final emission. `pshufd`
reads its source before defining the destination; destructive two-source shuffles
stage both inputs in reserved scratch before publishing the result. Existing
spill transport and typed assembler paths are reused. These recipes are legal
at explicit v1, v2 and v3; no feature-policy changes are involved.

Only register shuffles change. Original loads/stores keep their sizes, ordering
and alignment. The recipes move bits, including NaN payloads and signed zeros,
and do not modify MXCSR. No FP arithmetic, FMA contraction or min/max rewrite
is introduced.

## Finite validation and code generation

Base revision: `9ffab6bcdb610b74f9ac0fd92bfd6b1a4dd848a0`. The unchanged
128-function corpus in `x86_generic_shuffle::fixture` includes all qword index
pairs, representative dword/byte-group/word-group masks, repeated and reversed
lanes, same inputs, and both inputs remaining live after the shuffle.
Measurements use Full selection without an IR optimization pass and include
prologues, argument staging, all explicit memory operations and function padding.
All three explicit CPU levels produced the same counts:

- Full allocation, normal pools: 4,268 -> 2,528 instructions; 16,063 -> 8,700
  `.text` bytes; 892 -> 256 frame-memory instructions, including 372 -> 0
  vector frame-memory instructions.
- Fast allocation, normal pools: 4,396 -> 2,656 instructions; 16,465 -> 8,702
  bytes; the same frame-memory counts as Full allocation.
- Full allocation, empty vector pools: 5,528 -> 3,360 instructions;
  23,708 -> 14,323 bytes; 2,164 -> 1,184 frame-memory instructions, including
  1,396 -> 928 vector frame-memory instructions.
- Fast allocation, empty vector pools: 5,656 -> 3,488 instructions;
  24,031 -> 14,326 bytes; the same frame-memory counts as Full allocation.

Frame-memory counts describe emitted traffic, including ABI staging and the old
scalarization buffers; they are not allocator spill-slot counts. The empty pools
force vector spills. No runtime performance measurement or speedup claim is made.

On MacHost (ARM64), focused tests verify typed/text object equality for every
selector/allocator/level/pool combination, and all 256 `pshufd`/`shufps`
immediates against Clang's independent x86 assembler. A unit regression models
all 4,096 dword masks and all 16 qword pairs, including same-input selection and
rejected non-group-preserving masks. Existing window tests retain controlled
destination aliases, independent spilled inputs/results and boundary edits.

On LinuxHost (Linux x86-64), 36 exported generic-shuffle oracles and 20 existing
window oracles passed with Clang. The generic oracle checks all 256 byte
patterns plus FP edge payloads, offsets 0..15, output aliasing either input,
both inputs remaining live, accessible-page boundaries and unchanged MXCSR.
This is finite semantic coverage, not exhaustive equivalence or a new ISA
completion verdict.

Reproduce with `cargo test --offline --locked -j2 -p keel-codegen` and the tests
`x86_generic_shuffle` and `vector_shuffle_window`, under
`suite/fast_opt_locked.py`. Export with `KEEL_SIMD_EVIDENCE` and
`KEEL_SHUFFLE_EVIDENCE`; run the ignored `measure_generic_shuffle_codegen` test
with `--ignored --nocapture` at each revision. The existing
`tests/x86_widen/run_native.py --dir EXPORT` runner executes generic ELF+C
oracles on x86 Linux; `tests/vector_shuffle/run_native.py` runs window exports.
Builds use cargo's default shared cache and managed per-checkout targets, with
`CARGO_CACHE_LINKS=0`. Local bulk evidence lives outside Git at
`/tmp/keel-simd-70bb145d`, with native result JSON also retained on LinuxHost at the
same path.

## Unsigned byte comparisons

The unsigned `i8x16` compare recipe now builds its sign-bit bias with
`pcmpeqd; psllw 7; packsswb`. Each all-ones word becomes `0xff80` (-128),
which packs exactly into one `0x80` byte. Packing the same register as both
inputs fills all sixteen lanes. This replaces six bias-building instructions
with three and uses the same reserved scratch register, with no new memory
access or feature requirement. Ordering, operand swapping, canonical masks and
non-strict comparison inversion retain their existing code paths.

Against shuffle commit `151e23a3`, the unchanged four-predicate corpus in
`x86_unsigned_byte_compare::fixture` measures the following at every CPU level:

- Full allocation, normal pools: 120 -> 108 instructions, 469 -> 404 text
  bytes, unchanged 8 frame-memory instructions (0 vector frame accesses).
- Fast allocation, normal pools: 124 -> 112 instructions, 471 -> 406 bytes,
  the same frame-memory counts.
- Full allocation, empty vector pools: 144 -> 132 instructions, 601 -> 536
  bytes, unchanged 40 frame-memory instructions (32 vector frame accesses).
- Fast allocation, empty vector pools: 148 -> 136 instructions, 636 -> 571
  bytes, the same frame-memory counts.

The byte test checks strict v1/v2/v3 encoding through typed and textual object
paths and compares the bias recipe bytes with Clang. On LinuxHost, all 36 exported
byte oracles passed. Each checks all 65,536 operand pairs at every lane for all
four unsigned ordering predicates, with unaligned/aliased outputs and live
inputs. All 42 existing exported comparison/mask oracles also passed on LinuxHost; they cover
destination aliases, spilled inputs/results, all integer widths, and select
mask normalization. No runtime speedup is claimed. Export with
`KEEL_BYTE_COMPARE_EVIDENCE`; the ignored
`measure_unsigned_byte_compare_codegen` test records the unchanged corpus.

## Reproducing revision comparisons

The two measurement tests were introduced by this branch, so they are absent
from the historical base. Use a disposable archive of
`9ffab6bcdb610b74f9ac0fd92bfd6b1a4dd848a0` and copy only
`crates/keel-codegen/tests/x86_generic_shuffle.rs` and
`crates/keel-codegen/tests/x86_unsigned_byte_compare.rs` from the candidate into
that archive. Leave the base compiler sources unchanged. The byte corpus does
not contain shuffles, so this common base also reproduces its pre-bias-change
counts. The original intermediate comparison revision is `151e23a3`.

In each checkout or archive, run the following with a distinct evidence path
and log path under `/tmp`:

```sh
\
KEEL_SIMD_EVIDENCE=/tmp/keel-review-candidate/shuffle \
KEEL_BYTE_COMPARE_EVIDENCE=/tmp/keel-review-candidate/byte \
python3 suite/fast_opt_locked.py /tmp/keel-review-candidate.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen \
  --test x86_generic_shuffle --test x86_unsigned_byte_compare \
  -- --ignored --nocapture
```

Run only the ignored measurements at the base: the ordinary encoding tests
assert that the new recipes are selected. Keep each checkout's cargo-managed
target directory and the default shared cache; do not share `CARGO_TARGET_DIR`.
The printed instruction/byte/frame-access counts can be compared directly.
The saved assembly can additionally be inspected for vector-width frame
accesses; those are a subset of the printed frame-access total.

Local ARM64 success for these two test targets checks cross-generated x86
objects, typed/text equality, Clang encodings and C syntax. It does not execute
the exported x86 C oracles. Run those separately on Linux x86-64 with the
native runners above, under the same serialization wrapper, and retain their
`native-results.json` reports.

## Remaining limitations

Arbitrary cross-source dword masks and byte/word masks that split dword groups
still use the existing lane legalizer. D (64-bit) vectors and 256-bit shuffle
selection retain their existing paths. Single-source identity shuffles can
still emit `pshufd` when an earlier IR optimization has not removed them.
Two-source staging is conservative and can add copies. No generalized blend,
FP comparison, narrowing, widening-load fusion or addressing policy is added by
this change. Those need separate profitability and semantic checks, especially
for noncanonical select masks and exact-width faulting loads.
