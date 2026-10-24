# wasm32 basic optimization profile: design and safety audit

This page explains why the opt-in `--wasm-opt basic` profile (see the
[CLI reference](wasm-cli.md)) runs exactly four existing passes, why other passes
are excluded, and which invariants the driver relies on. It is a source audit of
the optimizer against the wasm32 [memory semantics](wasm-memory-backend.md) and the
[runtime contract](../crates/keel-wasm/README.md), not a proof for every IR graph.

Native [ISA](userspace-isa-status.md), [C intrinsic](c-x86-intrinsics.md) and
[GNU vector/mode](gnu-vector-mode.md) contracts are independent: native SIMD
fallbacks, vendor headers, FP, aggregate ABI and CPU features do not become Wasm
capabilities.

## The allowlist

One fixed sequence of function passes, run once:

```text
const-fold, simplify-cfg, phi-simplify, dce
```

No iteration to a fixed point and no implicit native tier. None of these builtins
requires canonical input form or module hooks; their identities are in
[`keel-opt/src/lib.rs`](../crates/keel-opt/src/lib.rs). No memory scheduling,
pointer algebra, frame or slot deletion, data rewriting, inlining or signature
rewriting is allowed.

- [const-fold](../crates/keel-opt/src/const_fold.rs) folds supported integer
  constants and constant selects. Division by zero and signed division overflow are
  left in place; signed remainder MIN/-1 folds to zero, matching MVP. Pointer
  operands never enter integer constant lookup (`Type::Ptr.is_int()` is false),
  pointer conversions are not folded, and `ptradd` is not folded. FP folding code
  in the same file is irrelevant because admission rejects FP first.
- [simplify-cfg](../crates/keel-opt/src/simplify_cfg.rs) folds known branches and
  switches, bypasses jump-only blocks with simultaneous argument substitution,
  removes unreachable blocks and merges single-edge jump chains, preserving
  instruction order. Equal-target folding compares edge arguments as well as
  targets. Anything it removes has already passed admission on the original module.
- [phi-simplify](../crates/keel-opt/src/phi_simplify.rs) substitutes only an
  anchored, strictly dominating incoming value after examining all incoming edges,
  and leaves entry parameters intact. It makes no pointer-width or memory proof.
- [dce](../crates/keel-opt/src/dce.rs) roots pinned instructions and terminators and
  removes unused pure values and unused non-entry block parameters.
  [`InstData::is_pinned` and `BinaryOp::can_trap`](../crates/keel-ir/src/inst.rs)
  keep divisions, remainders, loads, stores and calls. The one exception removes a
  dead ordinary load only when it is `notrap && !volatile`; that is the producer's
  promise, and the optimizer never infers or adds it. Dead volatile loads (including
  `volatile notrap`) stay. A `notrap` load that is actually out of bounds violates
  its producer contract, so its trap is not behavior DCE must preserve.

The optimizer requires valid input: malformed references must be rejected before a
pass indexes instruction or edge tables.

## Excluded passes

- [canonicalize](../crates/keel-opt/src/canonicalize.rs) is plausible but excluded.
  Besides alias and operand-order canonicalization it recognizes ORed complementary
  constant shifts as `rotr` (guarded to nonzero in-range counts on i32/i64, which
  the backend supports). No unsafe rule was found; its larger surface simply needs
  separate evidence.
- [instcombine](../crates/keel-opt/src/instcombine.rs) is excluded. Requiring
  canonical input triggers hidden canonicalization in the generic manager, and it
  performs quotient/remainder reuse, reassociation and constant-division reduction.
  For example, `udiv i64 x, 3` is rewritten by
  [`constant_division::reduce`](../crates/keel-opt/src/instcombine/constant_division.rs)
  into `umulhi`, which the Wasm backend does not support, so valid input would
  become unsupported. (This is a capability failure, not a miscompile.)
- [stack-promote](../crates/keel-opt/src/stack_promote.rs) (including its large
  variant) and [stack-forward](../crates/keel-opt/src/stack_forward.rs) are excluded.
  They size fields with `ty.bytes().unwrap_or(8)`, but
  [`Type::bytes`](../crates/keel-ir/src/types.rs) returns `None` for pointers, which
  are four bytes on wasm32. A slot holding a pointer at offset 0 and an i32 at
  offset 4 would be modelled as overlapping, and a four-byte pointer slot might be
  rejected as too small. No wrong-code bug was reproduced (conservative refusal can
  hide the mismatch), but the analysis model is wrong for wasm32. Removing private
  stores or changing slot tables would also need review of memory observability and
  stack-exhaustion behavior.
- SCCP, GVN, code motion, load/memory forwarding, small-memory lowering, call
  contracts, inlining, loop and vector passes are out of scope.

## Why not reuse the existing pipelines

`OptimizationTier::Basic` is `simplify-cfg, canonicalize, instcombine,
call-contracts, simplify-cfg, dce`; Fast adds more; None's empty list is not a Wasm
contract. `run_pipeline_with_context_and_options` runs module-level tracking before
function dispatch and inserts `canonicalize::run` when a pass requires canonical
form, and the report path duplicates that. Custom pass lists and the public registry
are therefore not an allowlist, and `VerificationPolicy::Disabled` cannot suppress
canonical restoration.

