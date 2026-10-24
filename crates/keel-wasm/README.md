# Bounded raw-IR wasm32 runtime

This library-only checkpoint adds static data and static software-stack frames
to the reviewed [fixed-memory backend](../../docs/wasm-memory-backend.md).
The original scalar and fixed-memory APIs and options retain their contracts;
the additive integer-negation subset is documented below.
No C frontend, Target, CLI, native backend, dependency or
optimizer integration is provided. Supply raw, unoptimized IR whose storage was
explicitly produced for wasm32; the caller owns that provenance.

```rust,ignore
use keel_wasm::{
    emit_wasm32_runtime_module, Export, Options,
    Wasm32MemoryOptions, Wasm32RuntimeOptions,
};
let bytes = emit_wasm32_runtime_module(&module, &Wasm32RuntimeOptions {
    memory: Wasm32MemoryOptions {
        scalar: Options {
            exports: vec![Export { name: "entry".into(), function: "entry".into() }],
        },
        pages: 1,
        export_name: "memory".into(),
    },
    data_floor: 1024,
    stack_bottom: 32768,
    stack_top: 65536,
})?;
```

The additive function returns complete independently validated MVP bytes or the
existing Diagnostic, without mutating input or publishing partial output.
It defines exactly one owned, unshared memory with initial and maximum size both
equal to pages; names and page validation retain the fixed-memory API contract.
There are no imports, start function, implicit exports or runtime helpers.
Function parameters/results remain the existing normal i32/i64 scalar contract,
including internal calls. Pointer and narrow scalar signatures, operand ABI
descriptors and multiple result values remain rejected. Multiple normal return
instructions in different CFG blocks are supported.

## Placement and initialization

data_floor must be nonzero and at or below stack_bottom. Both stack bounds must
be multiples of 16, bottom <= top, with top within memory. The three fields are
u32 byte offsets. Thus a 65536-page memory may be encoded, but its stack top is
at most 0xfffffff0; 2^32 is not a representable configured stack pointer.
The low region [0, data_floor) is reserved only by placement. Ordinary accesses
can still reach address zero and this region; there is no protected null page.

Nonempty data objects are placed in Module.data order, starting at data_floor,
rounding each object's start to its DataInit.align (power of two up to 4096).
Every object, including BSS, must end at or below stack_bottom; placement checks
sizes, alignment and additions before emitting segments. Local and hidden
definitions are allowed. The producer supplies object alignment, including any
array placement requirement; the backend cannot infer C array identity from
DataItems. DataInit.constant remains the producer's no-store contract and
linear memory does not enforce read-only protection.

Items are packed with no implicit item padding. Both checked total size and
every item cursor use DataLayout::WASM32. Bytes and Zero retain their exact
lengths; i8/i16/i32/i64 integers use exactly 1/2/4/8 little-endian bytes.
Pointers occupy exactly four bytes. Pointer items need object alignment >=4 and
a four-byte-aligned item cursor. Every target must be defined data in this
module; a u32 addend must be <= its target's exact WASM32 size. Object-one-past
is allowed. Base plus addend is checked for u32 overflow and must be <= owned
memory size. A representable pointer equal to memory end is accepted, and a
nonzero-width dereference traps.

Active MVP segments initialize bytes, integers and resolved pointer fixups before
any exported call. Owned memory starts zeroed, so Zero items and all BSS consume
no payload buffer or segment. Sparse zero runs are never expanded. Large malformed
objects are rejected before materializing output; near-4-GiB BSS and high-bit
address tests validate and decode small binaries without instantiating them.
A fresh instance receives independent memory and fresh initializers.

Runtime emission uses the additive
keel_ir::verify_module_with_layout(module, DataLayout::WASM32) on the real
module, retaining structural, reference, type, debug and metadata verification.
The native verify_module API and both older backend entrypoints still use the
native verifier contract. The backend separately rejects RTTI, function-address
items/instructions, external data, TLS, weak/ODR, retain and explicit sections,
including unused objects. Undefined direct calls and the previous unsupported
function metadata remain rejected. No initializer replacement or verifier
filtering is used.

