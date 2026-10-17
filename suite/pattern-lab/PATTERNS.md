# Pattern fixtures

These are small native microexperiments, not production compiler rules or a
whole-application cost simulator. All fixtures use the common ABI:

```c
uint64_t pattern_kernel(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask);
uint64_t pattern_oracle(uint64_t iterations, uint64_t seed,
                        const uint64_t *data, uint64_t mask);
```

Manifest paths are relative to this directory. Each variant defines the same
entry symbol and must be compiled and linked separately with the driver and
oracle objects, without LTO. The timed kernel is hand-written assembly;
closed-form C oracles are outside the timing region. The fixtures target
AArch64 baseline integer ISA and x86-64 baseline integer ISA, with no AVX.
Preprocessed `.S` files select Darwin's underscore symbol convention or ELF
`.type`/`.size` and nonexecutable-stack metadata. x86 uses the System V/Darwin
register ABI, not Windows x64. All kernels are leaf functions using only
caller-saved registers, no stack storage, and finite CFI regions. Compiling for
the wrong architecture must report unsupported; an absent model, PMU, or
native validation run is not a successful experiment.

All arithmetic below is modulo 2^64. Every kernel checks zero iterations before
its loop and returns `seed` unchanged for zero. A nonzero unsigned iteration
counter decrements to zero; no signed counter or increment-to-overflow test is
used. Very large counts are semantically defined but should be bounded by the
harness's execution policy. The manifests exercise seeds `0`, `1`, and exact
integer `18446744073709551615`, at counts `0,1,2,17,257`. JSON readers must retain
these integer values without a floating-point round trip.

## Arithmetic patterns

`arm-dependent-add` and `x86-dependent-add` are **latency** experiments: one
accumulator starts at `seed` and advances by two per loop iteration. Baseline
executes two dependent `add 1` operations; candidate executes one `add 2`.
Return value is `seed + 2*iterations`. Work is two **unit increments** per
iteration, rather than two executed instructions in both variants. The
assembly retains the actual loop; a compiler cannot replace its timed body
with the oracle formula. Loop control and function entry/exit overhead remain
part of the measured kernel.

`arm-independent-add` and `x86-independent-add` are **throughput** experiments:
four accumulators start at `seed+j`, for `j=0,1,2,3`. Each advances by two per
iteration. Baseline has two rounds of four `add 1` instructions; candidate has
one round of four `add 2` instructions. Each accumulator is independent of the
others inside the body. Work is eight unit increments per iteration. At exit,
return `sum(accumulators) - 3*seed - 6`, giving `seed + 8*iterations`, including
wraparound. This checksum includes every accumulator, but it is a reduction,
not a collision-free fingerprint of the four states. The mathematical C oracle
uses the closed form; the assembly calculates and combines the four actual
states. Initialization, final combination, and loop control are timed overhead.
Four-way independence does not imply saturation of a particular processor.

`arm-control` and `x86-control` have byte-identical baseline/candidate source.
Each iteration adds one to the seed-dependent state. These diagnose the
variability and overhead of the measurement procedure. They neither establish
an empty-loop cost nor justify subtracting timings from other patterns.

Arithmetic/control patterns do not access `data` and ignore `mask`; their
one-word data allocation is only a uniform harness input.

## ARM byte-index address pattern

`arm-address-load` is a **memory** experiment with a load-to-next-index
dependency. For each iteration:

```text
index = ((state >> 16) & 255) & mask
state = data[2*index]
```

The first state is `seed`; the final state is returned. All variants execute
`ubfx w4,w1,#16,#8`, mask with `w3`, then form the address and perform the same
`ldr x1,[x4]`. The loaded value feeds the next iteration's extraction. This
keeps actual valid-storage loads and data-dependent addresses in the timed
kernel. It is not a parallel-load bandwidth experiment, and it does not model
a full application's cache behavior.

The manifest uses `mask=255` and **512 uint64 words (4096 bytes)**. Index 255
loads element 510: its eight-byte access covers byte offsets 4080..4087. Thus
511 words is the exact minimum allocation; the fixture allocates 512. The
ordinary generic predicate `mask < data_words` alone does not establish this
stride-specific bound. The fixture tests check the full address range. The
oracle reads the same passed array, never a hardcoded replacement. Data
initialization and its deterministic generator belong to `driver.c` and the
harness documentation; no fixture assumes particular contents or derives the
expected answer from the generator. With zero iterations neither implementation
nor oracle dereferences the data pointer. Calls with nonzero iterations require
the documented allocation.

The variants differ only in address formation:

* `original`: `sxtw x4,w4; lsl x4,x4,#4; add x4,x2,x4`.
* `compact`: `add x4,x2,w4,uxtw #4`.
* `paddedfold`: `nop; nop; add x4,x2,w4,uxtw #4`.

The extraction proves the value is in `[0,255]`, so SXTW and UXTW have the same
integer result. Both address expressions add `16*index` to the data base.
Arbitrary unmasked W values do not satisfy the proof: `0xffffffff` sign-extends
to -1 but zero-extends to 4294967295. This fixture also masks after extraction;
any caller mask still leaves a bounded byte index. No intermediate result is
used outside the replaced sequence. All variants have the same loop control,
live inputs/output, scratch register, load, and leaf ABI. The padded version
preserves instruction slots, including load/branch offsets, but inserts frontend
work; it is not a pure dependency isolator.

The source-level expected AArch64 body sizes are original 24 bytes, compact 16,
and paddedfold 24. Including guard, counter/branch, return move, and return, the
expected function sizes are 44,36,44 bytes, excluding interfunction alignment
and metadata. These are predictions from fixed-width instruction counts;
assembler/disassembly evidence must confirm them before measurement. x86
encodings have variable lengths and no unassembled byte-size claim is made.

This construction follows the masked-byte proof reviewed in the E3 manual ARM
address experiment (2026-10-17). It is a new isolated fixture, not a replay of
that application's layout, register pressure, dispatch, or reported timing.
Null results and regressions are valid outcomes. No always-faster claim follows
from removing instructions.

## MCA extracts and verification limits

Each `mca/*.s` file contains exactly the straight-line body between `L_loop`
and the counter decrement in the corresponding `.S` source. Tests enforce
literal instruction correspondence for every manifest variant. Extracts omit
entry, initialization, counter, branch, and final reduction. The address extracts
retain extraction, mask, full address formation, NOPs when present, and load.
They do not simulate branch prediction, loop execution, caches, or a complete
function. Any LLVM CPU model must be explicitly selected and identified; model
acceptance does not establish fidelity to an Apple processor. A missing model
or tool must be reported unsupported rather than producing an invented cost.
MCA metrics are model outputs, not native core-cycle measurements or additive
per-instruction costs.

Run source-only checks with no generated bytecode:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s suite/pattern-lab/tests -p test_patterns.py -v
```

These tests check exact uint64 manifests, contained existing source paths,
actual add-body affine increments, modular checksum identity, exhaustive byte
index bounds, fold/padding correspondence, source guards/register usage, and
MCA correspondence. Their restricted source checks do not certify assembled
opcodes, object format, unwind data, linked ABI behavior, or machine execution.
Independent review and harness native checks are required before any benchmark.
Native reports must retain source/object/binary identities and unsupported
architecture/tool statuses; source test success alone is not native correctness.

## Additional coverage

See [COVERAGE.md](COVERAGE.md) for the six shared recurrences behind the twelve
`arm-more-*` / `x86-more-*` fixtures, their exact seeds, semantic proofs and
measurement limits.
