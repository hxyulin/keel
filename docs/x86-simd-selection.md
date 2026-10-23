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
FP comparison, narrowing or addressing policy is added by the shuffle work.
General widening-load fusion still needs separate profitability and semantic
checks; the bounded signed-dword m64 pilot below covers one closed chain.
Noncanonical select masks and other faulting-load shapes remain outside it.

## Exact signed dword widening load pilot

Starting from main `45c19d5d248ca7a8c213646ccdebf7f8beb2b339`, the existing
exact-low-eight-byte load cover additionally recognizes this adjacent chain:

```text
v2 = load.i64 align 1 v0
v3 = vsplat.i64x2 v2
v4 = bitcast.i32x4 v3
v5 = swiden_low.i64x2 v4
```

When the effective feature set permits SSE4.1, it emits `pmovsxdq xmm, m64`
at the widen, covering the first three instructions. Each intermediate must
have exactly one use. All four instructions must retain their own SSA result,
share the same complete source location, and have no existing selection,
condition or allocation constraint. Volatile loads, nonadjacent operations,
shared intermediates, other shapes and unsigned/high widening keep existing
lowering. Explicit v1 and `-sse41` overrides retain the SSE2 fallback; adding
SSE4.1 to v1 permits the cover. This changes no optimizer widths or feature
policy, general conversion lowering, positive32 DAG, or AVX2 recipe.

The memory operand remains exactly the original eight-byte access, at the same
position relative to effects and other accesses. Only adjacent pure vector
operations precede the replacement. Selected address leaves, register
allocation, reserved address scratch, destination XMM allocation and spill
publication reuse the existing low-load path. The instruction changes neither
integer flags nor MXCSR; no ABI or FP arithmetic changes are involved.

`x86_signed_widen_load` checks text/typed/chunked object parity and each typed
instruction's required features for Baseline/Trees/Full
selection, Fast/Full allocation, normal/tiny/empty pools and six effective
profiles: v1/v2/v3, v1+sse41, v2-sse41 and v3-sse41. It checks negative forms,
shared operands, use/definition constraints at every chain node, source line
and column boundaries, and independent Clang m64 encodings with extended
registers and indexed/displaced addresses. The existing `x86_packed_low_load`
count accepts either exact-width load encoding. Both test targets, plus
`vector_widen_x86` and `x86_packed_positive`, originally passed 16 tests in debug
and release locally on ARM64. Review adds optimized/unused-result coverage and
selector unit checks for conditions, covered nodes, resolved aliases and load
operand transfer. After rebasing onto `bfa86ed7`, review passed 17 integration
tests plus two selector unit tests in both debug and release. The legacy test
also requires MOVQ separately for each of its five non-pilot shapes. No shared
backend infrastructure changed.

