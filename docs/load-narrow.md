# Legal ordinary load narrowing

`load-narrow` is an opt-in pass that rewrites `and.i64(load.i64 notrap addr, 255)`
into a byte load plus zero extension. It implements only a private-stack subset
and does **not** activate on generic pointer code produced by the C frontend.
No default pipeline runs it.

## Contracts relied on

- `crates/keel-ir/src/memory.rs`: default loads may fault. `notrap` is an existing
  producer guarantee that this particular access cannot fault; it grants neither
  alias freedom nor permission to hoist across writes or lifetime changes.
- `docs/design.md`, Memory: operations follow CFG order; conflicting unsynchronized
  ordinary accesses are outside defined IR behavior; externally observable memory
  requires volatile. Volatile operations must execute as written.
- `InstData::memory_effects` and `may_trap` in `crates/keel-ir/src/inst.rs` separately
  classify effects and faults. All atomics, calls, assembly and lifetime barriers
  are excluded here. All intrinsics and non-load/store memory effects are excluded.
- `Function::restrict_params` explicitly supplies no dereferenceability, nonnull,
  alignment or immutable-memory guarantee. Generic pointers have no allocation
  extent in their type. There is no documented permission to suppress faults on
  an unannotated wide access merely because only its low bits are used.
- Fixed stack slots declare size and alignment. `stack-forward` already uses
  constant in-bounds paths and escape rejection for private stack objects, with
  debugger homes excluded. This pass uses its own deliberately smaller proof;
  it does not alter or depend on the forwarding implementation.
- `Function::inst_data_mut` requires unchanged result types. The new byte load
  must therefore be inserted, rather than changing the original result signature.
- `debug.rs` describes stack homes as externally editable. Functions with any
  debug variables are excluded; ordinary source locations remain supported.
- Existing narrow-load extension codegen cleanup leaves memory operations
  unchanged. Canonicalization only resolves operands and structural order at this
  base. Neither facility proves that a wide memory access may become a byte access.

## Exact rule and proof boundary

The named pass is `load-narrow`. Its target-independent `run` and Portable target
entry points return false. Explicit X86Sse2 and Aarch64Neon targets denote the
currently supported little-endian x86-64/AArch64 implementations. No target or
endianness is inferred from the machine running the optimizer. A future big-endian
target must not reuse these target variants without changing this contract.

The function must have no debug homes, volatile accesses, calls, atomics, inline
assembly, intrinsics or other memory effects beyond plain loads/stores. Every
memory address must be a fixed stack address or a checked constant nonnegative
`ptradd` chain within that allocation. Every original memory access, including
the original eight-byte load, must fit fully. Every use of a known stack address
must be one of those checked derivations or an ordinary access; conversions,
passing, storing or returning the address reject the function. Thus no candidate
can access a device, unknown allocation, escaped object or concurrently shared
storage under the documented ordinary-memory model.

For `and.i64(load.i64 notrap addr, 255)` (either operand order), require one
resolved operand use of the load and owning, live load/mask results. Count all
layout operands after alias resolution, conservatively including dead aliased
instructions still present in layout. Never publish a resolved alias key from a
dead producer. The load must already have `notrap`; a known slot alone does not
cause this pass to infer or add that flag. No generic validity facts are invented.

Replace the wide load with `load.i8` at exactly its original execution position,
with the identical address, flags, alignment and source location. Replace the
mask's data with `uext.i64` of the byte, keeping the mask result handle, location
and position. Remove the now-unused wide instruction. Other instructions retain
their positions and identities. There is no speculation, reordering or load
elimination. Only byte zero is accessed, so little-endian low-byte equality gives
`zext(load8(addr)) == load64(addr) & 255`. The explicit original `notrap` contract
and ordinary private allocation exclude a required wide-only fault. Nothing is
claimed for potentially faulting wide loads. Writes between load and mask remain
between the byte load and extension, so the extension observes the loaded SSA
value rather than reading memory again.

## Why generic pointers are not narrowed

A typical optimized C kernel computes an index and scaled address from a generic
pointer, then uses `load.i64` and `and ..., 255`. It supplies neither `notrap`
nor a fixed allocation. Caller facts such as "the buffer holds 256 valid words"
are absent from the IR function's contract, and C's source validity rules or
Clang's optimization choices do not establish a Keel IR contract. Such code is
left unchanged. Activating it would require an existing valid-access proof or a
new IR contract; the pass does not add metadata, infer `notrap`, infer bounds,
or weaken fault preservation.

## Tests

```sh
cargo test -p keel-opt --lib load_narrow
```

The tests live in `crates/keel-opt/tests/load_narrow/mod.rs`, included as unit
tests via `#[path]` so they can exercise direct in-memory alias states. They
cover offsets 0/1/8/16, commuted masks, identical alignment/flags, load/mask
source lines and instruction order, output verification, idempotence, missing
`notrap`, unknown pointers/target, volatile/atomic/lifetime effects,
out-of-bounds original widths, shared loads, wrong masks/types, escaped
addresses, debug homes and direct aliases. A captured C-frontend IR test asserts
the generic case stays unsupported on both explicit targets.

`crates/keel-opt/tests/load_narrow/validate.py --keel <binary> --output <dir>`
is a native runner, not part of `cargo test`. It checks that five private-stack
functions (offsets 0, 1, 8, 16, and a store between the load and mask) become
`load.i8`/`uext` on both targets, that missing-notrap, debugger-home and
generic-pointer functions stay unchanged, and that the captured C kernel's
optimized IR is identical with or without the pass. It checks both-target text
and objects across the baseline/full selection by Fast/Full allocation matrix
(full/full also with `-g`), and on arm64 compares each route against a Clang
`-O0` oracle over all 256 low bytes with four high-bit patterns, unaligned
offsets, intervening writes and debugger fallback. The generic wide load
faults as expected when its eight-byte access crosses into a protected page.
x86-64 native execution is not covered, and no performance claim is made.
