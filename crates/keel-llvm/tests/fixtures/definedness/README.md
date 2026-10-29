# Definedness boundaries

These fixtures test LLVM-defined executions whose unused intermediate results
may be poison. The importer must not turn those unused results into traps or
unconditional arithmetic/pointer promises.

`masked.ll` and its unsigned C checks cover signed-add overflow masked by select
and by a PHI edge, an oversized shift masked by select, and a poisoned nonnull
argument masked before a load. Selected signed-add results are tested only when
they fit; selected shifts are tested only below the bit width. The null argument
is passed through an LLVM wrapper without a call-site noundef annotation, so a
C frontend cannot accidentally strengthen that boundary. Non-null selected
pointer inputs reference a live aligned object.

`division.ll` checks a poison dividend refined to INT64_MIN. The existing
importer selects divisor one for the overflow pair, preventing a new Keel trap
on a select-masked path. Zero divisors and concrete unmasked MIN/-1 division
are not correctness inputs here. Division export is still unsupported, so its
round trip stops at verified Keel; an optional native Keel executable tests the
three optimization tiers.

`refinement.ll` checks explicit scalar poison and undef, masked selects/PHIs,
freeze of concrete integers/pointers/FP bits, propagated undef, flagged overflow,
and repeated uses of freeze results in loops. Its driver checks stable outcomes
and permitted ranges, not equality of arbitrary source and imported choices.
Original LLVM and re-exported LLVM run at O0/O2; optional native Keel runs at
none/fast/full. Raw scalar acceptance and the remaining aggregate/global
rejections are tested separately in `definedness.rs`.

`richer_facts.ll` checks return/parameter/indirect-call ranges, poisonable ranges
masked before observation, return-only capture, nullable readable extents and
allocation-like noalias returns. The driver stays inside retained range and
pointer promises. Both LLVM O0/O2 paths and Keel none/fast/full must print the
same verdict. Structural tests separately verify exact scope and transport.

`local_facts.ll` checks load-result ranges and pointer facts, including separate
cell/result alignments, nullable readable extents, masked poison without noundef,
and a second load after a call overwrites the cell. Valid source and round-trip
LLVM executables run at O0/O2; native Keel runs at none/fast/full. Result facts
must not transfer to siblings or make later loads non-faulting.

```sh
KEEL_TEST_LIBLLVM="$(rustc --print sysroot)/lib/libLLVM.dylib" \
KEEL_TEST_KEEL="$PWD/target/debug/keel" \
  cargo test -p keel-llvm --test import definedness::
```

Execution needs Apple arm64 and Clang. LLVM-dependent tests report a skip when
the library variable is absent; native division execution reports a skip when
`KEEL_TEST_KEEL` is absent. Supply an absolute executable path because Cargo runs
integration tests in the package directory.
