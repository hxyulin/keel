# Bounded cross-block load reuse

`load-reuse` is an explicitly registered opt-in function pass. No default,
Full or Fast pipeline includes it. Run it after GVN when independently computed
addresses need to become identical SSA values (the current Fast isort needs
this preparation; the Full isort does not).

The pass never introduces or moves loads. An ordinary, successful load makes
its value available for an exact type and canonical address. Availability flows
only from a completed unique predecessor in reachable reverse postorder. Entry,
merges, unprocessed predecessors and backedges cannot inherit facts. Separate
exit snapshots isolate siblings. A loop header resets availability, while a
header's successful load may still be reused by its uniquely entered body.
Block arguments are not translated and no merge intersection is attempted.

Chains stop after eight edges; snapshots hold at most 128 facts. At the fact
limit the entire map is cleared deterministically. There is one CFG traversal,
no fixed point, and no path enumeration. Snapshot storage and alias checks per
write are bounded by the fact limit. Address analysis is shared with
`memory-forward`: exact resolved SSA or checked constant-i64 ptradd paths,
with circular byte footprints and existing eligible restrict contracts.
Unknown or overlapping writes kill facts; pointer-width writes conservatively
kill all facts. Stores do not establish new load facts. Volatile accesses,
atomics, fences, calls, inline asm, debugtrap and lifetime barriers clear facts
through the IR memory-effects model. Functions with source debug variables are
excluded entirely because their stack homes can be edited between source
locations. Alignment and notrap annotations never establish alias freedom.

The original first successful load remains pinned, including if otherwise
unused. Intervening faulting accesses retain their order. A later duplicate
can disappear only after that original access has succeeded and without a
possible memory mutation or lifetime barrier on the inherited path. No new
notrap, noalias or integer-overflow flags are introduced.

Regression tests are in `keel-opt/tests/load_reuse.rs` and
`keel-codegen/tests/load_reuse.rs`. The latter exercises both targets, Fast/Full
IR tiers, baseline/full selection and Fast/Full allocation with object encoding.
For native macOS ARM validation, first build a worktree-local CLI, then run
`suite/check_load_reuse.py --keel <binary> --output <evidence-directory>` under
the shared `suite/fast_opt_locked.py` lock. It emits both targets, checks all 795
kernel driver lines against Clang O2, asserts isort's three loads become two,
and runs an independent C oracle at Clang O0/O2 for each IR/allocation tier.
The oracle checks aliasing/partial writes, unaligned accesses, integer edge
patterns, call mutations, widths and protected-page first-fault/untaken paths.
It performs no performance measurement and makes no native x86 claim.