## Static frames and lifetime

Each function's static slots are packed in slot-table order at their requested
alignment, followed by rounding the frame size to 16. Slot alignment must be a
power of two <=16; over-aligned slots, including unused slots, are rejected.
The total frame size and offsets must fit u32 after checked arithmetic.
A private mutable i32 global holds SP, initially stack_top.

Each call saves incoming SP in its own Wasm local. Before changing the global,
it checks unsigned SP >= frame_size, computes new_SP = SP - frame_size, and
checks new_SP >= stack_bottom. Failure traps before changing SP or executing
the body. Since static data ends <= stack_bottom, successful allocation cannot
overlap it. StackAddr is new_SP plus the checked slot offset. Recursive and
nested calls have independent frames and saved pointers. Every normal Return
restores the incoming SP, including void returns and returns from different CFG
blocks. Zero-sized frames preserve SP; engine call-stack limits remain engine
behavior.

Frames are not cleared on entry or return. Memory is not protected per object,
slot or lifetime, and arbitrary pointer operations can still corrupt another
object. A pointer to a returned frame has no valid object-lifetime guarantee.
The checks protect frame allocation, not every access through a pointer.

**Any trap invalidates this prototype instance by contract. The host must discard
it.** A callee trap may leave one or more active frames allocated; saved SP is
not restored on traps. Earlier stores remain observable and are not rolled
back. The host must not attempt another call or recover SP. This applies to OOB,
division, stack-exhaustion and engine traps. No unwind/recovery/reentrancy ABI is
provided.

Dynamic allocation/VLAs, stack save/restore/introspection, all intrinsics, heap,
growth, imported memory/functions, function tables, indirect/tail calls,
C/pointer/narrow call ABI, aggregate/variadic boundaries, FP, SIMD, atomics, EH,
TLS and native integration remain unsupported. Native ABI/FP/memory and
architectural contracts remain unchanged.

## Wrapping integer negation

All three existing entrypoints (`emit_module`, `emit_wasm32_memory_module`,
`emit_wasm32_runtime_module`) additionally accept raw-IR `UnaryOp::Ineg` on
I32 and I64. The result keeps its input width. Emission uses MVP typed `0 - x`,
wrapping modulo 2^32 or 2^64; negating INT_MIN returns INT_MIN without trapping.
There is no API/options change or runtime helper. Alias reads, destination locals,
normal scalar calls and CFG edge transport retain their existing contracts.

Other unary operations, I8/I16/I128 negation, floating-point and vector operations
remain rejected, including in unexported functions. This raw-IR addition does not
establish C lowering, a C ABI or new native semantics. Pointer offsets remain i64;
the memory profiles normalize the resulting offset through the existing wasm32
`ptradd` contract.

`tests/negation.rs` independently validates MVP bytes and executes Node against
BigInt width-normalized oracles in all three profiles. It covers zero, one,
minus one, signed extrema and deterministic full-width inputs, retained/removed
aliases, live inputs/results across calls, loop phi transport and negated counts
used by mixed-width shifts. A raw-IR pointer-decrement equivalent composes i64
negation with `ptradd` and exact four-byte stores, checking every memory byte,
negative/high-bit offsets, INT_MIN and an OOB store that writes no prefix.
The runtime instance is discarded after the final trapping operation.
Explicit negative tests retain unsupported unary/width/FP family coverage.

On 2026-10-24, implementation `ba0ca420` on reviewed main `0115a0f7`
passed the full keel-wasm debug and release suites: 42 integration tests per
profile (15 scalar, 10 fixed-memory, 13 runtime, 4 negation), with no failures
or ignored tests. Locked cargo check/build passed without compiler warnings;
`cargo fmt -p keel-wasm --check` and `git diff --check` also passed.
Node v24.22.1 executed the independent BigInt oracles locally.

