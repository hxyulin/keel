# Private wasm32 C pointer/data checkpoint

Rebased onto reviewed main `c8bfba1e` (including the reviewed i32/i64 `Ineg`
extension), this extends the private
[integer lowering seam](wasm-c-scalar-lowering.md), using the existing
[bounded runtime API](../crates/keel-wasm/README.md). The private test API remains
`irgen::wasm_scalar::lower(src, debug) -> Result<Module, Error>`. It selects
ILP32 before parsing, runs no optimizer, and now verifies the **actual** returned
module with `verify_module_with_layout(&module, DataLayout::WASM32)`. There is
no native-verifier relabelling, widened initializer substitution, public Target,
CLI, Options, macro/header, calling-convention or optimizer exposure. Production
compilation remains LP64. Parent owns independent review and integration.

Integer and object-pointer locals, address/dereference/subscript expressions,
member access and fixed arrays/records as storage are admitted. Aggregate values,
assignment/copies and aggregate ABI remain excluded. Pointer slots, member
layout, scalar loads/stores and initializer bytes use four-byte pointers.
Local alignment above 16 is rejected. Fixed local and static array objects of
at least 16 bytes receive placement alignment of at least 16; this does not
change the source type's `sizeof` or `_Alignof`.

IR pointer arithmetic retains i64 offsets. Dynamic C long offsets are sign-
extended before scaling. Integer-to-pointer casts, including constant shortcuts,
normalize to unsigned low 32 bits before IR `inttoptr(i64)`. `ptrtoint` returns
the runtime's zero-extended address as i64, then explicit C integer conversion
applies. Pointer differences subtract and divide/shift in i64 **before** the
signed-long i32 truncation. No optimizer passes run over this output.

Defined integer/byte/numeric-pointer static data and pointer-only symbolic
initializers targeting defined data are admitted. Serialization evaluates with
the parser's model, including 32-bit long truncation and numeric-pointer
normalization. Symbolic offsets must be nonnegative and at most target object
size (one-past allowed). Pointer relocations require four-byte object/cursor
alignment and advance the cursor by exactly four. Initializer extents and
nonoverlap are checked before serialization. After parsing, the private gate
checks each array dimension against u32 bounds before storage multiplication; symbolic evaluation checks i64
multiplication/addition/subtraction overflow in the private model. Native
evaluator compatibility paths retain their existing behavior. Backend placement
still checks memory bounds and static-data/stack overlap.

The gate walks all globals and function bodies before pruning, including unused
static data/functions and unselected runtime branches. As in the scalar stage,
it validates the typed AST after parser constant/type-query erasure, rather
than defining a token-level restricted grammar. It rejects unknown
external data, function pointers/indirect calls, TLS/weak/lifecycle/section/
retention attributes, aggregate values, FP/vector/atomic/assembly/VLA/debug,
bit-fields, compound literals and builtins. Function boundaries remain prototype-
based, plain i32/i64 or void, with no pointer/narrow/aggregate ABI. A host passes
an explicit i32 offset and C casts it internally. Undefined plain function
prototypes still describe IR only; the runtime rejects imports.

Local byte-string initializers remain fenced. Local scalar-element aggregate
initialization is limited to objects at most 64 bytes, retaining the existing
exact-width zero/store path. Larger local initializers would use an unsupported
memory intrinsic and are rejected even in unused bodies. String-literal symbolic
pointer initializers are excluded; static byte arrays may be initialized with
strings. Uninitialized fixed arrays can exceed 64 bytes within checked slot and
runtime frame limits. No heap, dynamic allocation or new memory-copy contract
is introduced.

## Executable evidence and remaining blockers

A minimal test-only path dependency on `keel-wasm` (and its one lockfile edge)
joins the private C seam to `emit_wasm32_runtime_module`; there are no other
dependency changes. The maintained header-free
[fixture](../crates/keel-c/tests/fixtures/wasm-pointer-data.c) compiles to IR,
then independently validated runtime Wasm bytes, then executes in existing Node.
It checks a bit-level unsigned checksum against a JavaScript reference, signed
offsets, array initialization and subscript stores, record fields and canaries,
a direct helper, repeated recursive calls, static pointer arrays/one-past
fixups, four-byte pointer spills, high-bit and truncated pointer round trips,
and host-memory sentinels. Clang independently checks wasm32 and native
layout/declarations; native Keel IR remains under the native verifier. Existing
full C tests provide native runtime/ABI controls.

The fixture's large numeric-address pointer-difference calls exercise the
specified generated IR arithmetic ordering; they are not a claim that C permits
subtracting pointers to unrelated objects. Ordinary same-array differences,
including a non-power-of-two stride, are also executed. High-bit addresses are
round-tripped/compared, never dereferenced. No large memory is instantiated.

