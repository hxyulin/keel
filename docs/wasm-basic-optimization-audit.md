# Bounded wasm32 basic optimization audit

Audited production base: `d8aeff5d`, 2026-10-24. This document records a
read-only source audit and independent engine reducers, not implementation or
approval of a general optimizer. Only this document belongs to this worker.
No local or ancestor AGENTS.md was found; the supplied shared-cargo instructions
apply. Parent owns independent review, integration and cleanup.

Read with [CLI input/output contract](wasm-cli.md), [freestanding C
contract](wasm-c-freestanding.md), [layout plan](wasm32-layout-plan.md),
[runtime README](../crates/keel-wasm/README.md), and [memory semantics](wasm-memory-backend.md).
Those base documents correctly describe unoptimized output at the audited base;
this proposal does not retroactively enable their rejected optimization flags.
Native [ISA](userspace-isa-status.md), [C intrinsic](c-x86-intrinsics.md), and
[GNU vector/mode](gnu-vector-mode.md) contracts remain independent. Native
SIMD fallbacks, vendor headers, FP, aggregate ABI and CPU feature promises do
not become Wasm capabilities.

## Recommended first allowlist

Use exactly one fixed function-pass sequence:

```text
const-fold, simplify-cfg, phi-simplify, dce
```

Do not iterate to convergence or invoke a native tier implicitly. A later fixed
second round needs its own evidence; it is not necessary for this first seam.
These existing builtins do not require canonical entry form or implement module
hooks. Their identities are in [BUILTINS and Pass](../crates/keel-opt/src/lib.rs).
No memory scheduling, pointer-value algebra, frame/slot deletion, module data
rewriting, inlining or signature rewriting is admitted.

- [const-fold](../crates/keel-opt/src/const_fold.rs), `run`, `fold`, `binary`,
  `convert`: folds supported integer constants in place and constant selects.
  Division by zero and signed division overflow stay unfurled. Signed remainder
  MIN/-1 is zero, consistent with MVP. Its pointer operands cannot enter the
  integer constant lookup (`Type::Ptr.is_int()` is false); pointer conversions
  are not folded, and `Ptradd` returns None. FP code in the same file is not
  permission to accept FP: original-input admission must reject it first.
- [simplify-cfg](../crates/keel-opt/src/simplify_cfg.rs), `run`: folds known
  branches/switches, bypasses jump-only blocks with simultaneous argument
  substitution, removes unreachable blocks and merges single-edge jump chains.
  Merging preserves instruction sequence. Equal-target folding checks arguments
  as well as target blocks. Removed unreachable operations must already have
  passed original-input admission, including unsupported operations.
- [phi-simplify](../crates/keel-opt/src/phi_simplify.rs), `replacement`: substitutes
  only an anchored, strictly dominating incoming value, examines all incoming
  edges, and leaves entry ABI parameters intact. It makes no pointer-width or
  memory proof. Test duplicate edges, cycles and escaping block parameters.
- [dce](../crates/keel-opt/src/dce.rs), `run`: roots pinned instructions and
  terminators, and removes unused pure values and unused non-entry parameters.
  [InstData::is_pinned and BinaryOp::can_trap](../crates/keel-ir/src/inst.rs)
  keep divides, remainders, loads, stores and calls. The existing explicit
  exception removes a dead ordinary load only when `notrap && !volatile`.
  This producer promise is accepted; the optimizer must not infer or add it.
  A live promised load remains pinned. Dead volatile loads remain roots,
  including volatile+notrap. An OOB promised load violates its producer contract;
  its emitted engine trap is not evidence that DCE must preserve that invalid
  input's behavior. Exact memory preservation forbids widened reads and lost
  faulting/volatile observations, while permitting this explicit dead-read case.

This is a bounded source argument plus required admission tests, not a proof
for every IR graph. In particular the optimizer requires valid input; malformed
references must fail before a pass indexes instruction/edge tables.

## Classify the adjacent candidates separately

[canonicalize](../crates/keel-opt/src/canonicalize.rs) is a plausible separate
candidate, excluded from the minimal pipeline. Besides aliases, commutative
ordering and comparison reversal, it recognizes complementary full-width
constant shifts ORed together as `Rotr`. It guards nonzero in-range counts and
I32/I64 width; [backend binary](../crates/keel-wasm/src/lib.rs) supports both
rotates in MVP. There is no finding that this rotate rule itself is unsafe.
Its larger semantic surface still requires its own admission evidence.

[instcombine](../crates/keel-opt/src/instcombine.rs) is excluded. Its canonical
requirement triggers hidden restoration in a generic manager; it also runs
quotient/remainder reuse, integer reassociation, and constant-division reduction.
A concrete capability reducer is `udiv i64_parameter, iconst.i64 3`:
[constant_division::reduce](../crates/keel-opt/src/instcombine/constant_division.rs)
emits `Umulhi`, absent from the base backend's binary allowlist. Valid original
Wasm input therefore becomes unsupported. This is a capability failure, not
an arithmetic miscompile claim. Neither `fast-local` nor a Portable vector
context restricts this whole scalar rewrite inventory to MVP.

