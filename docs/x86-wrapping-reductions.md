# Bounded x86 wrapping add reductions

The x86 backend retains `vreduce_add` on **i64x2 and i32x4** through shared
vector legalization. Selection uses only SSE2: `pshufd`, `paddq`/`paddd`, and
`movq`/`movd`. No frontend, optimizer, preferred-width, or ABI policy changes
are involved. C vector expressions and intrinsics retain their existing
frontend contracts; this selection applies when typed IR contains the operation.

For i64x2, swap the two qwords with `pshufd 0x4e`, then add. For i32x4,
first add opposite 64-bit halves with that shuffle, then add adjacent dwords
using `pshufd 0xb1`. Extract lane zero at the original lane width. Packed
addition wraps modulo 2^64 or 2^32, so this tree has exactly the same bits as
the previous sequential lane sum. Integer signedness does not change those
bits. An i32 result is written through a 32-bit GPR view; subsequent `sext`
and `uext` retain their distinct meanings. No widening sum is implied.

Selection stages the input in reserved xmm14 before defining the scalar
result, and uses reserved xmm15 for permutations. Allocated input registers
remain untouched, including inputs live across calls or after the reduction.
GPR and XMM register numbers refer to distinct register classes. The existing
allocator validation rejects allocations overlapping reserved scratch. Spilled
inputs reload exactly 16 bytes; ordinary scalar spill/result protocols remain
unchanged. Unspilled reductions require no lane buffer or lane memory traffic.

Other reduction operations, i8/i16 lanes, 64-bit vectors, 256-bit reductions,
and all FP operations retain existing behavior. In particular, this does not
change FP evaluation order, min/max semantics, the PMULLD helper, the AVX
conversion result-width guard, or Apple ARM64 lowering. The ordinary feature
guard still checks both debug and release emission.

## Validation and reproduction

`crates/keel-codegen/tests/vector_reduce_add_x86.rs` covers strict v1/v2/v3,
effective v3 with AVX/AVX2/FMA/F16C disabled, three selection tiers, fast/full
allocation, normal/tiny/empty pools, forced register views and input/result/
shared spill homes, reserved-scratch rejection, text/typed/chunked whole-object
parity (including metadata), and excluded-operation controls. The native C
oracle checks every combination of eight boundary values across the lanes
(64 i64x2 / 4,096 i32x4 cases), followed by 4,096 lane-probe/random inputs
per shape, each lane independently,
wrapping arithmetic, vector parameter/scalar result ABI, signed/unsigned
extension, live values across calls, unaligned memory, output canaries, input
preservation, and both protected-page edges. Its random reference uses unsigned
scalar arithmetic independently of the selected shuffle tree.

Use the existing toolchain and shared lock:

```sh
KEEL_REDUCE_EVIDENCE=/tmp/keel-reduce-current \
  python3 suite/fast_opt_locked.py /tmp/keel-reduce-build.jsonl \
  cargo test --offline --locked -j2 -p keel-codegen \
  --test vector_reduce_add_x86
```

For Linux x86 native execution, export `current` and frozen `baseline`
directories containing `oracle.c`, `.s`, and `.o` files under a `/tmp` root.
Run `tests/vector_reduce_add/run_native.py ROOT` (paths relative to
`crates/keel-codegen`) through `suite/fast_opt_locked.py`. It links each oracle
with clang, executes it, and independently compares clang-assembled `.text`
bytes. `bench.py ROOT` in that directory alternates nine CPU-pinned samples
of the same v1/full-selection/full-allocation objects with the same changing
inputs; use the shared lock for that command too. Baseline exports must come
from a frozen revision with this fixture ported and new-recipe assertions
omitted, rather than from another worker's changing checkout.

The ignored `compile_cost` test measures only typed code generation of the
same four-function module (20 warmups, 2,000 iterations). Run it separately
with `--release ... compile_cost -- --ignored --nocapture --test-threads=1`
on both revisions. Parsing, Rust builds, assembly, linking, and execution
are outside that measurement. Build wall times in the lock logs are separate.