Every compilation used the existing shared `suite/fast_opt_locked.py` lock,
`CARGO_CACHE_LINKS=0`, `cargo`, `--offline --locked -j2 -p keel-wasm`,
existing toolchain/features and default managed cargo storage; release test adds
`--release`. Exact command records and output remain outside Git at
`/tmp/keel-wasm-ineg-50d50f78-{debug,release,check,build}.{jsonl,log}`.
No existing negative test in this base used `ineg`; the new tests explicitly
reject the other unary operations without removing existing family controls.
This is finite raw-IR evidence. Parent owns C coordination, independent review,
integration and cleanup; no native execution or performance result is claimed.

Independent review reapplied candidate `4b5b61d5` (implementation `ba0ca420`)
onto `0115a0f7` as `4bc7a603` and `ed86b60f`. No blocking defect was found
in width preflight, resolved operand reads, raw destination locals, typed MVP
subtraction, or composition with existing select, calls and CFG transport.
The BigInt oracle computes width-normalized mathematical negation independently
of the emitter; its mixed-width count expectations preserve the required low
count bits. Valid negative fixtures retain the other unary and narrow/FP
rejection boundaries. Unsupported value types can be rejected before their
operation, so those cases establish rejection rather than a specific opcode
diagnostic. No production or regression-test correction was warranted.

Fresh review debug/release runs passed all 42 integration tests per profile
with Node v24.22.1; locked cargo check/build also passed without compiler warnings.
Formatting and whitespace checks passed. Commands used the same shared lock,
offline/locked/two-job settings and disabled link caching documented above.
Review logs and command records are outside Git at
`/tmp/keel-wasm-review-dd8bc459-{debug,release,check,build}.{log,jsonl}`.
This remains finite raw-IR evidence; C integration and its expected-diagnostic
update belong to the parent and were not performed in this review checkout.

## Validation

The dedicated tests/runtime.rs uses independent wasmparser MVP validation and
existing Node validation, compilation, instantiation and execution. Tests cover
static alignment and exact initializer cursors, zero BSS, pointer fixups and
symbol accesses, global loads/stores with byte canaries, per-instance
initialization, object/memory-one-past bounds, high-bit address encodings,
sparse large BSS, nested and recursive address-taken locals, slot alignment,
all normal return paths, underflow/exhaustion canaries, oversized frame metadata
and unused unsupported contracts. Existing scalar/memory tests remain controls.

Run from the repository root with existing pins/features, shared lock and cargo
default managed storage; repeat test with --release:

```sh
python3 suite/fast_opt_locked.py /tmp/<unique>-debug.jsonl \
  cargo test --offline --locked -j2 -p keel-wasm
python3 suite/fast_opt_locked.py /tmp/<unique>-check.jsonl \
  cargo check --offline --locked -j2 -p keel-wasm
python3 suite/fast_opt_locked.py /tmp/<unique>-build.jsonl \
  cargo build --offline --locked -j2 -p keel-wasm
```

Generated binaries travel in memory to Node; logs/artifacts remain outside Git.
Finite raw-IR tests do not establish executable Wasm C, 4-GiB instantiation,
native execution or performance. Parent owns independent review and integration.

On 2026-10-24 source `4fa9d0d8` (production `c4388780`) passed locally on
Apple ARM64, rustc 1.99.1 and Node v24.22.1: all 35 backend integration tests
in both debug and release (10 runtime, 10 fixed-memory, 15 scalar), zero
failures/ignored. Both profiles independently validate MVP binaries and execute
Node fixtures. Locked cargo check and build also passed with no compiler warnings.
`cargo fmt -p keel-wasm --check` and `git diff --check` passed.

Every Rust invocation used the shared `suite/fast_opt_locked.py` lock,
`CARGO_CACHE_LINKS=0`, `cargo`, `--offline --locked -j2 -p keel-wasm`,
existing toolchain/features, and default shared cache/managed target storage.
The release gate adds `--release` to test. Exact serialized command records and
captured output are outside Git:

