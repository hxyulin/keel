# Wrapping integer reduction unrolling

`loop-unroll` is an explicitly named function pass. It is registered in the pass
catalog and is absent from every optimization tier/default pipeline. Root owns
any future activation after combined correctness, runtime, code-size, and
compile-cost gates. It has no dependency on `AliasInfo` and makes no IR, ABI,
calling-convention, memory-contract, or verifier changes.

The motivation is the retained measured `hot` kernel: unsigned
`sum += (p[i] ^ k) * (i + 1)`. Its LLVM/Clang routes use two independent scalar
accumulators. The audited whole-route contrast also includes wrapper inlining,
call/frame handling and address selection. It is not an estimate of this pass's
speedup or an application gain. This checkpoint performs no measurements.

## Accepted shape

A natural loop has a two-instruction header (`icmp.ult iv,bound`, or the equivalent
`icmp.ugt bound,iv`, then a true-body/false-exit branch), one straight-line body,
and one unconditional external preheader. Header arguments are exactly the IV
followed by one sum; each can independently be i32 or i64. Entry IV is constant
zero, bound and seed dominate the preheader, and the backedge increments IV by
one. The sum update is a Keel wrapping integer `iadd(sum, independent_term)` in
either operand order. Neither the sum nor its updated result may feed other body
instructions. The independent term can depend on IV and ordinary loads.

Body operations are restricted to integer literals, wrapping add/subtract/multiply,
bitwise operations, modulo-width shifts, pointer addition, integer width
conversions, and one or two ordinary i32/i64 loads. Result types are limited to
scalar i8/i16/i32/i64 and pointers. Memory flags are copied exactly. There are no
new alias, null, alignment, lifetime, or `notrap` inferences. Both bodies retain
all operations, including unused loads, in original order.

The pass rejects stores, calls, volatile accesses, atomics, lifetimes, trapping
integer division/remainder, floating point, conditional bodies, additional exits,
body parameters, body-defined external liveouts, dependent recurrences, nested
loops, and functions carrying debug return/location/variable/scope information.
It leaves all such candidates unchanged. Arbitrary invariant starts, signed IVs,
other predicates, additional carried values, and a general rotation framework
are outside this initial scope.

## Transformation and safety argument

The new paired header carries `(iv, lane0, lane1)`, initialized to `(0, seed, 0)`.
It enters the twice-cloned body only when unsigned `bound - iv >= 2`. Entry
establishes `iv <= bound`. A paired step advances by exactly two and the guard
proves both unit increments remain at or below bound without wrapping, preserving
that invariant. In particular the guard never uses wrapping `iv + 1 < bound`.
The second iteration's loads execute only after proof that both original
iterations exist. No body load is moved into either header.

Each cloned body maps the original sum to its own lane; the second clone maps
IV to the first clone's updated IV. At paired exit, the two lanes combine once
with wrapping integer addition. Execution enters the original header with that
sum and the current IV. The original scalar loop handles zero or one remaining
iteration and all original exit arguments/direct header-parameter liveouts.
Zero-trip paths do not load. Modular associativity of integer addition justifies
the regrouping; no floating-point reassociation occurs. Original address
computations, including their modular behavior, are cloned without alteration.

The old header cannot be selected again because its new incoming IV is a dynamic
paired-header parameter, rather than literal zero. The paired header has three
parameters and cannot match. Idempotence is checked both immediately and after
default cleanup; the default vectorizer continues to see the original scalar
fallback. Reductions remain unsupported by the current vectorizer; activation
order should be reconsidered if a reduction vectorizer is added.

## Bounds and validation

The original body has at most 20 instructions including its backedge. The paired
body clones at most 19 instructions twice, adds one backedge, and adds seven
header/combine/setup instructions: at most 46 new instructions (tests allow a
conservative ceiling of 54) and exactly three blocks per candidate. A reverse
liveness estimate rejects candidates exceeding 16 simultaneously live scalar
values, including invariant operands. This is a target-independent pressure
heuristic, not a proof that either target avoids spills. There are at most two
loads per scalar iteration. Root's later code-size and compile-cost gates remain
necessary.

Source tests in `crates/keel-opt/tests/loop_unroll.rs` exercise deterministic actual
unrolling, independent lanes, verifier/parse round trips, repeat-pass behavior,
explicit pass plus default cleanup, wrapping i32/i64 sums/seeds, mixed widths,
0/1/2/3/odd/even/many iterations, IV liveouts, exact ordered loads, unsupported
operations, nested loops and body/pressure caps. A checked-memory execution model
compares original and transformed IR, plus an independent modular sum oracle.
Near-MAX checks resume the generated paired loop at states satisfying its proven
invariant and execute at most three iterations; nonzero/near-MAX initial-IV
shapes are rejected rather than running astronomical prefixes.

After a root-granted exclusive local MacHost build/native lease, announce before
starting jobs, then run serially:

```sh
cargo test -p keel-opt -p keel-ir
cargo build -p keel
python3 crates/keel-opt/tests/loop_unroll/validate.py \
  --keel target/debug/keel \
  --out suite/reports/optimization-wave7-loop-unroll-20261017-validation
```

The script uses an existing compiler and creates a fresh ignored evidence
directory. It records full commands and compiler/artifact SHA-256 hashes. First
it compiles the bounded C variants of the measured hot kernel through the frozen
scalar pipeline, asserts one load becomes three in each function, asserts a
three-parameter paired header and remaining-count guard, and checks idempotence.
Only after those assertions does it generate both AArch64 Mach-O and x86-64 ELF
assembly/objects for scalar, explicit-unroll, and unroll-plus-default-cleanup IR.
Unsupported C cases must remain byte-identical under the named pass.

The separate-TU driver compares scalar, unrolled, cleaned, and Clang-compiled C
objects against unsigned arithmetic assertions. Arrays end exactly at a protected
page; zero trips also use a null pointer. It checks exact last loads/no overread
across 0/1/2/3/odd/even/many counts with wrapping words and seeds. There is no timer
or speedup calculation. Release the lease explicitly after every job has ended.
Full relevant tests, actual-C activation, both-target codegen and native assertions
are pending at the source checkpoint; formatting and source inspection alone do
not establish correctness. Independent correctness review is root-owned after
the committed checkpoint. Never commit generated reports, binaries, or logs.
