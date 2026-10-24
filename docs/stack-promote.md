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

## Mixed-width initialization

Admission also handles `small-memory`'s i64 zero store at `acc + 16` together
with i32 counter updates, promoting the counter recurrence alongside two i64
accumulators. Dynamic or nonuniform wider initializers remain outside this
subset.

## Limitation: slots passed to `memset`

A slot whose address is passed to an ordinary declared `memset` call escapes,
so the pass leaves its field loads and stores intact. `call-contracts` has no
capture/alias contract that proves such a use safe. For example, the
`k_stackobj` corpus kernel keeps its six field loads and three stores (plus the
input load).

LLVM, given the same IR with the callee declared `nobuiltin` and without
`nocapture`/`noalias`, keeps `bl _memset` and the initial field reads, then
promotes and vectorizes the recurrence. That reflects LLVM's stronger
allocation-identity/alias analysis and loop memory promotion; it does not prove
Keel may assume the ordinary call never captures the address. Region promotion
past such a call would need proof that later input accesses cannot alias the
escaped field, that no intervening observer can inspect captured memory, and
correct entry reads, exit writes and first-fault behavior on every region exit.
This pass assumes none of those. See `stack-cache` below for the opt-in
alternative.

## Tests

```sh
cargo test -p keel-opt --test stack_promote
```

`tests/stack_promote.rs` covers all narrower integer widths and byte offsets
inside i16/i32/i64 splat stores, several fill bytes, loop and zero-trip paths,
independent subfields, loop-local resets, retained neighboring byte
observations, source locations, partial overlaps, unknown/nonuniform
initialization, escaping calls, exceptional/nonlocal control and
frame/debug/atomic barriers.

`tests/stack_promote/private.keel` is a reduced fixture with exact private
initialization stores; promotion removes all six field loads and six field
stores, leaving its one input load and two conditional branches. It is not
evidence of promotion in real corpora.

`crates/keel-opt/tests/stack_promote/validate.py` is a serial native validator
(no timing). It emits ARM64 Mach-O and x86-64 ELF objects and assembly for the
reduced-fixture variants (including an i64-initialized/i32-counter variant),
runs 10,401 unsigned-C oracle cases per native ARM variant, and compares
Fast+promotion and Full+promotion whole-driver output against Clang O2
(`-ffp-contract=off`, 795 lines). It takes the CLI binary (`--keel`), an output
directory (`--out`), a directory holding a captured corpus `raw.keel`
(`--investigation`), and a command wrapper plus log file (`--lock-wrapper`,
`--lock-log`); every subprocess runs as `python3 WRAPPER LOG argv...`. `--if-convert` adds the `if-convert` stage (see
[if-convert.md](if-convert.md)). x86 output is checked statically only.

## Post-capture read caching (opt-in)

`stack-cache` complements private-slot promotion for an object passed to an
ordinary initializer. It seeds exact integer/pointer fields from memory after
the last entry-block effect and carries those values through bounded loop SSA.
It does not assume `memset` semantics, initialization to zero, or `nocapture`.
Keel's stable uninitialized-memory rule permits the seed read; its fixed-slot
bounds prove that this additional read cannot fault.

Every original store stays at its original location. An arbitrary read through
a pointer returned by the initializer can therefore still observe those writes.
The suffix rejects unknown writes, calls, volatile/atomic operations, and other
memory barriers. Overlapping field types stay in memory. Debug homes, frame
observers, exception continuations, dynamic stack operations and returns-twice
calls disable the transformation. No edge may return to the entry block.

Eligibility is bounded at 64 blocks, 2,048 instructions and eight cached fields;
address discovery makes at most eight scans. Only fields read inside a natural
loop are selected. The pass is available explicitly and is not part of the
Full or Fast schedules (`cargo test -p keel-opt --test stack_cache`).
