# AVX2 half-local dword shuffle selection

For equal resolved operands, generic `i32x8` and `f32x8` shuffles select

```text
vpshufd ymm, ymm, imm8
```

when both 128-bit halves use the same four-dword control. Repeated indices are
supported. Either concatenated-source spelling of an index is accepted, but
only when both operands resolve to the same value. For example
`[3,2,1,0,7,6,5,4]` selects immediate 27.

## Behavior

- The selector stages the source with the existing wide helper, writes the
  reserved FS0 register, and publishes through the existing result helper.
- Requires AVX2. v1/v2 continue rejecting 256-bit transport; AVX without AVX2
  uses the existing legal lane fallback.
- Cross-half masks, different high-half controls, distinct sources and other
  element widths keep the general shuffle code (`vpshufb`).
- No optimizer, feature policy, memory access, FP arithmetic or MXCSR behavior
  changes. Unaligned loads and stores keep their exact widths.

## Tests

- The classifier unit test checks all 256 controls, all 256 equal-source
  spellings per control for both types, distinct-source rejection, single-index
  boundary edits, incorrect length and an unsupported lane width.
- `x86_avx2_shuffle_selection` checks typed/text object equality, all 256
  controls through codegen for each type, and all 256 YMM encodings against
  Clang's x86 assembler. Feature negatives cover both output paths at v1/v2 and
  legal compilation without AVX2. Different high-half controls and cross-half
  masks must keep `vpshufb` and never emit `vpshufd`. Each exported function
  asserts its own fast/fallback selection and, for positives, the expected
  immediate.
- The exported 48-function C/ELF corpus checks six controls, two fallback
  masks, distinct/same/resolved-alias inputs, live inputs, integer and FP
  payloads (including signed zeros and signaling/quiet NaNs), offsets 0..31,
  output aliases to either input, canaries, both protected-page edges and
  unchanged MXCSR. Controlled allocations also exercise result aliases to
  either source register, independent source/result spills and all spills.

```sh
cargo test -p keel-codegen --lib --test x86_avx2_shuffle_selection \
  --test x86_avx256_generic --test x86_wide_conversions
```

Set `KEEL_AVX2_SHUFFLE_EVIDENCE=<dir>` to export the corpus, then run
`crates/keel-codegen/tests/x86_widen/run_native.py --dir <dir>` on a Linux
x86-64 host. All 27 exported oracles (12 pool/selector/allocator combinations
plus 15 controlled layouts) pass on x86-64 Linux. This is finite evidence on
AVX2 hardware, not execution on v1/v2-only CPUs or exhaustive equivalence.

## Static measurements

Run the dedicated test with `-- --ignored --nocapture` to reproduce. Numbers
cover the unoptimized 48-function mixed positive/negative corpus with Full
selection, including all function overhead:

| Configuration | Instructions | `.text` bytes | Frame-memory instructions |
| --- | ---: | ---: | ---: |
| Full allocator, normal pools | 3,144 -> 2,064 | 16,696 -> 9,784 | 936 -> 456 |
| Fast allocator, normal pools | 3,192 -> 2,112 | 16,698 -> 9,786 | 936 -> 456 |
| Full allocator, empty vector pools | 3,368 -> 2,288 | 18,746 -> 11,642 | 1,304 -> 800 |
| Fast allocator, empty vector pools | 3,416 -> 2,336 | 18,748 -> 11,644 | 1,304 -> 800 |

Frame counts include mask-buffer and ABI traffic, not only allocator spills.
The 24 fallback functions are unchanged. A standalone half-reverse register-ABI
function drops from 26 to 3 body instructions (source staging, `vpshufd`,
result publication), removing eight mask-stack stores; whole-function size
falls from 36 to 13 instructions and 184 to 39 bytes. No runtime speedup is
claimed.
