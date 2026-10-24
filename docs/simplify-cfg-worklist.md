# Simplify-CFG block merging

`simplify-cfg` merges a block into its predecessor when the predecessor ends in
an unconditional jump and that jump is the block's only incoming edge. This note
describes how `merge_blocks` (`crates/keel-opt/src/simplify_cfg.rs`) iterates
candidates without restarting a layout search after every merge.

## Algorithm

The pass takes one branch-edge census and one snapshot of the block layout, then
visits each predecessor in layout order, retrying it immediately after a
successful merge.

This is sound because a merge removes only the jump edge into the removed block.
Moving that block's instructions preserves every outgoing edge, including
parallel edges and backedges, so incoming counts for surviving blocks do not
change. Only the absorbing predecessor gets a new terminator; previously
rejected surviving predecessors remain ineligible. Snapshot entries for blocks
that were already absorbed have empty instruction lists and are skipped.
Merged blocks leave the layout together at the end.

The result is the same first-eligible-layout merge sequence as a full rescan,
including backward layout edges, without a second census or membership
searches. Candidate checks are bounded by the snapshot length plus the number
of successful merges. The snapshot adds one allocation proportional to live
blocks; instruction movement and alias resolution keep their existing costs.
This does not make the whole merge operation linear.

## Invariants

- The entry block is never merged away, and a block never merges into itself.
- Block-parameter aliases are installed in parameter order before the jump is
  removed; instruction order and debug locations are unchanged.
- After each merge the removed destination's edge count is decremented and the
  absorbing predecessor's new terminator is re-examined.
- The outer cleanup rounds of `simplify-cfg` are unchanged.

No IR, calling, memory, canonicalization, verification or debug contract
changes. Unit tests in `simplify_cfg.rs` cover layout order, stale snapshot
entries, cycles and aliased arguments; `crates/keel-opt/tests/cfg.rs` covers
duplicate branch edges, switch dispatch, chains and invokes:

```sh
cargo test -p keel-opt --lib simplify_cfg
cargo test -p keel-opt --test cfg
```
