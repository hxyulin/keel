# Experimental null pointer rematerialization

With `experimental-regalloc`, `--remat null --regalloc fast` (or `full`)
substitutes an immutable zero pointer recipe for eligible existing scalar spill
homes after the ordinary allocator runs. The library switch is
`RematerializingAllocator::null_pointers`. Set constants, stack_addresses and
prefer_spilling to false. Combining any of those switches with null_pointers,
or using Local/Split through the wrapper, returns the original base
allocation. The CLI rejects those two allocator tiers for null mode.

This accepts only a live canonical result0 of `inttoptr.ptr` whose input is a
live result0 of `iconst.i64 0`, with at most eight input aliases, all typed I64.
Result aliases, cycles, out-of-range values, removed definitions, other widths,
nonzero literals and computed zeros are ineligible. Functions with debug_vars
retain their original null homes. Existing constraints, both Reuse endpoints,
sparse edits, low-part-only substitution, shared/vector homes and fixed stack
compaction safeguards remain in force. Register assignments and pressure victim
selection are unchanged by null-only mode.

x86 uses `MOV r32,0`, preserving flags and zero extending to pointer width.
AArch64 uses non-NZCV-setting 64-bit zero materialization. The exact allocated
Remat conversion definition is skipped. Existing transfer scratch registers and
parallel-copy ordering handle all uses, including stack destinations. Null
recipe materializations use the existing backend-profile RematConstants counter.
The supported pointer contract is limited to the existing 64-bit targets.

The default build, default allocator and constants/stack/all/pressure modes do
not enable null rematerialization. No frontend, IR semantics, selector, ABI,
source-visible memory operation, debug format, pool or dependency changes are
part of this policy.

## Tests

```sh
cargo test -p keel-codegen --features experimental-regalloc --test null_rematerialization
```

- Recognition tests cover malformed graphs before any unbounded resolve,
  alias-depth boundaries 0/1/8/9, canonical ownership, exact I64 zero, wrong
  widths, nonzero literals, conversion expressions and removed definitions.
  IntToPtr requires I64 on both supported 64-bit targets, so width-normalized
  zero is exactly the literal zero.
- Allocation tests cover mixed-mode and unsupported-tier fallback, existing
  spill/register choices, constraints and both Reuse endpoints, debug metadata,
  sparse edits, shared/vector homes and fixed stack compaction suppression.
- Cross-target object tests compare structured objects with text assembly,
  check mode-off parity and flag-neutral reloads, and independently interpret
  forced parallel-copy cycles and null-to-stack transfers.
- Native ARM fixtures cover clobbers, selects, comparisons, ABI register/stack
  arguments, pointer returns, loops, setjmp/longjmp, stackrestore, and Invoke's
  normal and caught exceptional continuations, with real and empty pools.
- The existing symbolic transfer checker also checks null-only allocations
  across the fixture corpus. CLI tests cover feature gating, Fast/Full-only
  admission, and unchanged legacy rematerialization mode bits.

The Invoke fixture declares its AArch64 target and its object matrix runs only
there, because x86 does not support Invoke lowering. The forced-copy fixture
uses x9/x10 on AArch64 (where allocatable caller-saved registers begin) and
rsi/rdi on x86. It interprets the exact emitted straight-line edge moves on
both targets: distinct input tokens must swap, the null stack cell must become
zero, and the zero/store pair must preserve a separately live cycle temporary.
The interpretation is bounded to this fixture's move/load/store operations and
rejects unknown instructions; it is not a general emulator.

No runtime or compilation-performance measurement is claimed. x86 object
correctness is checked; x86 native execution is not covered.

## Integration boundaries

Null-only mode deliberately recognizes ordinary Fast/Full tiers only. A new
experimental incoming-affinity tier must remain outside that admission test.
New RematerializingAllocator literals need an explicit null_pointers field;
legacy callers should set it to false. None/Fast/Full defaults and the
three-round Full IR schedule are unchanged.