- `/tmp/keel-wasm-runtime-ecd33817-debug-final.{jsonl,log}`
- `/tmp/keel-wasm-runtime-ecd33817-release.{jsonl,log}`
- `/tmp/keel-wasm-runtime-ecd33817-check.{jsonl,log}`
- `/tmp/keel-wasm-runtime-ecd33817-build.{jsonl,log}`

The append-only debug command records retain initial test-fixture corrections
(an `icmp.eq` spelling and an explicit u32 literal type); the final invocations
above passed after those corrections. No production semantic correction was
needed during these runs. No 4-GiB instantiation, remote host, installation,
cache/toolchain change, output deletion, performance campaign or main
integration/cleanup was performed.

Validation temporarily uses prerequisite `8f3eab3e`, the unmodified cherry-pick
of the separately owned IR implementation `4d974a65`, on base `aa326b52`.
That prerequisite is separate from the two runtime-owned commits and is subject
to the parent's IR review/integration. The runtime-owned patch changes only
this crate. The independent runtime review is recorded below; parent integration remains separate.

## Independent runtime review

The independent review reapplied runtime commits `c4388780`, `4fa9d0d8` and
`d44fa09e`, then dropped the temporary IR prerequisite and rebased onto reviewed
main `6a1e148712617ab38a62df7d227a3614c9b2b059`. The IR production source has no
diff from `4d974a65`; `verifier.rs` has Git blob identity
`66da48503cadc0d107bfdc6788f3d2b6faeedc6f` in both. The separate IR review was
not repeated. Runtime production source has no diff from candidate `d44fa09e`.
No blocking runtime semantic defect was found in this bounded review.

Three focused regressions close coverage gaps: retained aliased StackAddr
producers must not overwrite replacement locals; pointer phi cycles with a
removed layout block must preserve distinct slots across a callee frame;
zero-frame callers must restore SP across repeated calls and permit an empty
stack region until a framed callee is actually invoked. Runtime entrypoint
negative cases additionally require diagnostics for out-of-range slot/data
handles, an unused undeclared data reference and zero initializer alignment.
These check the backend verification boundary, rather than repeat the IR suite.

Final reviewed source `c994e3eb` passed all 38 backend integration tests in
both debug and release (13 runtime, 10 memory, 15 scalar), with no failures or
ignored tests. Both profiles independently validate MVP bytes and run the Node
fixtures. Locked cargo check and build passed without compiler warnings;
`cargo fmt -p keel-wasm --check` and `git diff --check` also passed. Every Rust
gate used the prescribed shared suite lock, `CARGO_CACHE_LINKS=0`, default cargo
storage and `--offline --locked -j2 -p keel-wasm`. Command records are
`/tmp/keel-wasm-runtime-review-5172ebf9-{debug,release,check,build}.jsonl`;
release/check/build output is beside those records as `.log` files. The debug
output is retained in the review thread.

This is finite raw-IR runtime evidence. Near-4-GiB BSS/high-bit fixtures only
encode, validate and inspect small binaries; no large memory is instantiated.
Trap instances are discarded. No C ABI, FP, native regression execution,
performance result or general malformed-IR resource limit is established.
Main integration and worktree cleanup remain the parent's responsibility.

## Runtime admission without encoding

`validate_wasm32_runtime_module(&Module, &Wasm32RuntimeOptions)` returns
`Result<(), Diagnostic>` after the same memory-option, static placement,
WASM32 module verification, all-function unsupported/signature/call/export and
static-frame checks used by runtime emission. It checks unexported functions
and unused declarations/data/slots. It borrows the real input without mutation,
cloning or synthetic modules, and does not encode code/data sections or expand
sparse BSS. This is an admission check; only the emitting API independently
validates generated MVP bytes. The older scalar and fixed-memory entrypoints
retain their existing contracts. `tests/preflight.rs` checks diagnostic parity,
nonmutation and near-4-GiB sparse BSS admission without instantiation.

