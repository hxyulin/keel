# wasm32 memory backend

`keel_wasm::emit_wasm32_memory_module` extends the
[scalar backend](wasm-scalar-backend.md) with one fixed linear memory, loads and
stores, pointers and narrow-integer conversions. It uses the
[explicit IR storage layout](wasm-ir-layout.md). The runtime profile
(`emit_wasm32_runtime_module`, see the [crate README](../crates/keel-wasm/README.md))
builds on this contract and adds static data and a software stack; the CLI always
uses the runtime profile.

This page is the reference for memory, pointer and conversion semantics shared by
both profiles.

## API and memory ownership

```rust,ignore
use keel_wasm::{emit_wasm32_memory_module, Export, Options, Wasm32MemoryOptions};
let bytes = emit_wasm32_memory_module(&module, &Wasm32MemoryOptions {
    scalar: Options {
        exports: vec![Export { name: "read".into(), function: "read".into() }],
    },
    pages: 1,
    export_name: "memory".into(),
})?;
```

`emit_module` and its accepted subset are unchanged; only the memory and runtime
entry points accept the features below. All return complete validated bytes or a
`Diagnostic`, without partial output or input mutation. Supply raw, unoptimized IR
produced for wasm32; the caller owns that layout provenance.

The module defines exactly one owned, unshared memory, exports it under
`export_name`, and has no imports. Initial and maximum size both equal `pages`
(64 KiB units, 0..=65536). Validation uses u64 arithmetic, so the 4 GiB maximum is
representable; whether a host can allocate it is a separate matter.

The memory export name must be nonempty and distinct from every function export
name. Any valid UTF-8 string is accepted (including non-ASCII and embedded NUL).
Function exports follow the scalar rules.

Memory starts zeroed. The host may write the exported memory and observe effects
after synchronous calls. There are no data segments in this profile, no allocator,
no growth instructions and no imported or extra memories. The low/null region is
**not reserved or protected**: address zero is ordinary memory. The engine traps if
any byte of an access lies outside memory; there are no per-object, lifetime or
stack bounds.

## Pointers

Function parameters, results and direct-call signatures keep the plain i32/i64
scalar contract. Pointer and i8/i16 signatures are rejected even on internal
functions. A host passes an address as an i64 scalar and the IR converts it with
`inttoptr.ptr`; an i32 address must first be widened with `uext.i64`:

```text
func @read(i64) -> i64 {
block0(v0: i64):
    v1 = inttoptr.ptr v0
    v2 = load.i8 v1
    v3 = uext.i64 v2
    return v3
}
```

Pointer values live in Wasm i32 locals and occupy four bytes in memory
(`DataLayout::WASM32.pointer_bytes()`). The IR verifier keeps i64 pointer offsets
and conversions, with these wasm32 semantics:

- `inttoptr.ptr(i64)` keeps the low 32 bits.
- `ptrtoint.i64(ptr)` zero-extends the unsigned address to i64. Consequently
  `ptrtoint(inttoptr(x))` is not an identity: high bits are lost.
- `ptradd(ptr, i64)` adds the low 32 bits of the offset with i32 wrap-around.
  Every memory instruction uses memarg offset zero, so no folded offset can move
  this wrap after the engine's non-wrapping effective-address calculation.
- Pointer `eq`, `ne`, `ult`, `ule`, `ugt`, `uge` compare normalized address bits.
  Signed pointer comparisons are rejected.
- Pointers may flow through `select`, block parameters and parallel edge copies.
  There are no function addresses, tables or pointer-call signatures.

`iconst.ptr` is not valid IR; form a constant pointer with `iconst.i64` followed by
`inttoptr.ptr`. Integer constants of i8/i16/i32/i64 truncate to their width.

## Loads and stores

- i8/i16/i32/i64 accesses touch exactly 1/2/4/8 bytes, pointers exactly 4, in
  little-endian order. FP, i128 and vectors are rejected.
- Unaligned accesses work. Alignment flags must be nonzero powers of two; the
  encoded hint is capped at the natural width and never widens an access or adds
  an alignment trap.
- `notrap` is a producer promise. It does not suppress engine bounds checks and the
  backend never removes or hoists loads because of it.
- Volatile and ordinary memory operations are emitted in IR layout order, including
  across calls. Volatile accesses are neither atomic nor fences; shared memory and
  atomics are unsupported.

## Narrow integers

i8 and i16 values are kept as canonical zero-extended bit patterns in i32 locals.
Loads zero-extend; `sext` interprets the source sign bit; `uext` preserves unsigned
bits; `trunc` discards high bits and renormalizes. All widening and narrowing pairs
among i8/i16/i32/i64 are supported, implemented with MVP shifts rather than the
optional sign-extension instructions. Narrow stores write only the low byte or word.

Narrow arithmetic, comparisons and switches are rejected. i8/i16 values may be
shift counts for i32/i64 shifts, with the usual modulo-width count semantics.

## Rejected in this profile

Both entry points verify the IR and then check the whole module (unused
declarations, callee and indirect-signature tables, unexported functions,
unreachable code) before encoding. Beyond the scalar rejection list:

- All `Module.data` and function data-symbol tables are rejected here; static data
  is a runtime-profile feature.
- Stack slots, stack addresses, dynamic stack allocation and all intrinsics are
  rejected; the software stack is a runtime-profile feature. Ordinary recursion
  uses Wasm locals, and call-stack exhaustion is engine behavior.
- Imports, indirect and tail calls, variadics, aggregate or extended-FP ABI,
  explicit operand ABI, multiple returns, `returns_twice`, exception handling,
  weak/ODR, lifecycle, retain and section metadata, and architectural
  instructions are rejected.

The encoder and the independent `wasmparser::Validator` use the WebAssembly 1.0
feature set.

## Testing

`crates/keel-wasm/tests/memory.rs` covers the fixed maximum encoding, export name
errors, every integer and pointer access width with byte canaries, unaligned
addresses, end-of-memory and OOB accesses (with no partial OOB writes),
host-written bytes, high-bit pointer normalization, negative and wrapping offsets,
all 18 conversion width combinations, narrow values through CFG edges, volatile
ordering across calls and recursion, and unused unsupported contracts. Node checks
results against independent BigInt and byte oracles. The 65536-page case is
validated without being instantiated.

```sh
cargo test -p keel-wasm --test memory
```
