# Pointer-free scalar WebAssembly checkpoint

`keel-wasm` implements the first bounded backend proposed in
[the feasibility evaluation](wasm-feasibility.md), based on main `666093b8`.
It accepts an already constructed `keel_ir::Module` and returns a complete core
WebAssembly binary. It is a library experiment; no public `Target`, CLI route,
C frontend, data layout, memory, runtime, object/linker or C ABI support is added.
Native instruction selection, register allocation and code generation are unchanged.
The native [ISA](userspace-isa-status.md), [intrinsic](c-x86-intrinsics.md) and
[vector](gnu-vector-mode.md) contracts remain native contracts.

The later [raw-IR memory checkpoint](wasm-memory-backend.md) adds a separate
explicitly selected memory API; the original scalar API/subset documented here
remains unchanged.

## API and publication

```rust,ignore
use keel_wasm::{emit_module, Export, Options};
let bytes = emit_module(&module, &Options {
    exports: vec![Export {
        name: "public_name".into(),
        function: "ir_function_name".into(),
    }],
})?;
```

`emit_module(&Module, &Options) -> Result<Vec<u8>, Diagnostic>` borrows input
without modifying it. `Diagnostic` implements `Display` and `Error` and contains
its message in `.0`. Export names are exact UTF-8 strings, may differ from IR
names, and must be unique. Every requested function must be defined. An empty
export list exports nothing; an empty module is valid. There are no implicit
exports, imports, underscore prefixes, start function, memory or table.
Local/hidden IR functions can be explicitly exported by this manifest.

Emission first calls `keel_ir::verify_module`, then preflights the entire module,
including unexported functions, unused callee-table entries, declarations and
indirect signature tables. Encoding starts only after preflight succeeds.
`wasmparser::Validator` independently validates the finished bytes before return;
validation explicitly selects WebAssembly 1.0 rather than evolving proposal
defaults. Errors return no partial binary. The caller owns any eventual file publication.
A declaration without a definition is rejected even when unused; the existing IR
verifier rejects duplicate declaration/definition names. Direct callee-table
entries must name a definition with an exactly matching supported signature.
No host imports or runtime helpers are inferred.

## Accepted subset

Function parameters and zero or one result are plain i32/i64 with normal purpose,
no extension descriptors, nonvariadic, single-return behavior, and `cc(c)` in the
input. That IR spelling selects this bounded scalar value contract; it does not
establish a native C calling convention in WebAssembly. `cc(fast)`, x86 vector
ABI metadata, `returns_twice`, aggregate boundaries, multiple results and explicit
call operand ABI descriptors are rejected.

The supported instruction list is exhaustive:

- `iconst.i32`, `iconst.i64`; i32 constants truncate to 32 bits.
- i32/i64 `iadd`, `isub`, `imul`, `udiv`, `sdiv`, `urem`, `srem`, `and`, `or`,
  `xor`, `shl`, `ushr`, `sshr`, `rotl`, `rotr`.
- All ten i32/i64 integer comparison predicates, producing canonical i8 0/1.
- Scalar `select`, direct defined `call`, `return`, `jump`, `brif`, and i32/i64
  `switch`, with per-edge block arguments and a default switch edge.
- Internal i8 constants and transport through block arguments/select/conditions.
  Constants truncate to 0..255; comparisons produce 0/1. There is no external
  i8 signature, i8 arithmetic/comparison/switch, or narrow integer conversion.
  Shift counts may be i8/i32/i64 independently of the i32/i64 data operand.

Addition, subtraction and multiplication wrap at the data width. Shifts and
rotates reduce counts modulo that width, including negative bit patterns and
mixed-width counts. Signed and unsigned division/remainder trap on zero;
signed division also traps at MIN/-1. Signed remainder at MIN/-1 is zero.
Comparisons distinguish signed and unsigned bit interpretations. Any nonzero
integer condition is true, including i64 with only high bits set.

Every other instruction/type is diagnosed, including unary operations (also
integer negation/bit counting/byte swaps), conversions, i16/i128, all FP/FMA,
high/overflow multiplication, vectors, architectural intrinsics, inline assembly,
pointers, data, loads/stores, atomics, stack addresses/allocation, indirect/tail
calls, EH and explicit unreachable/trap instructions. There is no silent native,
scalar-helper, import or approximate-FP fallback. Weak/ODR, lifecycle, used/retain,
custom sections, personality, stack-slot/data-symbol tables and indirect signature
tables are rejected. Ordinary optimization hints and source debug metadata do not
create Wasm sections or native behavior promises.

## CFG and alias lowering

Each active SSA definition receives its own typed Wasm local, with entry block
parameters occupying function parameter locals. Operand reads use `Function::resolve`.
Definition writes use raw values so a surviving aliased instruction cannot overwrite
its replacement. Detached instructions/blocks are absent from layout traversal;
unsupported instructions still in layout are rejected even in unreachable blocks.