Independent admission-hook review reapplied candidate `82d3244e` onto main
`49cd1747` (documentation-only successor to `d8aeff5d`) as `f2e76034`.
No blocking production defect was found. The shared borrowed preparation keeps
memory options before placement, placement before actual WASM32 verification,
then declarations, exports, all function/callee/instruction contracts and every
static frame table. It allocates placement/index/frame metadata, without
cloning the module, constructing synthetic IR, encoding sections or expanding
zero/BSS storage. Final generated-byte validation remains in the emitter.

Review test commit `67cdef88` adds negative parity checks for unused unresolved,
incompatible and attribute-mismatched callee tables, unused indirect signature
tables, function retention metadata and explicit scalar call ABI descriptors.
Simultaneous independent failures pin diagnostic precedence through the frame
check. Adjacent four-byte pointer initializers demonstrate real WASM32 admission
while both older entrypoints retain their native-verifier rejection. Input
nonmutation and repeated-emission byte equality remain covered. Compared with
`d8aeff5d`, the instruction helper, body-encoder and section-encoding source
ranges and all of `src/runtime.rs` are byte-identical; SHA-256 evidence is
`/tmp/wasm-hook-review-8a97197a-invariance.json`. This is source invariance,
not a separate original-versus-candidate binary corpus comparison.

On 2026-10-24, reviewed source `67cdef88` passed all 48 integration tests in each
of debug and release (15 scalar, 10 memory, 13 runtime, 4 negation, 6 preflight),
including existing Node v24.22.1 runtime witnesses. Locked cargo check and build
with `--all-targets`, `cargo fmt -p keel-wasm --check` and `git diff --check`
passed without compiler warnings. Every compilation used the existing shared
suite lock, `CARGO_CACHE_LINKS=0`, `cargo`, `--offline --locked -j2`,
existing pins/features and default managed cache/storage. Logs and exact command
records are `/tmp/wasm-hook-review-8a97197a-{debug,release,check,build}.{log,jsonl}`.
The debug record also retains earlier review-test construction failures; the
final complete debug run passed. No production correction was required.

This review covers only the admission hook and shared preparation extraction.
It does not audit the parallel optimizer, prove future passes preserve memory
or trap behavior, validate generated bytes through the hook, instantiate
near-4-GiB memory, or establish general resource limits or performance. Existing
recursive-stack, exact-width memory and trap witnesses remain finite controls.
Parent owns independent integration, rebasing and worktree cleanup.

## Direct emission for one return block

All three emitters directly emit a function whose layout contains exactly one
block ending in ordinary `Return`. The shared instruction emitter, raw definition
locals, resolved alias reads, calls, runtime prologue and normal-return SP
restoration are unchanged. This path omits the PC local, dispatcher loop,
dispatch condition and invalid-PC traps. Runtime allocation checks and their
traps remain. No IR optimizer or local register allocator is involved.

Every other valid CFG retains the dispatcher, including jumps, branches,
switches, self-edges and extra unreachable layout blocks. Removed blocks are
absent from layout and do not prevent direct emission. A one-block self-loop
still fails the existing verifier prohibition on targeting the entry block;
this change does not bypass or relax verification. Whole-module verification,
unsupported checks, declarations/calls/exports and all frame checks precede the
choice. Unexported unsupported instructions still reject before any encoding.

`tests/direct_return.rs` decodes MVP operators and checks all three profiles,
void/i32/i64 returns, calls, recursion through another function, raw aliased
producer destinations, removed blocks, byte canaries, volatile/call order,
wasm32 pointer normalization, OOB store/division trap order and repeated normal
stack restoration. Five fallback body snapshots per profile (self-edge, jump,
branch, switch and unreachable layout block) are byte-identical to the unchanged
dispatcher captured at `82d3244e` on base `d8aeff5d`. Node executes independent
integer/BigInt and complete-memory oracles. Invalid empty functions and public
one-block self-loops retain rejection; empty modules retain acceptance.

