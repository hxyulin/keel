# Aggregate temporary checkpoint, 2026-10-17

Exact initial source: f614e7b38da7cf14222741ab491f8bbef9e17a9d,
tree d046bdfafcf7052387e72c5badc3d09a564d475c. Isolated clean worktree
verified before edits. Other wave8 workers' commits are not included.

Owned implementation: `crates/keel-opt/src/stack_forward.rs` and
`crates/keel-opt/tests/stack_forward.rs`; this checkpoint document. No changes
to inline policy, frontend, pass registration, command line, ABI or frame layout.

## Exact source and retained IR diagnosis

The reviewed current-runtime-compile-profile-20261017 report and its
`data-review-r1/report.md` establish a 5,456-byte initial Keel frame versus
Clang's 848 bytes. The IR has 505 declared slots/7,820 bytes and 305 referenced
slots/4,940 bytes. These are fixed objects, not an allocator spill count.

Frontend `irgen.rs:1834` creates a distinct result slot for struct calls;
`:1846` describes call-time ABI consumption, and `:1871` freezes aggregate
arguments before later effects. Inlining may expose their scalar stores and
loads. Existing stack-forward only carries facts inside a block. Codegen
`stack_frame.rs` retains every surviving StackAddr (and every home in debug
modules). AArch64 `move_args` loads byval register pieces or copies larger
arguments; `call_result` stores register-returned pieces through saved sret
addresses. The x86-64 backend independently implements its own aggregate ABI.
Changing those interfaces is outside this proposal.

Retained evidence (ROOT suite/reports/current-runtime-compile-profile-20261017/
inspection/JS_CallInternal.keel): slot ss493 is 16 bytes, align 8.
At lines 3532–3539, block320 stores v18794:i64 at offset 0 and v18798:i64 at
8. Its branch optionally visits block1407, which reads load.ptr at offset 0
and increments the pointed-to reference count, then joins block1408. Lines
10299–10300 load both i64 pieces again. Both stores dominate these loads;
there are no other writes or escapes of ss493. The pointer load has a different
type from the stored i64 and is deliberately outside the proposed transform.
The exact extraction and retained input hash are in ignored
`suite/reports/aggregate-temporaries-20261017/`.

## Bounded opportunity

Extend existing private-stack forwarding with exact-type cross-block facts.
A candidate store's byte range must have no overlapping other store anywhere
in the function, and its instruction must dominate the load (including order
within one block). Reuse the pass's whole-function escape exclusions and
existing dominator implementation. No phi creation, pointer conversion,
address substitution, new lifetime marker, slot coalescing or ABI change.

For ss493, expected source-level result: remove the two i64 reloads in
block1408; keep load.ptr in block1407 and both stores. This case therefore
predicts memory-operation reduction, not slot/frame elimination. Other fully
forwarded private objects can use the existing store cleanup and later DCE.
No measured lower frame, runtime or compilation-cost claim is made.

A syntactic unique store may execute repeatedly in a loop: dominance ensures
the current execution precedes every eligible load, and no competing writer
can intervene. Missing initialization paths, backedge-only stores and
unreachable blocks fail dominance. Overlapping and partial writes reject the
candidate; unrelated regions may qualify independently. Calls cannot access
these nonescaping homes. Existing frame-address/stack-save/restore,
returns-twice and inline-assembly exclusions remain global barriers. Debug
homes remain excluded, preserving address stability and debugger edits.
Snapshot slots passed to calls escape and remain untouched. In-memory alias
states must not cause a defining result alias to be overwritten or cloned.

## Acceptance before integration

Source-only now: commit this checkpoint before implementation/review; add
focused exact-type diamond and loop tests, competing/partial writes, missing
initialization, escapes, offsets, debugger homes and alias-state negatives.
No tests/builds/native execution are authorized in this phase.

After a host grant: run keel-opt focused tests and existing full relevant
checks; replay the exact retained QuickJS input at this base plus this change
and assert actual ss493 before/after instructions and preserved pointer load.
Record affected function and IR storage counts, not only pass registration.
Compile positive/negative aggregate C fixtures against native Clang and run
both x86-64 and AArch64 functional suites (on suitable hosts), including
mutation of later arguments and byval/sret alias/snapshot cases. Compare both
text and structured backend emission. Capture compiler/binary/input hashes.
Measure text size, actual frame prologues and compile cost only after a later
performance grant; do not infer frame savings from removed loads.

Independent source review and root combined validation are required. Any
broader lifetime/provenance/aggregate ABI proposal returns to root as a design
checkpoint. No nested workers, pushes, installs or unrelated cleanup.