A structured outer loop dispatches on an i32 program counter using an if-chain.
Each ordinary IR block is one dispatch arm. This handles irreducible CFGs without
recovering source nesting or changing native control-flow infrastructure.
Only the selected outgoing edge evaluates its argument copies. All sources are
first saved on the Wasm operand stack, then assigned to destination parameter
locals in reverse order. These stack temporaries implement a parallel copy for
self-edges, swaps/cycles, and different arguments on two edges to the same block.
The stack is empty before setting the PC and branching back to dispatch.
Invalid dispatch fallthrough traps; ordinary function exits use explicit returns.
This baseline makes no code-size or performance claim.

## Dependencies and validation

The workspace uses path crates plus explicit registry dependencies and its shared
lockfile. No additional local dependency policy or AGENTS.md was found. Locally
cached `wasm-encoder = 0.245.1` and `wasmparser = 0.245.1` were selected and pinned;
no tools were installed. Encoder features are limited to `std`; parser features
are `std`, `validate`, `features` and `prefer-btree-collections`. This avoids
component, SIMD, serde and hash-table features. The only new registry packages
are these two crates, `leb128fmt` and `bitflags`; existing package versions remain
unchanged. The encoder produces core instructions only. Binary validation is a
separate parser/validator, not byte snapshot comparison.
Both pinned Wasm crates declare Rust 1.82 as their minimum; this is below the
workspace's edition-2024 minimum of Rust 1.85. Existing locked `indexmap` also
requires 1.85. No workspace Rust pin or dependency version was changed.

`crates/keel-wasm/tests/scalar.rs` requires an existing `node` on PATH and fails
if it is unavailable. Accepted fixtures pass wasmparser validation and Node's
independent `WebAssembly.validate`, V8 compilation, instantiation and execution.
This explicit external-tool requirement follows the existing C ABI tests that
require Clang; it adds Node to environments running the full workspace tests.
Only Node v24.22.1 has been validated here; no Node installation or version pin
is supplied by this crate.
BigInt arithmetic provides independent wrapping/signed/unsigned integer oracles.
Tests cover diamonds, nested loops, multiple returns, switch/default at both
widths, irreducible two-entry loops, self-edge phi swaps, distinct arguments to
the same block, aliases/removal, recursion/void calls, explicit exports, internal
predicate transport, mixed-width shifts, arithmetic extrema and division traps.
Negative fixtures cover invalid exports, undefined calls, incompatible signatures,
unused contracts and unsupported operation families. Family rejection fixtures
also pass the IR verifier, avoiding rejection solely for malformed IR.

Run with the existing shared lock and toolchain:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-wasm-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-wasm
```

Generated binaries travel in memory to Node; build products use cargo's managed
checkout target directory. Logs and any other generated evidence stay outside Git.
No remote hosts, toolchain/cache configuration changes, performance campaigns or
main integration are part of this checkpoint. Independent review and integration
remain the parent task's responsibility. FP, general narrow integers, unary and
conversion operations, memory/layout, C, host imports and runtime contracts require
separate tested checkpoints before the accepted subset expands.

Local validation on 2026-10-24 used the existing rustc 1.99.1 and Node v24.22.1.
All 13 integration tests passed, including Node execution of every accepted
fixture. `cargo check`, `cargo build`, `cargo fmt -p keel-wasm --check` and
`git diff --check` also passed. Final test command metadata is
`/tmp/keel-wasm-impl-20261024-e.jsonl`; build metadata is
`/tmp/keel-wasm-impl-20261024-build.jsonl` (check metadata uses suffix `a`).
These are finite integer/CFG checks, with no FP, C ABI, memory, native runtime
or performance evidence implied.

## Independent candidate review

Candidate `a786ee5a798e3ad3eccbdf47dc9f93b4b5274508` was reapplied onto
`666093b8` in a separate review worktree. The audit checked dispatcher branch
depths (jump/default 1, conditional/case 2), full-width i64 conditions/switches,
raw destination versus resolved source locals, layout-only emission, selected
edge parallel copies, defined-call indices/signatures, explicit exports, and
whole-module unsupported-contract preflight. No runtime miscompile was found.
The validator's permissive default proposal set was replaced by an explicit
WebAssembly 1.0 feature set in production and execution fixtures.

Review regressions add mixed i32/i64 switch self-edges with three-value cycles,
different copies on default/case edges, removed-block arena/layout gaps, and
unreachable laid-out unsupported code. A separate JavaScript state-update loop
checks the cycle results across 60 inputs; it does not inspect encoder output.
The integer oracle now includes 24 deterministic full-width patterns in addition
to nine boundary patterns at each width (1,089 ordered pairs per width).
BigInt mathematical operations and explicit width normalization remain independent
of the emitter's opcode mapping. Added verifier-valid negative gates cover unary
counts, conversions, explicit scalar call ABI descriptors and unused indirect
signature tables. These tests retain the bounded subset rather than extending it.

All 15 tests passed in debug and release on rustc 1.99.1 / Node v24.22.1, using
the command above (plus `--release`) with shared locking, `--offline --locked
-j2`, cargo and `CARGO_CACHE_LINKS=0`. Metadata is outside Git at
`/tmp/keel-wasm-review-d9a054c9-debug.jsonl` and
`/tmp/keel-wasm-review-d9a054c9-release.jsonl`. Formatting and diff whitespace
checks passed. No native source or existing dependency version changed; no
native runtime, memory, FP, C ABI or performance validation is claimed.