The backend remains independently bounded. Initially, `p--` emitted valid i64
`Ineg` that the old runtime rejected. Parent separately integrated and reviewed
the i32/i64 wrapping-negation extension in main `c8bfba1e`; that historical
blocker is now resolved. The maintained fixture uses actual `p--`, and the
former negative witness is now a positive C-to-Wasm-to-Node test. It checks
postfix value/address behavior, stores between local-array canaries, and guarded
INT_MIN/INT_MAX offsets. Extreme offsets return before pointer formation; every
accepted decrement and dereference remains within one actual C array. Repeated
calls also exceed the finite software stack budget if frames leak. Unsupported
unary operations retain the backend's existing negative controls. No backend
source is modified by this candidate. Earlier integer-only narrow arithmetic
limitations, public preprocessing/target routing, narrow/pointer ABI, optimizer
audit, FP/SIMD/atomics/debug and general application/WASI support remain blocked.

All Rust validation uses the existing shared lock and pins, `CARGO_CACHE_LINKS=0`,
default shared cargo cache/managed targets, and `--offline --locked -j2`. Generated
logs/artifacts stay outside Git. No remote hosts, subagents, toolchain/cache
changes, output deletion, main integration/cleanup or performance campaign.

## Validation before negation integration

Implementation commits are `ab44053f` and the private array-sizing guard
`efb262a0`. On 2026-10-24, Apple ARM64, rustc 1.99.1, Apple Clang 21.0.0
(`clang-2100.3.34.2`) and Node v24.22.1, final combined debug validation passed
308 C tests plus all 38 existing Wasm integration tests. Focused C release
validation passed 124 tests; the Wasm release controls passed 38 tests. There
were no failed/ignored tests or compiler warnings. Both C profiles include all
ten private lowering tests, the maintained C-to-Node fixture, independent Clang
wasm32/native declaration checks and the existing integer constant oracles.
Applicable native ABI/runtime controls executed locally; Linux/x86-only branches
remain cross-object evidence. Production check/build for both crates passed.
Focused rustfmt checks on the three changed Rust files and `git diff --check`
also passed. No performance or general Wasm C ABI claim follows.

Exact serialized invocations and final captured output are outside Git:

- `/tmp/keel-c-pointer-debug-final-8e8a.{jsonl,log}` — full
  `cargo test --offline --locked -j2 -p keel-c -p keel-wasm`.
- `/tmp/keel-c-pointer-release-8e8a.{jsonl,log}` — the focused release C command
  listed in the scalar checkpoint, with the same test selection.
- `/tmp/keel-c-pointer-wasm-release-8e8a.{jsonl,log}` —
  `cargo test --offline --locked -j2 -p keel-wasm --release`.
- `/tmp/keel-c-pointer-{check,build}-8e8a.{jsonl,log}` — production
  `cargo check/build --offline --locked -j2 -p keel-c -p keel-wasm`.

Every invocation above is wrapped by
`python3 suite/fast_opt_locked.py /tmp/<unique>.jsonl
cargo ...`. Append-only records retain successful earlier runs before
the final test-only array bound guard; final debug/release output includes that
guard. Focused development metadata is
`/tmp/keel-c-pointer-unit-8e8a.jsonl`, with the final passing output beside it
as `.log`; initial entries include a missing dev-dependency, fixture extraction
and qualifier corrections, and the discovered `Ineg` backend diagnostic.
The initial negative witness was subsequently replaced after the separately
reviewed backend integration, as recorded below. Parent owns independent review
and integration; this checkout is left intact for that work.

## Reviewed negation integration

Parent integrated reviewed negation support as main `c8bfba1e`. This candidate
was rebased onto that commit without conflicts: implementation `ab44053f` became
`39aa0ad4`, the array bound guard `efb262a0` became `321a5f11`, and the initial
checkpoint documentation became `ab94e5a1`. Test commit `9e8ba2be` replaces the
historical negative `p--` witness with positive C-to-Wasm-to-Node execution and
changes the maintained checksum/walk fixture to use actual pointer decrement.
There are no backend edits relative to this reviewed main.

Fresh combined debug validation passed 350 tests (308 C and 42 Wasm), and the
same focused C release selection passed 124 tests. All ten private lowering
tests pass in both C profiles, including the decrement/canary/extreme-offset
fixture and independent Clang wasm32/native checks. There were no failures,
ignored tests or compiler warnings. Focused rustfmt and `git diff --check`
passed. The unchanged production C check/build and the parent's independently
reviewed backend check/build are retained; production checks were not repeated
after this rebase. The separately reviewed backend's 42 release controls are
recorded in its README, rather than duplicated here.

Final rebased-source commands, all using the same prescribed cargo wrapper,
shared lock, pins and `--offline --locked -j2`, are recorded outside Git:

- `/tmp/keel-c-pointer-ineg-unit-8e8a.{jsonl,log}` — affected private C tests.
- `/tmp/keel-c-pointer-ineg-debug-8e8a.{jsonl,log}` — combined full C/Wasm debug.
- `/tmp/keel-c-pointer-ineg-release-8e8a.{jsonl,log}` — focused C release.

The local-byte and above-64-byte local initializer fences remain unchanged.
No public API, optimizer, native ABI/FP/memory or backend source change is
introduced by this integration follow-up. Independent review remains the next
parent-owned gate.