[stack-promote](../crates/keel-opt/src/stack_promote.rs), including its large
variant, and [stack-forward](../crates/keel-opt/src/stack_forward.rs) are excluded.
Their private-field widths use `ty.bytes().unwrap_or(8)`, whereas
[Type::bytes](../crates/keel-ir/src/types.rs) deliberately returns None for Ptr.
Wasm pointer storage is four bytes. A slot containing `ptr` at offset zero and
i32 at offset four has disjoint Wasm fields, but the native helper models the
pointer as overlapping the integer. A four-byte pointer slot may instead be
rejected as too small. These show a mismatched analysis model, not a reproduced
wrong-code bug in either existing pass: conservative refusal can hide it.
Do not claim those passes are admitted just because integer-only cases pass.
Deleting private stores or changing slot/frame tables would also require a
separate review of memory observability and stack-exhaustion behavior.

SCCP, GVN, code motion, load/memory forwarding, small-memory lowering, call
contracts, inlining, loop and vector passes are outside this first allowlist.
No native pass or API needs to be removed or modified to establish this seam.

## Hidden manager paths and exact additive boundary

[OptimizationTier::Basic](../crates/keel-opt/src/lib.rs) is currently
`simplify-cfg, canonicalize, instcombine, call-contracts, simplify-cfg, dce`.
Fast also invokes fast-local and a loop reduction; None's empty list is not a
Wasm contract. `run_pipeline_with_context_and_options` invokes
`run_module_tracked` before function dispatch, and inserts `canonicalize::run`
when required. Its report path duplicates restoration/verification dispatch.
Custom passes and the public registry are therefore not a Wasm allowlist.
`VerificationPolicy::Disabled` suppresses repeated checks only; it cannot
suppress canonical restoration or make an unreviewed pass safe.

`verify_after` calls **function** `verify(func)`. It is not correct to claim
that this call alone applies native pointer layout to global initializers:
function verification has no module data. The missing boundary is explicit
whole-module layout verification. [verify_module](../crates/keel-ir/src/verifier.rs)
selects NATIVE64; `verify_module_with_layout(..., DataLayout::WASM32)` checks
pointer initializer alignment, item cursors and addends under the correct model.
Do not replace the latter with per-function checks or the native module wrapper.

Proposed additive public optimizer entry (name subject to owner agreement):

```rust,ignore
pub fn optimize_wasm32_basic(module: &mut Module) -> Result<(), OptError>;
```

It explicitly asserts caller-supplied WASM32 storage provenance, performs
whole-module WASM32 verification before mutation and after each fixed pass,
and directly dispatches only the four builtins. It never accepts arbitrary
pass names, native effort tiers, vector targets or custom module hooks. Whole
module verification also catches cross-function metadata/reference errors that
function-only checks cannot. Existing native APIs, defaults and target-free
Portable optimization retain their behavior. Runtime capability admission is
separate: keel-opt need not acquire a dependency on keel-wasm or runtime options.

The agreed backend seam is:

```rust,ignore
pub fn validate_wasm32_runtime_module(
    module: &Module,
    options: &Wasm32RuntimeOptions,
) -> Result<(), Diagnostic>;
```

It must share checks with runtime emission and encode no function bodies or
segments. At the base, [emit_wasm32_runtime_module/emit](../crates/keel-wasm/src/lib.rs)
and [Placement::new/Frame::new](../crates/keel-wasm/src/runtime.rs) jointly own
memory bounds, WASM32 module verification, every declaration/function/callee
and laid-out block/instruction, export validation, data/fixups and every static
frame. Extract or share these checks; a second hand-maintained opcode list will
drift. Optimizing and only then calling the emitter breaks original admission.

The driver ordering must be explicit:

1. Validate CLI option presence/target routing and load the existing C or raw-IR
   input route. Preserve C's whole-translation-unit unsupported-before-pruning
   frontend check; raw IR still asserts its layout and cannot relabel artifacts.
2. Run non-emitting runtime preflight on the **original** module and exact
   requested exports/memory/stack options before any pruning, even DCE alone.
3. Run the fixed Wasm optimizer, with WASM32 structural verification.
4. Run normal final runtime emission with shared feature/placement/frame checks
   and independent MVP byte validation. Original admission is not a certificate
   for transformed instructions; final legality is not a substitute for it.
5. Publish complete bytes through the existing same-directory atomic output
   replacement. Failures preserve sentinel destinations.

## Concrete negative cases and engine reducers

These are counterexamples to tempting transformations, not allegations that the
four admitted passes currently perform them. Current instcombine has no general
IntToPtr/PtrToInt cancellation rule.

Pointer normalization: `ptrtoint.i64(inttoptr.ptr(iconst.i64 4294967297))`
returns 1. Replacing it with the original i64 returns 4294967297. Likewise -1
becomes 4294967295, not -1. The backend uses `i32.wrap_i64` followed by
`i64.extend_i32_u`. Do not globally fold this round trip as a native identity.

