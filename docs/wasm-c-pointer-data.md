# Private wasm32 C pointer/data checkpoint

Based on reviewed main `0115a0f7`, this extends the private
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

The backend remains independently bounded. A separate regression retains the
current diagnostic for `p--`: the frontend emits valid i64 `Ineg`, which the
runtime rejects as `unsupported instruction Unary { op: Ineg, ... }`. The
executable fixture uses `p -= 1`. Parent is arranging a separate backend change
and independent review; when that lands, this witness must become acceptance.
Other unsupported instructions still diagnose at the backend boundary. No
backend source is modified here. Earlier integer-only narrow arithmetic
limitations, public preprocessing/target routing, narrow/pointer ABI, optimizer
audit, FP/SIMD/atomics/debug and general application/WASI support remain blocked.

All Rust validation uses the existing shared lock and pins, `CARGO_CACHE_LINKS=0`,
default shared cargo cache/managed targets, and `--offline --locked -j2`. Generated
logs/artifacts stay outside Git. No remote hosts, subagents, toolchain/cache
changes, output deletion, main integration/cleanup or performance campaign.

## Validation record

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
The explicit negative witness remains in the final suites for the parent's
separate backend follow-up. Parent owns independent review, integration and
any resulting witness update; this checkout is left intact for that work.
