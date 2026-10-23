# AVX2 half-local dword shuffle selection

For equal resolved operands, generic `i32x8` and `f32x8` shuffles select
`vpshufd ymm, ymm, imm8` when both 128-bit halves use the same four-dword
control. Repeated indices are supported. Either concatenated-source spelling
of an index is accepted only when the operands resolve to the same value.
For example `[3,2,1,0,7,6,5,4]` selects immediate 27.

The selector stages the source with the existing wide helper, writes reserved
FS0, and publishes through the existing result helper. It requires AVX2.
V1/v2 continue rejecting 256-bit transport; AVX without AVX2 uses the existing
legal lane fallback. Cross-half masks, different high-half controls, distinct
sources and other element widths retain the unchanged general shuffle code.
No optimizer, feature policy, memory access, FP arithmetic or MXCSR operation
changes. Original unaligned loads and stores keep their exact widths.

Validation at base `45c19d5d248ca7a8c213646ccdebf7f8beb2b339`:

- The classifier checks all 256 controls, all 256 equal-source spellings per
  control for both supported types, distinct-source rejection, single-index
  boundary edits, incorrect length and an unsupported lane width.
- `x86_avx2_shuffle_selection` checks typed/text object equality, all 256 controls
  through codegen for each type, and all 256 YMM instruction encodings against
  Clang's independent x86 assembler. Feature negatives check both output paths
  at v1/v2 and legal compilation without AVX2. Different high-half controls and
  cross-half masks must retain `vpshufb`, without `vpshufd`.
- The exported 48-function C/ELF corpus checks six controls, two fallback masks,
  distinct/same SSA/resolved-alias inputs, live inputs, integer and FP payloads
  including signed zeros and signaling/quiet NaNs, offsets 0..31, output aliases
  to either input, canaries, both protected-page edges and unchanged MXCSR.
  Each input is exactly 32 bytes; output contains three 32-byte stores (result
  plus captured inputs). Controlled allocations additionally exercise result
  aliases to either source register, independent source/result spills and all
  spills. Captured inputs are stored before the shuffle in controlled cases so
  reusing their registers is legal.
- Local ARM64 cross checks passed 108 codegen library tests, four new integration
  tests, three existing `x86_avx256_generic` tests and one
  `x86_wide_conversions` test. Clippy passed with existing repository warnings;
  no new warning originated in the added selector or test. These cross checks
  do not execute x86 objects.
- Native LinuxHost Linux x86-64 execution passed 27 exported oracles: 12 normal/empty
  pool combinations across all three selectors and both allocators, plus 15
  controlled layouts across all three selectors. Final object/C SHA-256 values
  match the native report. This is finite semantic evidence on an AVX2-capable
  host, not execution on v1/v2-only hardware or exhaustive equivalence.

Static measurements use Full selection without optimization and the unchanged
48-function mixed positive/negative corpus, including all function overhead:

- Full allocator, normal pools: 3,144 -> 2,064 instructions;
  16,696 -> 9,784 text bytes; 936 -> 456 frame-memory instructions.
- Fast allocator, normal pools: 3,192 -> 2,112 instructions;
  16,698 -> 9,786 bytes; the same frame-memory counts.
- Full allocator, empty vector pools: 3,368 -> 2,288 instructions;
  18,746 -> 11,642 bytes; 1,304 -> 800 frame-memory instructions.
- Fast allocator, empty vector pools: 3,416 -> 2,336 instructions;
  18,748 -> 11,644 bytes; the same frame-memory counts.

Frame counts include mask-buffer and ABI traffic, not just allocator spills.
The 24 fallback functions have identical assembly at base and candidate.
The standalone half-reverse register-ABI function falls from 26 to 3 body
instructions, with eight mask-stack stores removed. Whole-function counts fall
from 36 to 13 instructions and 184 to 39 text bytes. The three body instructions
are source staging, `vpshufd` and result publication; frame overhead remains.
No runtime timing or runtime speedup is claimed.

Reproduce locally with `CARGO_CACHE_LINKS=0`, cargo's shared default cache and each
checkout's managed target directory:

```sh
KEEL_AVX2_SHUFFLE_EVIDENCE=/tmp/keel-avx2-shuffle-evidence \
python3 suite/fast_opt_locked.py \
  /tmp/keel-avx2-shuffle-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen \
  --lib --test x86_avx2_shuffle_selection \
  --test x86_avx256_generic --test x86_wide_conversions
```

Run the dedicated test with `-- --ignored --nocapture` for static measurements.
For the base, archive the revision above into a disposable directory and copy
only the new test file into it; run only the ignored measurement. Base evidence
is `/tmp/keel-avx2-shuffle-base-evidence`, candidate evidence is
`/tmp/keel-avx2-shuffle-evidence`. No historical binary is changed.

Copy exported root-level `.o`/`.c` pairs to LinuxHost and run the existing
`crates/keel-codegen/tests/x86_widen/run_native.py --dir EXPORT` under
`suite/fast_opt_locked.py`, without nesting locks. The native report is
`/tmp/keel-avx2-shuffle-evidence/native-results.json` on LinuxHost and locally.
Generated objects, source snapshots, captures and build logs remain under `/tmp`.

Independent review after rebasing onto `bfa86ed7` reproduced the dedicated
integration tests, exhaustive classifier test, existing wide SIMD/conversion
regression tests and ignored static measurement
under the shared investigation lock. Every exported function now asserts its
own fast/fallback selection and, for positives, the expected immediate across
all 27 allocation/selection combinations. Default temporary directories are
separate per test to prevent parallel test cleanup racing artifact writes.
The 27 rebased object/C pairs exactly match the previously native-tested hashes;
those hashes and passing statuses were also verified on LinuxHost. No native rerun
was required for byte-identical exports. The native report SHA-256 is
`aaec70ae00f7aa619b2dead78cc051eb41cbe83a36fd9a45fefc5e88b4891bf1`.
Review artifacts are in `/tmp/keel-avx2-review-evidence`; serialized commands
are in `/tmp/keel-avx2-review-unique.jsonl`. Whole-workspace formatting check
reports existing unrelated formatting differences; the changed test passes
its focused rustfmt check. The review found no selector correctness issue.
