# Split single-call cache preference

This preference is part of the existing experimental Split allocator, selected
with the `experimental-regalloc` feature and `--regalloc split`. Normal Fast/Full
allocation and None/Fast/Full optimization defaults are unchanged. Full IR
optimization still runs three rounds. Global scan priorities, spill homes and
rematerialization are unchanged.

At a clean spill Use miss, a low-half pointer or i64 may occupy an otherwise free
callee-saved GPR until its next selected use in the same block, across exactly
one ordinary nonterminating call strictly between the uses. The register must
already be saved by the baseline allocation, be permitted by the target pools,
not overlap the caller-saved pool, be clean and unpinned, and be absent from
read/write unavailability. The preference adds no save, spill home or eviction.

The entire block is excluded if any selected operand has a non-Any constraint,
including a Reuse Def whose input Use remains Any. This protects later constrained
uses of *different* values displaced by the preference, as well as the candidate
itself. Both metadata maps are replaced before this early return, so stale
windows cannot survive a constrained block.

Global liveness enter events must leave the register free through the next
use's **write point, inclusive**. A global owner returning from a lifetime hole
counts as an entry. Local cache emptiness alone is insufficient. Candidates follow
target pool order; failure uses the existing acquisition path. Ordinary later
cache eviction remains possible and may erase the benefit.

Covered/duplicate uses, definitions and aliases are accounted for when finding
the next selected use. Inline assembly, intrinsics/helper-like or unknown
clobbers, Invoke/tail terminators and returns-twice prevent a call-window proof.
The existing whole-function vector/returns-twice fallback remains intact.
Only the pointer SSA value is retained: calls and stores may still mutate its
pointee. This introduces no memory forwarding or arithmetic movement. Existing
dirty Def handling, source forwarding, call clobbers, parallel copies, block
flush/reset, CFG edges and allocation finishing remain in the cache walk.

`split_call.rs` builds candidate enter lists from sorted global events. Each
all-Any block gets a backward next-use/call/barrier pass; candidate event cursors
advance monotonically. With B blocks, I instructions, O selected operands,
E global events and a small target pool P, expected added work is
O(B + I + O + E + P * eligible misses), plus fixed-pool membership checks.
Fresh maps charge growth/drop work to the block that grew them, avoiding repeated
clears at the largest previous capacity. Peak storage is O(candidate enter events
+ eligible values/windows in the largest block). Hash operations have expected
bounds; allocation/deallocation cost and compile overhead remain unmeasured.

The private post-scan test seam compares the old and preferred cache walks with
the same selected Function, computed intervals and explicit baseline homes.
The positive token-oracle test removes one home read/transfer while keeping spill
homes, spill units, saved registers, stack destinations and edge transfers equal.
This is an allocator assertion, not a frame-layout or runtime measurement.

Fresh qualification on current main (`86c064365b46`) on ARM macOS:

- All 17 R3 tests pass under `regalloc::split::tests`; this filter must execute
  17 tests, rather than the obsolete zero-match `regalloc::split_tests` filter.
  Coverage includes whole-block Fixed/Register/Reuse exclusions, different-value
  displacement, fresh metadata/capacity accounting, inclusive owner/hole return
  boundaries, occupied/dirty/pinned/unsaved candidates, call barriers, aliases,
  scalar halves, vectors/returns-twice, source forwarding and CFG edges.
- The full experimental backend library passes 71 tests.
- CLI tests pass both with and without the feature (31 each); both independent
  tier/default compatibility tests pass.
- All 61 host-applicable native programs pass in four Split configurations
  (244 executions): real/tiny pools with baseline selection/unoptimized IR or
  full selection/Full IR optimization. The eight default Full-allocation
  configurations also pass (488 executions).
- Assembly is byte-identical to frozen current main in 840 source/target/mode
  comparisons across both supported targets: default invocation and the
  None/Fast/Full optimization modes with normal Fast/Full allocation. Both
  comparison binaries have the experimental feature enabled; Split is opt-in.

The focused gate is `cargo test --offline --locked -p keel-codegen --features
experimental-regalloc --lib regalloc::split::tests`. Native gates use `--test run`
with `split_allocator_programs -- --exact` and `programs -- --exact`. Validation
used separate Cargo targets for this worktree and the frozen-main snapshot, with
all Cargo/native commands serialized by `suite/fast_opt_locked.py`.

The token oracle checks values, copies and pool clobbers; it does not model
concrete ABI staging, pointee memory, instruction encodings or exception runtime.
Native corpus checks provide complementary coverage and do not establish x86
native runtime correctness. No suite-wide runtime benefit, compile-cost result
or default promotion is claimed. Register competition can worsen other cache
choices; this remains experimental.