For the minimal void and i32/i64 identity functions, decoded body bytes
(including local declarations, excluding the enclosing size prefix) changed
from 18/20/20 to 3/5/5 in scalar/fixed-memory profiles; operator counts changed
from 11/12/12 to 2/3/3. Runtime body bytes changed from 57/59/59 to 42/44/44,
operators from 32/33/33 to 23/24/24. Each omits one PC local. These are structural
encoding observations, not runtime performance evidence; no runtime timing was
performed.

On 2026-10-24, all 52 backend integration tests passed in debug and release on
Apple ARM64 with Node v24.22.1 and the existing rustc 1.99.1. Locked cargo check
with `--all-targets`, build, rustfmt and whitespace checks passed without compiler
warnings. Rust gates used the prescribed shared suite lock, `CARGO_CACHE_LINKS=0`,
`cargo`, `--offline --locked -j2 -p keel-wasm`, existing features and
default managed targets/cache; release tests added `--release`.

Evidence remains outside Git at
`/tmp/keel-wasm-straight-7b9f63be-{hook,baseline,baseline-counts,debug,release,check,build}.{jsonl,log}`.
Baseline counts intentionally ran the new direct-shape assertion against the old
dispatcher and failed after recording all counts. Command records also retain
initial fixture corrections (Module constructor, undeclared call, rejected
entry self-loop and scalar-only conversion); final debug/release runs pass.
No native implementation, ABI, feature set, FP/SIMD, imports/WASI, dependency,
CLI or optimizer policy changed. Parent owns independent review, integration
and cleanup.

## Independent direct-emission review (2026-10-24)

Emission candidate `602a807b` was reviewed independently against its original
base `d8aeff5d` plus prerequisite `82d3244e`, then rebased onto reviewed admission
main `32e96946`. The duplicate prerequisite was dropped. The admission production
source on that main is identical to `82d3244e`; the final emission production
source is byte-identical to `602a807b`. Only the adjacent README sections needed
conflict resolution, retaining both reviews. No blocking production defect or
production correction was found in this bounded emission review.

The audit checked that layout iteration and entry selection agree, verification
rejects empty/malformed functions before selection, and exactly one ordinary
terminal Return is required. It checked raw definition destinations versus
resolved alias reads, detached aliased definitions, removed layout blocks,
result stack typing, direct/recursive calls, void and integer normal returns,
static frame local indices and restoration, and retained allocation checks.
All-function unsupported admission still precedes encoding. Instruction order,
access widths, pointer normalization and the shared Return/prologue emitter are
unchanged. The fallback path retains its previous local indices and operations;
all 15 frozen fallback body comparisons pass on the reviewed base.

One additional maintained regression checks that a retained aliased signed
division producer still executes, preserves its replacement parameter on normal
return, and traps on zero and signed overflow in all three profiles. It also
checks that unused volatile i64 and ordinary i16 loads remain emitted, with an
exact-memory oracle at the last valid i64 address and the first invalid one.
Earlier stores survive the load trap and later stores do not run. Each trapping
instance is discarded. Existing witnesses cover recursive frames, underflow,
allocation exhaustion before stores, calls, pointer wrap and exact-width memory.

All 56 backend integration tests passed in both debug and release with Node
execution, followed by locked cargo check with `--all-targets` and build. Every
Rust compilation used `python3 suite/fast_opt_locked.py`, the
shared lock, `cargo`, `--offline --locked -j2 -p keel-wasm`, existing
pins/features and default managed cache/targets. Targeted rustfmt and whitespace
checks passed. Evidence stays outside Git:
`/tmp/wasm-emission-review-918699ad-final-{debug,release,check,build}.{jsonl,log}`.

The hook's independent audit is recorded separately above. This review does not
validate optimizer integration, arbitrary CFG specialization, engine performance,
native execution or recovery after traps. No imports, WASI, ABI, FP, SIMD,
native implementation or admission policy changed. Parent owns integration and
worktree cleanup.