Export native bundles with `KEEL_SIGNED_LOAD_EVIDENCE=/tmp/EXPORT` using:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-pmovsxdq-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen --test x86_signed_widen_load
```

Run the exported `run_native.py --dir /tmp/EXPORT` on Linux x86-64 under the
same lock wrapper. On LinuxHost (Ryzen 9 9950X3D), all 108 exported bundles passed:
direct, offset and indexed addressing, live vector/address sources, destination
memory aliasing the source, signed extrema and deterministic xorshift samples,
all 16 alignment offsets, both protected-page edges, faulting accesses crossing
either edge before any output write, canaries and MXCSR preservation. These
are finite tests on a host supporting SSE4.1, not a v1-only execution proof.
Review reran 108 freshly exported signed bundles plus the 13 retained generic
low-load bundles on LinuxHost; all passed. Review artifacts are under
`/tmp/keel-pmov-review-{export,generic}` on LinuxHost, with lock metadata at
`/tmp/keel-pmov-review-unique.jsonl` on each host.

### Maintained widen32 measurement

LinuxHost compared frozen release baseline and candidate compilers using the exact
unchanged `suite/runtime-kernels/kernels.c`, `driver.c` and Python oracle.
Both used `cc -O --opt-tier full --cpu-level v1|v2|v3 --emit-object`, with the
same Clang-O2 driver object and no LTO. The existing runtime runner was copied
under `/tmp` and bounded to six baseline/candidate Full configurations, cases
`widen32`, `positive32`, `adjacent`, and lengths
`0,1,7,8,17,257,4096,65536,1048576`; no maintained harness or kernel was edited.
All 162 correctness checks preceded timing. CPU affinity was 2, seed 20261021,
one warmup compile plus one measured compile, seven shuffled runtime rounds,
`target_elements=4000000`, and one untimed call per sample. The public runner
owned the existing lock; it was not nested under another wrapper.

The v2/v3 `rk_widen32` body fell from 106 instructions / 378 bytes to
78 instructions / 256 bytes. The hot loop per eight inputs fell from
43 instructions / 180 bytes to 15 instructions / 61 bytes: four exact m64
`pmovsxdq` loads replace the four complete chains, removing all sixteen
`movaps` copies in that loop. V1 function bytes are identical. Independent
object disassembly and symbol-byte hashes found only `rk_widen32` changed
at v2/v3; all other eleven maintained kernel functions were byte-identical.

Median nanoseconds per call (baseline -> candidate), seven samples per cell:

| Length | v1 | v2 | v3 |
| --- | --- | --- | --- |
| 257 | 38.97 -> 39.11 | 39.15 -> 13.40 | 38.91 -> 13.40 |
| 4096 | 624.99 -> 622.63 | 622.24 -> 221.14 | 622.55 -> 221.04 |
| 65536 | 10343.84 -> 10326.95 | 10386.62 -> 3404.84 | 10348.92 -> 3448.92 |
| 1048576 | 158800.00 -> 158816.67 | 158756.67 -> 55738.00 | 159533.67 -> 57761.33 |

For those lengths the measured v2/v3 median ratio is 2.76–3.05x. V1 and the
byte-identical positive32/adjacent controls stay within 0.9% in median runtime.
This is a bounded warm repeated-input result on one host; short calls include
call/checksum overhead, and physical code pages, layout, boost and load remain
uncontrolled. It is not an application-wide or call-static speedup claim.

LinuxHost artifacts remain at `/tmp/keel-pmovsxdq-{base,candidate}` (source/builds),
`/tmp/keel-pmovsxdq-oracles-final` (108 ELF+C bundles and `native-results.json`), and
`/tmp/keel-pmovsxdq-measure` (`results.json`, commands, correctness, raw rounds,
objects, assembly, binaries, `static.json` and disassembly). The bounded runner
is `candidate/suite/runtime-kernels/pmovsxdq_pilot_run.py` under that `/tmp`
prefix. Its invocation used `--rounds 7 --compile-rounds 1 --target-elements
4000000 --cpu 2` plus the frozen compilers, `/usr/bin/clang`, label and a fresh
output directory. Compiler SHA-256 values are
`c128c9db50b2a716017cc63a369da3b8186ade854cf95b6b3b21acf2814d9143`
(baseline) and
`95bb7d02ca725e8da528584d656b3f95b134c8dd4aba3b7121f7721c378d8d4d`
(candidate). Builds used cargo's shared default cache, per-checkout targets,
`CARGO_CACHE_LINKS=0`, `--offline --locked -j2` and the shared lock wrapper.

To recreate the bounded runner without relying on retained `/tmp` artifacts,
export baseline `45c19d5d` and the candidate commit into separate clean source
directories under `/tmp` (for example, with `git archive REF | tar -x -C DIR`).
Build each with the following command from its own directory, preserving any
project toolchain/configuration and using its own cargo-managed target directory:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-pilot-build.jsonl \
  cargo build --release --offline --locked -j2 -p keel
```

Set `KEEL_PILOT_BASE` and `KEEL_PILOT_CANDIDATE` to those absolute source paths.
The following recreates the measured runner from the maintained runner; only
the case/length lists and compiler configurations differ. All other correctness,
shuffle, locking and timing code is retained:

```sh
python3 - "$KEEL_PILOT_CANDIDATE" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])
runner = root / 'suite/runtime-kernels/run.py'
text = runner.read_text()
anchor = 'from oracle import CASES, LENGTHS, MASK, inputs, reference\n'
assert text.count(anchor) == 1
text = text.replace(anchor, anchor +
    'CASES = ["widen32", "positive32", "adjacent"]\n' +
    'LENGTHS = [0, 1, 7, 8, 17, 257, 4096, 65536, 1048576]\n')
start = text.index('def compiler_configurations(')
end = text.index('\n\ndef canonical_runtime(', start)
text = text[:start] + '''def compiler_configurations(include_gcc=False, include_baseline=False, full_passes=None, baseline_passes=None):
    assert include_baseline and not include_gcc and full_passes is None and baseline_passes is None
    return {f'{version}-{level}': (family, ['cc', '-O', '--opt-tier', 'full', '--cpu-level', level, '--emit-object'])
            for version,family in [('candidate','keel'),('baseline','baseline-keel')]
            for level in ['v1','v2','v3']}
''' + text[end:]
runner.with_name('pmovsxdq_pilot_run.py').write_text(text)
PY
python3 "$KEEL_PILOT_CANDIDATE/suite/runtime-kernels/pmovsxdq_pilot_run.py" \
  --keel "$KEEL_PILOT_CANDIDATE/target/release/keel" \
  --baseline-keel "$KEEL_PILOT_BASE/target/release/keel" \
  --clang /usr/bin/clang --label signed-widen-review \
  --output /tmp/keel-pilot-fresh-results --cpu 2 \
  --rounds 7 --compile-rounds 1 --target-elements 4000000
```

The output directory must be new. This entry point takes the shared lock itself.
The archived campaign has no Git metadata (`checkout_commit` is null); binary
and input hashes identify it. Review compared its candidate build inputs to
`b62af511`: production source differences were whitespace and a comment only.
The independent tests added during review do not establish a new timing result
for the rebased tree.