Wrapping effective addresses: `inttoptr(-1); ptradd(pointer, i64 1); load.i8`
reads byte zero. Replacing it with a load from address 0xffffffff with memory
immediate offset 1 traps even in maximum wasm32 memory: the effective-address
addition does not wrap to zero. [Body::memarg](../crates/keel-wasm/src/lib.rs)
uses offset zero deliberately; `Ptradd` wraps its i64 offset to i32 and performs
i32 addition before the access. Constant offset reassociation that preserves
modulo-2^32 address arithmetic is not the same as offset absorption into a load.

Fault ordering: store byte 53 at address zero; evaluate unused `sdiv.i32 1,0`;
return 7. Correct execution traps with the prior store visible. Removing the
unused divide returns 7. An unused ordinary i32 load at 65536 in a one-page
memory likewise traps. Keep both, and do not move them across earlier stores or
calls. Signed MIN/-1 division must trap; signed MIN%-1 is zero. Dead promised
nonvolatile notrap loads are the expressly accepted exception described above.

A standalone, hand-encoded 202-byte MVP module exercised these paired operations
in Node v24.22.1 locally on 2026-10-24, independently of Rust/Keel emission:

```text
roundtrip(4294967297) = 1; cancelled(4294967297) = 4294967297
wrapped byte load = 42; absorbed-offset byte load = trap
dead divide = trap, prior memory[0] = 53; removed divide = returns 7
dead ordinary OOB load = trap
```

It passed `WebAssembly.validate` and assertions. Each trap instance was discarded;
reading memory after the trap only observed prior stores, with no subsequent
exported call. Source and results remain under
`/tmp/keel-wasm-basic-audit-b2e7174d/{reducers.cjs,results.json}`. This tests the
engine semantic counterexamples, not actual execution of an optimizer pass or
independent wasmparser certification. Initial harness attempts used CommonJS in
a .js module and then a reserved CommonJS `exports` binding; switching to .cjs
and renaming that binding fixed the harness before the successful run.

Global-layout reducers should include:

```text
data @x = align 4 { i32 7, ptr @x + 8 }
```

Its pointer starts at offset four and its total size is eight on WASM32. Native
module verification rejects its alignment/offset; function-only verification
cannot check it. Conversely a target containing `ptr` plus i32 is size eight
on WASM32 versus twelve natively: a fixup addend twelve can pass the native
size bound but is invalid for WASM32. These are source-derived verifier cases;
this audit did not run a Rust verifier executable.

Unsupported-before-pruning reducers must retain rejection for an unexported FP
function, dead FP/vector/intrinsic values, a constant-false branch containing
an unsupported unary, unused unresolved declarations, unused unsupported static
data and unused over-aligned frames. An optimized module that happens to contain
none of these cannot erase the original admission failure. A supported original
`udiv x,3` becoming Umulhi separately demonstrates why final capability checking
remains necessary even after original admission.

## Admission gates for the implementation and review

Maintain differential unoptimized/basic execution with independent Node integer
oracles: wrap/extrema arithmetic, shifts of mixed-width counts, constant branches,
switches, loops, phi cycles/duplicate edges, direct calls and recursion. Require
an actual reduction on a constant/CFG witness; successful compilation alone
does not demonstrate that optimization ran.

Compare every byte of relevant memory and adjacent canaries for pointer loads
and stores, narrow accesses, unaligned operations, high-bit/negative/wrapping
pointer offsets, static pointer initializer cursors and overlapping objects.
Keep calls and volatile reads/stores ordered; exercise dead ordinary faulting
loads, all divide/remainder trap families, earlier-store observations and the
explicit dead-notrap/volatile+notrap distinction without inventing promises.

Static slot tables and frame sizes must remain intact in this first seam, even
if a StackAddr value dies. Recursive address-taken locals, multiple normal-return
blocks, void returns and repeated successful exported calls must continue
restoring SP. Stack exhaustion traps before frame mutation/body stores; any
trap invalidates the instance. Removing an apparently unused slot can eliminate
an allocation trap, so slot compaction is outside this allowlist.

Keep whole-input negative gates, native/Portable CLI controls and failed-output
sentinels. Test final feature legality independently from original admission.
No imports, WASI, new FP/SIMD, C ABI expansion, runtime helper, native codegen,
linker or broad optimization framework change is authorized here.

No Rust build was needed for this read-only audit. Implementation Rust gates
must use the existing pins/features/cache and shared suite lock, for example:

```sh
python3 suite/fast_opt_locked.py /tmp/<unique>.jsonl \
  cargo test --offline --locked -j2 -p keel-opt
```

Run the implementation's focused Wasm/CLI and native control suites with the
same wrapper; use `--release` where required. Do not share CARGO_TARGET_DIR
across worktrees, install tools, delete outputs or change toolchain/cache policy.
This audit makes no timings, performance, native execution or external-host claim.
