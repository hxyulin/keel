# Scalar WebAssembly backend

`keel_wasm::emit_module` is the base of the `keel-wasm` backend, as proposed in
the [feasibility evaluation](wasm-feasibility.md). It takes a `keel_ir::Module`
and returns a complete core WebAssembly binary for pointer-free integer code. It
has no memory, data, runtime or C ABI. The
[memory backend](wasm-memory-backend.md) and the runtime profile in the
[crate README](../crates/keel-wasm/README.md) build on the same lowering. Native
instruction selection, register allocation and code generation are not involved.

## API

```rust,ignore
use keel_wasm::{emit_module, Export, Options};
let bytes = emit_module(&module, &Options {
    exports: vec![Export {
        name: "public_name".into(),
        function: "ir_function_name".into(),
    }],
})?;
```

`emit_module(&Module, &Options) -> Result<Vec<u8>, Diagnostic>` borrows the input
without modifying it. `Diagnostic` implements `Display` and `Error`; the message is
in `.0`.

- Export names are exact UTF-8 strings, may differ from IR names, and must be
  unique. Every exported function must be defined. Local and hidden IR functions
  may be exported.
- An empty export list exports nothing; an empty module is valid. There are no
  implicit exports, imports, underscore prefixes, start function, memory or table.

Emission first runs `keel_ir::verify_module`, then checks the entire module against
the supported subset, including unexported functions, unused callee-table entries,
declarations and indirect signature tables. Encoding starts only after that check
succeeds, and `wasmparser::Validator` (with the WebAssembly 1.0 feature set)
validates the finished bytes before they are returned. A declaration without a
definition is rejected even if unused, and direct callee-table entries must name a
definition with an exactly matching signature. Writing the bytes to a file is the
caller's job.

## Accepted subset

Function parameters and the optional single result are plain i32/i64 with normal
purpose, no extension descriptors, not variadic, and `cc(c)`. That spelling selects
this scalar value contract; it is not a native C calling convention. `cc(fast)`, x86
vector ABI metadata, `returns_twice`, aggregate boundaries, multiple results and
explicit call operand ABI descriptors are rejected.

The supported instruction list is exhaustive:

- `iconst.i32`, `iconst.i64`; i32 constants truncate to 32 bits.
- i32/i64 `iadd`, `isub`, `imul`, `udiv`, `sdiv`, `urem`, `srem`, `and`, `or`,
  `xor`, `shl`, `ushr`, `sshr`, `rotl`, `rotr`.
- All ten i32/i64 integer comparison predicates, producing canonical i8 0/1.
- Scalar `select`, direct `call` to a defined function, `return`, `jump`, `brif`,
  and i32/i64 `switch`, with per-edge block arguments and a default edge.
- Internal i8 constants and transport through block arguments, `select` and
  conditions. Constants truncate to 0..255. There are no i8 signatures, i8
  arithmetic/comparison/switch or narrow conversions in this profile.
  Shift counts may be i8/i32/i64 independently of the data width.
- `ineg` on i32/i64 (wrapping `0 - x`), added later for all three profiles.

Semantics:

- Addition, subtraction and multiplication wrap at the data width.
- Shifts and rotates reduce the count modulo the width, including negative and
  mixed-width counts.
- Division and remainder trap on zero; signed division also traps on MIN/-1.
  Signed remainder MIN/-1 is zero.
- Any nonzero integer condition is true, including an i64 with only high bits set.

Everything else is rejected, including other unary operations (bit counting, byte
swaps), conversions, i16/i128, all FP/FMA, high/overflow multiplication, vectors,
intrinsics, inline assembly, pointers, data, loads/stores, atomics, stack slots and
allocation, indirect/tail calls, exception handling and explicit trap instructions.
There is no silent native, helper, import or approximate-FP fallback. Weak/ODR,
lifecycle, used/retain, custom sections, personality, stack-slot/data-symbol tables
and indirect signature tables are rejected. Optimization hints and debug metadata
are ignored and produce no sections.

## CFG and alias lowering

Each active SSA definition gets its own typed Wasm local; entry block parameters use
the function parameter locals. Operand reads resolve aliases with
`Function::resolve`, while definitions write to their raw value's local so that a
surviving aliased instruction cannot overwrite its replacement. Removed instructions
and blocks are skipped; unsupported instructions still in the layout are rejected
even in unreachable blocks.

In general, a structured outer loop dispatches on an i32 program-counter local with
an if-chain, one arm per IR block. This handles irreducible CFGs without
reconstructing source nesting. Only the taken edge's argument copies run: all
sources are pushed on the Wasm operand stack first, then popped into destination
locals in reverse order, which gives parallel-copy semantics for self-edges,
swaps, cycles and different arguments on two edges to the same block. The operand
stack is empty whenever the PC is set. Falling out of the dispatcher traps.

Simple shapes skip the dispatcher (straight-line single-return functions and a
terminal two-way `if/else`); see the [crate README](../crates/keel-wasm/README.md).
This lowering makes no code-size or performance claim.

## Dependencies

The crate depends on `wasm-encoder = 0.245.1` (feature `std`) and
`wasmparser = 0.245.1` (features `std`, `validate`, `features`,
`prefer-btree-collections`), pinned exactly. This avoids component, SIMD, serde
and hash-table features. Validation uses the separate parser/validator, not byte
snapshot comparison.

## Testing

`crates/keel-wasm/tests/scalar.rs` requires `node` on `PATH` (validated with Node
v24) and fails if it is missing. Every accepted fixture passes wasmparser
validation and Node's `WebAssembly.validate`, compilation, instantiation and
execution, with BigInt reference results for wrapping, signed and unsigned
arithmetic. Fixtures cover diamonds, nested loops, multiple returns, switches at
both widths, irreducible two-entry loops, self-edge and cyclic phi swaps, distinct
arguments to the same block, aliases and removed blocks, recursion, explicit
exports, mixed-width shifts, arithmetic extrema and division traps. Negative
fixtures pass the IR verifier, so they test backend rejection rather than malformed
IR.

```sh
cargo test -p keel-wasm --test scalar
```