The per-pass `verify_after` hook calls the function verifier, which has no module
data. What is needed is whole-module verification with the correct layout:
[`verify_module`](../crates/keel-ir/src/verifier.rs) uses NATIVE64, while
`verify_module_with_layout(..., DataLayout::WASM32)` checks pointer initializer
alignment, cursors and addends under the wasm32 model.

## Entry points and driver order

The optimizer entry point is:

```rust,ignore
pub fn optimize_wasm32_basic(module: &mut Module) -> Result<(), OptError>;
```

It asserts caller-supplied WASM32 storage, verifies the whole module with
`DataLayout::WASM32` before mutation and after each pass, and calls only the four
builtins directly. It takes no pass names, tiers, vector targets or module hooks.
`keel-opt` does not depend on `keel-wasm`; backend admission is the caller's job.

The backend admission check is
`keel_wasm::validate_wasm32_runtime_module(&Module, &Wasm32RuntimeOptions)`. It
shares its checks with runtime emission (rather than duplicating an opcode list)
and encodes nothing.

The driver runs these steps in order:

1. Parse options, route the target and load C or raw IR. C keeps its
   unsupported-before-pruning frontend check; raw IR cannot relabel artifacts.
2. Run backend admission on the **original** module with the requested exports and
   memory options, before any pruning.
3. Run `optimize_wasm32_basic` on a clone.
4. Emit with the normal runtime emitter, which repeats legality checks and
   validates the MVP bytes. Original admission does not certify transformed code,
   and final legality does not replace original admission (the `udiv` by 3 case
   above shows why both are needed).
5. Publish atomically; failures leave the destination untouched.

If input verification fails, the module is unchanged; a later failure may leave
earlier passes' changes applied, which is why the CLI optimizes a clone.

## Counterexamples to avoid

These are tempting transformations that would be wrong on wasm32. The four admitted
passes do not perform them; current instcombine has no general
`inttoptr`/`ptrtoint` cancellation either.

- **Pointer round trips are not identities.**
  `ptrtoint.i64(inttoptr.ptr(iconst.i64 4294967297))` returns 1, and -1 becomes
  4294967295. The backend emits `i32.wrap_i64` then `i64.extend_i32_u`.
- **Do not fold wrapping offsets into memargs.** `inttoptr(-1)`, `ptradd 1`,
  `load.i8` reads byte zero. A load from `0xffffffff` with memarg offset 1 traps
  even with maximum memory, because the effective-address addition does not wrap.
  The backend always uses offset zero. Reassociating constant offsets modulo 2^32 is
  fine; absorbing them into the load is not.
- **Keep faults and their order.** Store 53 at address 0, evaluate an unused
  `sdiv.i32 1, 0`, return 7: correct execution traps with the store visible;
  removing the divide returns 7. An unused ordinary load past the end of memory
  must also trap. Do not move either across earlier stores or calls. Signed MIN/-1
  division traps; signed MIN % -1 is zero.

A hand-encoded MVP module confirmed these engine behaviors in Node:

```text
roundtrip(4294967297) = 1; cancelled(4294967297) = 4294967297
wrapped byte load = 42; absorbed-offset byte load = trap
dead divide = trap, prior memory[0] = 53; removed divide = returns 7
dead ordinary OOB load = trap
```

Layout-sensitive verifier cases: `data @x = align 4 { i32 7, ptr @x + 8 }` has its
pointer at offset 4 and total size 8 on wasm32; native verification rejects its
alignment, and function-only verification cannot see it. A target containing `ptr`
plus i32 is 8 bytes on wasm32 and 12 natively, so an addend of 12 passes the native
bound but is invalid on wasm32.

Admission must still reject, before and after requesting the profile: an unexported
FP function, dead FP/vector/intrinsic values, an unsupported unary in a
constant-false branch, unused unresolved declarations, unused unsupported static
data and unused over-aligned frames.

## Test requirements

The implementation's tests (`cargo test -p keel-opt --test wasm32_basic` and
`cargo test -p keel --test wasm_output`, corpus in `suite/wasm-basic`) cover:

- Differential plain/basic execution against independent Node integer oracles:
  wrapping and extreme arithmetic, mixed-width shift counts, constant branches,
  switches, loops, phi cycles and duplicate edges, direct calls and recursion, plus
  a witness showing the optimizer actually reduced something.
- Byte-exact memory and canaries around pointer and narrow accesses, unaligned
  operations, high-bit, negative and wrapping offsets, and static pointer
  initializers. Ordered calls and volatile accesses, dead faulting loads, all
  division trap families, and the dead-`notrap` versus `volatile notrap` distinction.
- Static slot tables and frame sizes stay intact even when a `StackAddr` dies, since
  removing a slot could remove an allocation trap. Recursion, multiple return
  blocks and repeated calls keep restoring SP; stack exhaustion traps before any
  store.
- The whole-input rejection cases above, native and Portable CLI controls, and
  unchanged destinations on failure.
