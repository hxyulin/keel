# Legal ordinary load narrowing: bounded source checkpoint

This checkpoint implements only an opt-in private-stack subset. It does **not**
activate on the actual C compiler bridge. No build, compiler invocation, assembler,
native execution, remote access, correctness run, or timing was authorized or run.
Root must obtain independent source review and grant correctness execution before
these source tests can establish any validation result.

## Audited contracts

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

## Bridge blocker and root decision

The retained optimized `bridge_narrow_load` computes an index and scaled address
from a generic pointer, then uses `load.i64 v15` and `and v16, 255` on both targets.
It supplies neither `notrap` nor a fixed allocation. The harness's 256 valid words
and bounded masks are caller facts, absent from the IR function's contract. C's
source validity rules and Clang's optimization choices do not establish a new
Keel IR contract. The bridge must remain unchanged in this checkpoint.

Root should accept this as a bounded source checkpoint or stop the opportunity
at the semantic blocker. Activating the generic bridge requires a separately
approved source of existing valid-access proof, or a separately approved IR
contract project. Do not add metadata, annotate the bridge `notrap`, infer bounds
from the harness, or weaken fault preservation to obtain activation here.

The prior measured microkernel x86 ratio near 0.875 and ARM null concern warmed
ordinary memory whose index settles to zero. They establish no general compiler,
loop or application speedup, and supply no legality proof.

## Owned interfaces and acceptance

Owned source is `crates/keel-opt/src/load_narrow.rs`, its dedicated tests in
`crates/keel-opt/tests/load_narrow/mod.rs`, and this document. Tests are included
as unit tests to exercise direct in-memory alias states. The separate registration
patch adds only `mod load_narrow;` and `&load_narrow::LoadNarrow` to `PASSES` in
shared `lib.rs`; it changes no indexed builtins or default pipelines. Root owns
reconciliation with the sign-extension worker. Until that patch is applied, the
new module and its tests are deliberately unregistered and uncompiled.

Source tests cover offsets 0/1/8/16, commuted masks, identical alignment/flags,
load/mask source lines and instruction order, output verification, idempotence,
missing `notrap`, unknown pointers/target, volatile/atomic/lifetime effects,
out-of-bounds original widths, shared loads, wrong masks/types, escaped addresses,
debug homes and direct in-memory aliases. A retained bridge-IR test asserts
unsupported behavior on both explicit targets. These tests are **NOT RUN**.

After independent source review and an explicit root host lease, acceptance must
include compiling the separate registration, running these and relevant optimizer
tests, verifying both-target text and objects, checking the actual C bridge stays
unsupported, and independent Clang native comparison of supported stack cases
on each leased target. Include edge bytes 0/127/128/255, offsets, intervening
writes, selected/debug modes, and aliases. Protected-boundary native faults are
only relevant to the rejected generic potentially-faulting load: confirm the
original wide access remains; never feed a false `notrap` promise to such a test.
No performance claim or default activation follows from correctness acceptance.
