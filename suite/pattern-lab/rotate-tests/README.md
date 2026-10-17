# Constant rotate recognition

Canonicalize recognizes `(x << L) | (x >> R)` for the same resolved i32/i64
value and literal counts with `0 < L,R < width` and `L+R=width`. Either OR
operand order is accepted. It writes existing `rotr x,R`, retaining result
identity, source location and the original shift instructions; ordinary DCE
can remove those shifts. It resolves aliases before inspecting definitions
and before publishing operands. No backend, pass-list or language change is
required. Arithmetic right shifts, different values, dynamic counts, other
widths and noncomplementary/out-of-range counts are rejected. Recognition
introduces no speculative evaluation, loads, trapping operations or effects.

The choice of right rotate avoids the existing ARM left-rotate lowering's
extra count negation. ARM currently materializes the constant in a register
and emits register ROR; it does not fold this case to immediate ROR. The
actual C bridge originally used LSR13 plus ORR with shifted LSL51 on ARM,
and SHL51/SHR13/OR on x86. After recognition its optimized IR uses `rotr 13`,
x86 emits immediate ROR13, and ARM emits a constant13 load followed by
same-value register ROR. This establishes activation and equivalent lowering,
without a timing or profitability claim. Handwritten microexperiment gains
do not transfer to generated loops.

Optimizer regressions:

```sh
CARGO_TARGET_DIR=/absolute/private/target cargo test -j2 -p keel-opt
```

After the integration owner grants an exclusive correctness lease, run the
explicit validation script using a separately built compiler and a new output
directory. It never builds a compiler or runs a benchmark itself:

```sh
python3 -B suite/pattern-lab/rotate-tests/validate.py \
  --compiler /absolute/private/target/debug/keel \
  --output /absolute/evidence/new-validation
```

It retains actual C inputs, commands, stdout/stderr, IR, text assembly, direct
objects, independently assembled objects, native binaries and SHA256 values.
It asserts real C activation for all 31 i32 and 63 i64 nonzero counts and the
original `bridge_rotate` loop, before native compilation. Even counts commute
the OR operands. Dynamic shifts, different input values and arithmetic right
shifts must retain fallback IR. The signed arithmetic-shift C fixture is only
an IR negative case; it is never executed, because signed left shift can be
undefined. In-memory IR tests additionally cover other widths, invalid counts,
source/count/shift aliases left in layout, result identities and idempotence.

Full selection, baseline selection and `-g -O` modes emit IR/text/objects for
AArch64 Darwin and x86-64 Linux. Debug locals remain in memory, so two loads
of a local have distinct SSA values and safely miss recognition. Native ARM
checks cover eight edge values, every legal constant count, dynamic counts
1..63, all edge-value pairs for different-source shifts, and bridge counts
0/1/2/17/257 with null data. Each mode executes both direct and assembled
objects against a separate Clang `-O0 -fno-lto` oracle translation unit.
There is no native x86 validation on MacHost and no cross-target runtime claim.
Failures preserve the partial output directory. Run a new directory after a
fix; never overwrite prior evidence.