## Bounded evidence, 2026-10-23

Compared with frozen `8690a355b2986288e5b53b6401a79ba342cc16f9`, LinuxHost
(Ryzen 9 9950X3D) passed 90 current and 54 baseline oracle objects plus clang
encoding comparisons. Excluded reduction assembly and Apple ARM64 reduction
assembly matched the frozen baseline byte for byte. Existing Apple ARM64
native vector/ABI tests passed locally.

In the v1/full/full unspilled `sum` functions, explicit data-memory operands
fell from three to zero for i64x2 and five to zero for i32x4. The reserved stack
frame fell from 32 to 16 bytes; prologue/epilogue push/pop traffic remains.
Instruction counts stayed at 12 and 14 respectively. Call-save and allocator
spill traffic elsewhere is still required.

Nine alternating 10-million-call samples on LinuxHost gave baseline/current
medians of 1.4759/1.4604 ns for i64x2 and 1.5223/1.4954 ns for i32x4. Ranges
overlap (roughly 1.46–1.90 ns), including cold/boost variation. Shared-lock load
was 0.64 before and after. These short external-call kernels establish no
application speedup; performance conclusions require a longer controlled run.
A separate MacHost release-profile typed-codegen sample recorded 53.72 µs per
four-function module at the baseline and 25.16 µs with this selection. Host
load was 13–16 and other builds were active, so compile-cost conclusions are
deferred; these are contaminated observations, not a validated speedup.
Generated evidence and build/measurement logs remain under `/tmp`.

## Independent review, 2026-10-23

Candidate `30402b244982d585e7591bba02aa774bfc238e90` was reapplied onto
`4611704b187f44eeafd347d29875545b89e63eac` in an isolated review worktree.
No production correctness defect was found. Review strengthened the finite
oracle with the boundary cross-products above and checked rejection of each
reserved physical SIMD register (13, 14 and 15). A focused legalizer test
checks all eight integer reduction operations across D/Q and representative
Y shapes at v1/v2/v3, including the retained scalar result type. Existing
AVX2 Y reductions remain retained; other excluded operations still scalarize.
The AVX conversion result-width fallback and effective-feature PMULLD helper
remain intact.

Fresh debug and release reduction gates passed (three tests each; the explicit
compile-cost measurement stayed ignored). The 23 x86 library checks passed
in both profiles, including the release emission guard, D-width fallback,
PMULLD retention and the new reduction retention test. The complete debug
library suite also passed (115 tests). Text, typed and chunked
object checks passed. All 90 debug/release exported objects were byte-identical.
A frozen `/tmp` source snapshot of `8690a355` generated 54 baseline objects
using the same expanded oracle, with current-recipe assertions and custom
allocation overrides omitted. Baseline/current input IR and oracle C matched;
the Apple ARM64 control and all three excluded-reduction assemblies also
matched byte for byte. An additional 22 regressions passed for Apple ARM64
(native), shared owned legalization rounds, SSE widening, AVX256 and AVX
conversion width fallbacks. Existing vector IR/x86 and strict-v1 join gates
also passed (16 tests).

LinuxHost executed all 90 current plus 54 baseline debug-exported objects and
independently matched each object's text bytes against Clang assembly. A
separate release run passed all 90 current objects and encoding comparisons
(234 total native object runs). This
covers finite extrema, random values, lane isolation, modular overflow,
explicit sign/zero extension, forced spills/shared homes, empty pools, live
sources, calls, scalar/vector ABI, unaligned accesses, canaries and guarded
input-page edges. Review artifacts, SHA-256 source/object provenance and
shared-lock logs are under `/tmp/keel-reduce-review-b8d47a24` on LinuxHost and
`/tmp/keel-reduce-review-{debug,release}` locally. No timing campaign was run.
Native execution used a newer x86 CPU, not a v1-only machine; strict-v1
legality is established for the tested emissions by feature checks and
encoding validation. This is bounded backend validation, with no new frontend,
FP-environment, exhaustive ISA or performance claim.
