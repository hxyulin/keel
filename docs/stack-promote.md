# Private fixed-field promotion

`stack-promote` runs in Full's late memory/loop epilogue and requires explicit
scheduling in other tiers. Allocator defaults are unchanged.
Run it after inlining, CFG cleanup and, for small intrinsic initialization,
`small-memory`; follow it with `phi-simplify,dce`.

The pass admits scalar i8/i16/i32/i64 and pointer fields reached solely through
fixed stack addresses and constant, nonnegative, in-bounds pointer offsets.
Accesses to an overlapping range must agree exactly in offset and type, except
that a fully covering ordinary integer store of a constant repeated byte may
initialize a smaller integer field. The byte-splat representation is independent
of endianness and field offset. Arbitrary nonuniform bits, pointer representations,
partial writes and overlapping reads are not reinterpreted. Disjoint fields are
independent. Unsupported uses escape the entire slot,
including ordinary calls, intrinsics, volatile/atomic accesses, address comparisons,
returning an address, storing it, and nonconstant pointer offsets. Debugger homes
are excluded. Inline assembly, frame observers, stack save/restore, returns-twice
calls (direct or indirect), invokes and landing pads exclude the function.
Unrelated ordinary calls remain in place and cannot observe an unescaped slot.

A greatest-fixed-point must-initialization analysis starts false at entry and
true elsewhere. Every load must be preceded by an exact store or a fully covering
byte-splat store on every path. Each covering store supplies a typed constant at
its original instruction position and source location, including loop-local
resets. The complete original store remains in memory if any overlapping read
is unpromoted; otherwise it is removed after promotion. This preserves all bytes
of padding and neighboring fields that remain observable.
This includes zero-trip and loop-entry paths; the pass never invents an undef,
zero, speculative read, libc contract or writeback. Initialized non-entry blocks
receive fresh parameters, every syntactic incoming edge receives its outgoing
field value, and loads/stores become SSA uses/definitions. Duplicate edges retain
independent argument lists. Existing aliased load definitions are excluded.
The initial implementation requires all blocks reachable and no entry backedge,
and caps functions at 256 blocks, 8192 instructions and 64 distinct fields. Parameters are initially
unpruned; `phi-simplify,dce` removes the redundant ones.

## Actual corpus blocker

The retained 2026-10-20 `k_stackobj` calls ordinary declared `memset` with the
slot address. That escapes the slot. `stack-promote` deliberately leaves its
six field loads and three stores intact (plus the input load). Existing
`call-contracts` has no capture/alias contract that proves this use safe.

The diagnostic same-IR LLVM export declares the callee `nobuiltin`, without
`nocapture`/`noalias`. Its ARM assembly keeps `bl _memset` and initial field reads,
then promotes/vectorizes the recurrence. This is consistent with LLVM's stronger
allocation identity/alias analysis and loop memory promotion; it does not prove
that Keel may assume the ordinary call never captures the address. A future
region promotion needs a proof that subsequent input accesses cannot alias the
escaped field, no intervening observer can inspect captured memory, and correct
entry reads, exit writes and first-fault behavior on every region exit. This
pass supplies none of those assumptions.

`tests/stack_promote/private.keel` is an explicitly reduced fixture with ordinary
exact private initialization stores. It is not evidence of actual corpus
promotion. Its field memory instructions change from six loads and six stores
(three initialization stores plus three loop stores) to zero; its one input
load and two conditional branches remain.

## Validation

All build/compiler/native subprocesses use the shared investigation lock wrapper
and the worktree-local `CARGO_TARGET_DIR=target/kernel-gap-build`. No timings.

- `cargo check --offline --locked -j2 -p keel-opt`.
- `cargo test --offline --locked -j2 -p keel-opt`: 326 tests at the initial
  seven-test fixture revision; the focused final `--test stack_promote` passes
  nine tests, including additional exceptional/nonlocal and partial-init cases.
- `cargo build --offline --locked -j2 -p keel`.
- `python3 crates/keel-opt/tests/stack_promote/validate.py --keel
  target/kernel-gap-build/debug/keel --out target/stack-field-evidence/stage1
  --investigation target/kernel-gap-20261020
  --lock-wrapper suite/fast_opt_locked.py
  --lock-log target/kernel-gap-implementation-20261020/lock-events.jsonl`.

The serial validator emits ARM64 Mach-O and x86-64 ELF objects plus assembly for
five reduced-fixture variants, runs 10,401 independent unsigned-C oracle cases
per ARM variant (baseline, promotion, duplicate promotion variant, Fast cleanup,
Full cleanup), and compares both Fast+promotion and Full+promotion whole-driver
outputs against a fresh Clang O2 reference: 795 lines each. Clang uses
`-ffp-contract=off` to match Keel's separate FP operations. No native x86 execution.
Ignored evidence is in `target/stack-field-evidence/stage1`, with exact argv,
outputs, activation counts and SHA-256 hashes in `commands.json`/`result.json`.

## Mixed-width initialization (2026-10-21)

The existing Full-only admission now handles small-memory's i64 zero store at
`acc + 16` together with i32 counter updates. It promotes the counter recurrence
alongside the two i64 accumulators. No pass registration, Fast pipeline, allocator
or libc-name rules change. Dynamic/nonuniform wider initializers remain outside
this subset. Promotion after an ordinary escaping `memset` still requires the
region alias/observer proof described above; no loads or writebacks are moved
across such a call by this change.

`tests/stack_promote.rs` exercises all narrower integer widths and byte offsets
inside i16/i32/i64 splat stores, five fill bytes, loop and zero-trip paths,
independent subfields, loop-local resets, retained neighboring byte observations,
source locations, partial overlaps, unknown/nonuniform initialization, escaping
calls and frame/debug/atomic barriers. The native validator now also constructs
an i64-initialized/i32-counter variant of `private.keel`, verifies both named-pass
and Full activation, emits ARM64 and x86-64 assembly/objects, and applies the same
10,401-case unsigned oracle to each native variant.

Validation in the isolated priority3 worktree used `target/priority3/build` and
`suite/fast_opt_locked.py` with the shared `/tmp/keel-perf-20261019.lock`:

- The complete `keel-opt` suite passed, followed by all 16 final focused tests.
- All eight reduced native variants passed 10,401 oracle cases each. Mixed-width
  promotion reduced seven loads/six stores to one input load/no stores.
- Fresh Full C compilation of the retained explicit-builtin `intrinsic.c`
  produced one input load/no stores in `k_stackobj`; its whole-driver output
  matched fresh Clang O2 across 795 lines. Both target object routes passed.
- Ordinary escaping-memset corpus field accesses remained intact. Native x86
  execution and performance measurements were not performed.

Exact commands, statuses, outputs and artifact hashes are retained locally in
`target/priority3/native/result.json`, `commands.json`, `intrinsic-result.json`,
`intrinsic-commands.json`, and `target/priority3/lock-events.jsonl`.
