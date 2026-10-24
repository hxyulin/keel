# Additional pattern coverage

The `arm-more-*` and `x86-more-*` fixtures add the same six recurrences on
AArch64 and x86-64, using baseline integer instructions and the unchanged
four-argument ABI. Each has exactly `baseline` and `candidate` variants. These
are experiments with possible null results or regressions; fewer instructions
establishes no performance win, whole-application benefit, or compiler rule.

All state and intermediates in the oracles are `uint64_t`, wrapping modulo
2^64. Every recurrence starts with `state=seed`, performs the specified work
once per iteration, and returns the actual final state. Zero iterations returns
seed without accessing data. Constant setup is outside the assembly loop;
entry, setup, loop control and return still contribute to a native timer.
Independent ordinary C loops specify semantics without inline assembly or the
candidate's division magic. Each shared oracle serves both architectures.

The exact decimal integer seeds are `0,1,9,10,11,127,128,255,256,
9223372036854775807,9223372036854775808,18446744073709551615`.
Correctness counts are `0,1,2,17,257`; all fixtures report one work unit per
iteration. JSON readers must preserve integers exactly, including uint64 max.

## Recurrences and operation pairs

`madd` is a latency chain: `state=state*3+7`. ARM compares multiply then add
with integer multiply-add; x86 compares multiply then add with address-style
LEA arithmetic. Work is one `state_update`. Neither variant loads data; the
one-word allocation and zero mask are unused. Modular multiply-add equivalence
holds for every seed, including overflow. This chain does not measure independent
multiply throughput or floating-point fused arithmetic.

`rotate` is a latency chain: `state=((state<<51)|(state>>13)) XOR
0x9e3779b97f4a7c15`. Both nonzero shift counts are below 64. Baseline uses two
shifts and OR; candidate uses native rotate; both XOR the same preloaded salt.
Work is one `state_update`; data and mask are unused. Rotation's bit permutation
preserves all 64 input bits before XOR. No variable-shift or vector behavior is
covered.

`udiv10` is a latency chain: `state=state/10+0x9e3779b97f4a7c15`. Baseline
uses unsigned hardware division by a preloaded ten; candidate takes the high
64 bits of unsigned multiplication by `0xcccccccccccccccd`, then shifts right
three. Both add the same salt with uint64 wrap. Work is one `state_update`;
data and mask are unused. Signed division and variable divisors are outside
coverage. On x86 the divide's high numerator half must be zero each iteration;
the data pointer is unused by this family, so caller-saved RDX is scratch.

The division proof covers the entire uint64 domain. Let `B=2^67`,
`M=0xcccccccccccccccd`, so `10*M=B+2`. For `n=10*q+r`, `0<=r<=9`,
`n*M=q*B+2*q+r*M`. For `0<=q<=floor((2^64-1)/10)`, the remainder
`2*q+r*M` is nonnegative and at most `2*floor((2^64-1)/10)+9*M < B`.
Thus `floor(n*M/B)=q=floor(n/10)`. Taking product high-half then shifting
three computes that quotient exactly. The source tests check this exact
inequality and adversarial quotient/remainder boundaries near powers of two
and uint64 max; samples supplement the proof.

`branch-select` is a control experiment: `index=(state>>8)&255`,
`bit=data[index]&1`, then `state+=bit?3:5`. Baseline uses a conditional branch;
candidate uses integer conditional selection (ARM CSEL or x86 CMOV). Work is
one `conditional_update`. Both perform the same ordinary uint64 load and
state-dependent address formation. The 256-word allocation and mask 255 bound
every eight-byte load to offsets 0..2047. The current driver's fixed data
generator is used without new distributions. Branch outcome predictability is
unknown, can depend on seed and recurrence, and is not asserted random or
50/50. Loop branches remain in both variants. MCA is intentionally omitted:
a straight-line body cannot represent the data-dependent mixture of paths,
branch prediction or misprediction costs.

`sext8` is a latency chain: `byte=state&255`, `extended=byte<128?byte:byte-256`
in uint64 arithmetic, then `state=extended+0x9e3779b97f4a7c15`. Baseline shifts
left 56 then arithmetic right 56; candidate sign-extends the low byte. Work is
one `state_update`; data and mask are unused. The C oracle uses unsigned
subtraction, avoiding implementation-defined signed casts and signed overflow.
The proof exhausts all 256 bytes. This is integer low-byte extension, with no
claims about wider extensions or language-level signed-overflow transforms.

`narrow-load` is a memory experiment: `index=(state>>8)&255`,
`state=data[index]&255`. Baseline loads a uint64 word then masks; candidate
zero-extends a byte at the same byte address. Work is one `low_byte_load`.
The 256-word allocation is 2048 bytes. For every index 0..255, the baseline
access ends at `8*index+7<=2047`, while the byte access starts at `8*index`.
Little-endian storage makes that first byte equal to the word's low eight bits.
The loaded state feeds the next index; after the first load it is a byte, so
subsequent indices are zero with mask 255. This deliberately limited recurrence
therefore measures a narrow, highly concentrated dependency chain, not broad
random-access bandwidth. The transformation assumes ordinary immutable
allocated data: volatile, atomic, concurrently mutated, device and faulting or
partially allocated memory are excluded. Different access widths do not imply
equivalence in those settings.

## Control, data and model limits

Arithmetic families allocate one unused word; memory/control families allocate
256 words with mask 255. The existing driver initializes ordinary storage and
runs the oracle before the timed kernel. These are warmed-data experiments;
there is no cold-cache flush, cache-size sweep, concurrent mutation, data
prefetch control or guarantee that all lines remain resident. The oracle's
access sequence also warms the dependency path. Memory source proofs establish
allocation safety, not cache hit rates. This extension adds no harness knobs,
new ISA feature bits, PMU counters or cache/branch distributions.

MCA extracts for the five straight-line families must match the actual body
between `L_loop` and counter decrement for each variant. Constants are assumed
ready in these extracts; entry/setup, loop control, return, cache hierarchy and
branch prediction are omitted. Multiplication/division instruction costs and
port pressure are model-specific. No Apple M5 model, hardware cycle estimate,
additive cost model or performance promise follows. Native correctness and
source review must precede any opt-in benchmark.

The source tests interpret the actual small C oracle grammar and compare it
against independent mathematical definitions, exercise adversarial data, and
check actual assembly guards, caller-saved operands, transformations and MCA
correspondence. Unknown oracle syntax fails. Source checks cannot certify C
compilation, machine encoding, object ABI/CFI, linker behavior or execution.
The separate strict runtime manifest parser checks all available manifests;
the full contract check explicitly reports pending until both committed
architecture sets are present. Final integrated acceptance requires all 19
manifests and no pending architecture tests. Native correctness is a separate
step; this document records no native or timing result.
