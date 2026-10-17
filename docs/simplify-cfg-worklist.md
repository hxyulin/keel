# Simplify-CFG candidate iteration

Source checkpoint: `f614e7b38da7cf14222741ab491f8bbef9e17a9d`, tree
`d046bdfafcf7052387e72c5badc3d09a564d475c`.

The current runtime compiler profile and independent data review identify
`merge_blocks` restarting a layout search after every merge. The existing
branch-edge census and alias installation already avoid their earlier global
rescans. The observed first simplify-CFG transform total is 56.697 ms; this
proposal has no measured benefit yet.

Replace repeated searches with a layout snapshot, visiting each predecessor in
order and retrying it immediately after a merge. A merge removes only the jump
edge into the removed block. Moving its instructions preserves every outgoing
edge, including parallel edges and backedges. Incoming counts for surviving
blocks therefore do not change. Only the absorbing predecessor gets a new
terminator; previously rejected surviving predecessors remain ineligible.
Removed snapshot entries have empty instruction lists and must be skipped.
This preserves the original first-eligible-layout merge sequence, including
backward layout edges, without a second census or membership searches.

Keep the entry-block exclusion, self-merge exclusion, parameter alias order,
instruction movement and debug locations unchanged. Refresh the absorbing
predecessor's terminator and destination after each merge, decrement the removed
destination's edge count, and retain the outer cleanup rounds. No IR, calling,
memory, canonicalization, verification or debug contract changes are intended.

Source-only checkpoint: no Cargo, build, native execution, remote work or timing
is authorized here. Add focused regressions for layout order, stale snapshot
entries, cycles, parallel edges and in-memory aliases. Independent correctness
review and explicit root build lease are required before validation. Later
validation must establish retained application-corpus activation, IR/assembly
byte parity, full correctness and measured compile cost, including small-input
bookkeeping overhead. Oracle-only fixtures and source reasoning establish no
performance gain. Root owns integration and host scheduling.
