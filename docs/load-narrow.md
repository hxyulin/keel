# Legal ordinary load narrowing: bounded source checkpoint

This implements only an opt-in private-stack subset. It does **not** activate on
the actual C compiler bridge. The initial source-only checkpoint was followed by
an explicit root-granted MacHost correctness lease. The validation results below
come from that lease; MacHost was explicitly released after all owned jobs exited.
Root owns final acceptance and integration. No timing or LinuxHost access occurred.

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
commit adds only `mod load_narrow;` and `&load_narrow::LoadNarrow` to `PASSES` in
shared `lib.rs`; it changes no indexed builtins or default pipelines. Root owns
reconciliation with the sign-extension worker. `cargo test -p keel-opt --lib
load_narrow` discovers all eleven dedicated tests through the module's explicit
`#[path]`; no new Cargo test target is needed.

Source tests cover offsets 0/1/8/16, commuted masks, identical alignment/flags,
load/mask source lines and instruction order, output verification, idempotence,
missing `notrap`, unknown pointers/target, volatile/atomic/lifetime effects,
out-of-bounds original widths, shared loads, wrong masks/types, escaped addresses,
debug homes and direct in-memory aliases. A retained bridge-IR test asserts
unsupported behavior on both explicit targets.

After independent source review and an explicit root host lease, acceptance must
include compiling the separate registration, running these and relevant optimizer
tests, verifying both-target text and objects, checking the actual C bridge stays
unsupported, and independent Clang native comparison of supported stack cases
on each leased target. Include edge bytes 0/127/128/255, offsets, intervening
writes, selected/debug modes, and aliases. Protected-boundary native faults are
only relevant to the rejected generic potentially-faulting load: confirm the
original wide access remains; never feed a false `notrap` promise to such a test.
No performance claim or default activation follows from correctness acceptance.

## Retained correctness validation

Source `1690c16126ce9af662b5226ff2a677da72100cff` contains the rule and unit
tests. Separate registration `d50d422e61d461bce91a5f2e4b24a5bc2c69bce8`
was followed by the diagnostic-only golden update
`1c2cab1ff7859970fd26ff94cb1d770fa84c9571`. Dedicated object/native fixtures
and runner were committed as `37bfe7f3a46532eb6f65a5bae4e922001bdee160`.
Production code did not change after registration.

An initially cold private target directory and serial `-j2` builds were used:

- Focused load-narrow unit tests: 11 passed, including direct aliases, flags,
  positions, locations, full allocation bounds, and fallback cases.
- Full `keel-ir` / `keel-opt` gates: 299 passed after the expected unknown-pass
  diagnostic golden was updated to include the registered name. The original
  failed run and exact mismatch are retained; no optimizer result was blessed.
- Full `keel` CLI gate: 46 passed. Existing ignored tests remained ignored.
- `tests/load_narrow/validate.py`: five known private-stack functions activate
  from load.i64 to load.i8/uext before emission on both targets. The functions
  cover offsets 0, 1, 8, 16 and a store between the load and mask. Missing-notrap,
  debugger-home and generic-pointer functions remain unchanged. The actual C
  bridge's entire optimized IR remains identical with or without the new pass.
- Both-target text, direct objects and independent Clang-assembled objects pass
  in the baseline/full selection by fast/full allocation matrix. The full/full
  configurations also use `-g`. Native object headers are checked for each target;
  emitted assembly contains the expected byte-access instructions.
- MacHost ARM64 native: all eight direct/assembled routes passed, each with 8,393
  direct comparisons against a separately compiled Clang `-O0 -fno-lto` oracle
  and two protected-boundary fault checks. All 256 low bytes with four high-bit
  patterns, unaligned offsets, intervening writes, debugger fallback, bounded C
  bridge inputs, and null-pointer zero-count bridge calls are covered. The
  generic wide load and the independent unaligned-wide oracle both fault when
  their eight-byte access crosses into a protected page; no false notrap promise
  is used. Total: 67,144 comparisons and 16 expected faults.

These are finite correctness checks. Native x86 was **not run**; both-target
object checks do not substitute for x86 execution. No generated-loop/application
timing, benchmark, generic activation, or default expansion is claimed.

The CLI binary was built at the registration checkpoint and is unchanged by the
later test-only commits. Its SHA-256 is
`e279548ca927d77c225909a3cb3a81e46a22f8f8a383022d153c77799a45ccdd`.
Exact source trees, tool identities, argv, timestamps, test/native hashes, original
failure and output streams are retained under ROOT
`suite/reports/pattern-opt-20261017/load-narrow`. The durable coordination record
is ROOT `.git/keel-coordination/pattern-opt-load-narrow-20261017.json`. Repeating
the native runner requires a new explicit correctness lease and a fresh output
directory; it is intentionally excluded from automatic Cargo test execution.
