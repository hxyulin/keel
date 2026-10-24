# Wrapping integer reduction unrolling

`loop-unroll` is an explicitly named function pass. It is registered in the pass
catalog and runs in Full's late memory/loop epilogue, before
`loop-reduce-multi` and `loop-rotate`. Other tiers require explicit scheduling.
It has no dependency on `AliasInfo` and makes no IR, ABI,
calling-convention, memory-contract, or verifier changes.

The motivating shape is a `hot` benchmark kernel: unsigned
`sum += (p[i] ^ k) * (i + 1)`, which LLVM/Clang compiles with two independent
scalar accumulators. No speedup estimate is claimed for this pass alone.

## Accepted shape

A natural loop has a two-instruction header (`icmp.ult iv,bound`, or the equivalent
`icmp.ugt bound,iv`, then a true-body/false-exit branch), one straight-line body,
and one unconditional external preheader. Header arguments are exactly the IV
followed by one sum; each can independently be i32 or i64. Entry IV is constant
zero, bound and seed dominate the preheader, and the backedge increments IV by
one. The sum update is a Keel wrapping integer `iadd(sum, independent_term)` in
either operand order. Neither the sum nor its updated result may feed other body
instructions. The independent term can depend on IV and ordinary loads. Header parameters must
resolve to themselves; an aliased comparison result is rejected. Body operations
must each have exactly one result that resolves to itself: retained dead alias
definitions must never overwrite clone mappings for invariant/earlier values.

Body operations are restricted to integer literals, wrapping add/subtract/multiply,
bitwise operations, modulo-width shifts, pointer addition, integer width
conversions, and one or two ordinary i32/i64 loads. Result types are limited to
scalar i8/i16/i32/i64 and pointers. Memory flags are copied exactly. There are no
new alias, null, alignment, lifetime, or `notrap` inferences. Both bodies retain
all operations, including unused loads, in original order.

The pass rejects stores, calls, volatile accesses, atomics, lifetimes, trapping
integer division/remainder, floating point, conditional bodies, additional exits,
body parameters, aliased body results, body-defined external liveouts, dependent recurrences, nested
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
default cleanup. Full runs its original vectorization/reduction rounds before
unrolling and the extended `loop-reduce-multi` pass afterward. These passes
retain their own eligibility checks and scalar fallbacks.

## Bounds and validation

The original body has at most 20 instructions including its backedge. The paired
body clones at most 19 instructions twice, adds one backedge, and adds seven
header/combine/setup instructions: at most 46 new instructions (tests allow a
conservative ceiling of 54) and exactly three blocks per candidate. A reverse
liveness estimate rejects candidates exceeding 16 simultaneously live scalar
values, including invariant operands. This is a target-independent pressure
heuristic, not a proof that either target avoids spills. There are at most two
loads per scalar iteration.

## Tests

```sh
cargo test -p keel-opt --test loop_unroll
```

The tests exercise deterministic unrolling, independent lanes, verifier/parse
round trips, repeat-pass behavior, explicit pass plus default cleanup, wrapping
i32/i64 sums and seeds, mixed widths, 0/1/2/3/odd/even/many iterations, IV
liveouts, exact ordered loads, unsupported operations, nested loops and
body/pressure caps. A checked-memory execution model compares original and
transformed IR, plus an independent modular sum oracle. Near-MAX checks resume
the generated paired loop at states satisfying its proven invariant and execute
at most three iterations; nonzero/near-MAX initial-IV shapes are rejected.

A native validator runs against a built CLI:

```sh
cargo build -p keel
python3 crates/keel-opt/tests/loop_unroll/validate.py \
  --keel target/debug/keel \
  --retained-hot path/to/hot/scalar.keel \
  --out target/loop-unroll-validation
```

It compiles bounded C variants of the `hot` kernel through the scalar pipeline,
asserts one load becomes three in each function, a three-parameter paired header
and remaining-count guard, and idempotence. The supplied `hot` scalar IR must
pass the same activation check. It then emits AArch64 Mach-O and x86-64 ELF
assembly/objects for scalar, explicit-unroll, and unroll-plus-cleanup IR, and
requires unsupported C cases to stay byte-identical under the named pass.

A separate-TU driver compares scalar, unrolled, cleaned and Clang-compiled C
objects against unsigned arithmetic assertions. Arrays end exactly at a
protected page and zero trips also use a null pointer, so the driver checks
exact last loads and no overread across 0/1/2/3/odd/even/many counts with
wrapping words and seeds. It does not time anything.
