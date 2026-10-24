# Unsigned byte sign-extension recognition

`sign-extend-idiom` is a named, opt-in function pass. It is absent from all
optimization tiers and default pipelines; run it on optimized C IR explicitly,
for example `keel opt -p sign-extend-idiom,dce optimized.keel -o out.keel`.

It recognizes the shape the C frontend and CFG simplifier produce for a manual
signed-byte conversion (fixture:
`crates/keel-opt/tests/fixtures/sign_extend_idiom/bridge_sext8.ir`, captured
from optimized C and identical on both targets): `state & 255`, unsigned
`< 128`, a direct merge edge for the positive result, and a single
`byte - 256` block for the negative result.

The matcher requires exactly four header definitions plus the conditional
branch, a three-instruction negative arm, one i64 merge parameter, exactly one
incoming arm edge and exactly two merge edges. It counts edges from the entire
layout, including unreachable blocks. The masked source must resolve to a block
parameter. The mask, comparison, constants and subtraction must have only their
expected uses. Only i64 arithmetic and byte width are supported. `ult 128` and
`uge 128` with the corresponding opposite branch polarity are supported;
signed comparisons, other bounds, widths and arithmetic are rejected.

For b = state & 255, b < 128 returns b. Otherwise b - 256 modulo 2^64 returns
the two's-complement representation of b's signed byte value. Consequently
`sext.i64(trunc.i8(state))` equals both paths for every input. Truncation and
sign extension cannot trap. The pass inserts these existing operations before
the branch, changes the branch to a jump with that value, and removes the
exclusive negative arm using existing Function APIs. It leaves the merge
parameter and surviving instruction identities intact. Dead header operations
can be removed by a separately requested existing DCE pass.

No loads, calls, effects or trapping operations can occur in either matched
region. Nothing is speculated or moved across the loop guard. Negative-arm
source locations cause rejection, preserving stepping points; the original
branch location is retained and copied to both conversions. Surviving header
locations and function debug metadata are unchanged. Operands resolve aliases;
aliased dead definitions cannot masquerade as live result owners, and no cloned
alias keys are published.

Regression tests (`cargo test -p keel-opt --test sign_extend_idiom`) cover the
captured C shape, all 256 bytes, high-bit edge seeds, zero and multiple
iterations, polarity and mask order, noncanonical widths/bounds/predicates,
extra predecessors and shared results. No performance claim is made for the
pass.
