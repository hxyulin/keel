# keel-wasm

`keel-wasm` turns a `keel_ir::Module` into a complete core WebAssembly 1.0 (MVP)
binary. It is the backend behind `keel cc/compile --target wasm32-unknown-unknown
--emit-wasm` (see [docs/targets/wasm/README.md](../../docs/targets/wasm/README.md)), and can also be used
directly as a library.

It does not run an optimizer, does not use the native backend, and produces no
imports, start function, tables or runtime helpers. Inputs must be raw IR built for
the wasm32 storage layout; a module carries no target tag, so the caller owns that
provenance. Native-layout IR must never be relabeled as wasm32.

## API

Three emitters cover increasing feature sets. Each borrows the module without
mutating it and returns either complete, independently validated bytes or a
`Diagnostic` (which implements `Display` and `Error`); no partial output is produced.

| Function | Adds |
| --- | --- |
| `emit_module(&Module, &Options)` | Scalar integer code, no memory |
| `emit_wasm32_memory_module(&Module, &Wasm32MemoryOptions)` | One fixed, exported linear memory; loads, stores and pointers ([memory semantics](#memory-semantics)) |
| `emit_wasm32_runtime_module(&Module, &Wasm32RuntimeOptions)` | Static data, pointer fixups and a checked software stack for address-taken locals |

`validate_wasm32_runtime_module(&Module, &Wasm32RuntimeOptions)` runs the same
admission checks as the runtime emitter without encoding anything. The CLI uses it
to check the original module before optimization.

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

Exports are explicit: each `Export` maps an exact UTF-8 export name to a defined IR
function. There are no implicit exports and no underscore decoration.

## Supported subset

- **Types and signatures:** function parameters and the single optional result are
  plain i32/i64 with the `cc(c)` spelling, no extension or operand ABI descriptors,
  no variadics. Pointer, i8/i16 and aggregate signatures are rejected, including on
  internal functions. Multiple `Return` blocks are fine.
- **Integer operations:** i32/i64 arithmetic, bitwise operations, shifts and
  rotates, all ten comparisons, `select`, wrapping negation (`ineg`, computed as
  `0 - x`; negating INT_MIN returns INT_MIN). Division and remainder trap on zero;
  signed division traps on MIN/-1, while signed remainder MIN/-1 is zero.
- **Control flow:** `jump`, `brif`, `switch`, block parameters with per-edge
  arguments, direct calls to defined functions, recursion. Arbitrary (including
  irreducible) CFGs are supported.
- **Memory (memory and runtime profiles):** exact-width loads/stores of
  i8/i16/i32/i64 and 4-byte pointers, narrow-integer conversions, `inttoptr`,
  `ptrtoint`, `ptradd` and unsigned pointer comparisons.
- **Runtime profile:** static data (`Module.data`) with pointer fixups, static
  stack slots and `StackAddr`.

Everything else is rejected, including FP, SIMD, i128, atomics, intrinsics, inline
assembly, imports, indirect and tail calls, function addresses, dynamic stack
allocation, exception handling, TLS, weak/ODR linkage, RTTI, explicit sections and
retain metadata. Rejection is whole-module: unexported functions, unused
declarations and unreachable blocks are checked before any encoding begins.

## Memory semantics

The memory and runtime profiles define exactly one owned, unshared memory,
exported under `export_name`, which must be nonempty and differ from every
function export. Initial and maximum size both equal `pages` (64 KiB units,
0..=65536). Memory starts zeroed, and there is no allocator, growth, import or
second memory. Address zero is ordinary memory, and the engine traps only when
a byte of an access lies outside memory.

A host passes an address as an i64 scalar, which IR converts with
`inttoptr.ptr`; pointer and i8/i16 signatures are rejected. Pointers live in
i32 locals and occupy four bytes in memory:

- `inttoptr.ptr(i64)` keeps the low 32 bits, and `ptrtoint.i64(ptr)`
  zero-extends, so `ptrtoint(inttoptr(x))` loses high bits.
- `ptradd(ptr, i64)` adds the low 32 bits of the offset with i32 wrap-around.
  Every access uses memarg offset zero, so the wrap happens before the
  engine's non-wrapping address calculation.
- Pointer `eq`, `ne`, `ult`, `ule`, `ugt` and `uge` compare address bits;
  signed pointer comparisons are rejected. There are no function addresses.

i8, i16, i32 and i64 accesses touch exactly 1, 2, 4 and 8 bytes, and pointers
4, little endian. Unaligned accesses work; the alignment hint never widens an
access or adds a trap. `notrap` does not suppress bounds checks, and no load is
removed or hoisted because of it. Volatile and ordinary accesses keep IR order.
i8 and i16 values are zero-extended bit patterns in i32 locals: loads
zero-extend, `sext` and `uext` convert, `trunc` renormalizes and narrow stores
write only the low bytes. Narrow arithmetic, comparisons and switches are
rejected.

## Runtime profile

### Placement

`data_floor`, `stack_bottom` and `stack_top` are u32 byte offsets. `data_floor` must
be nonzero and at most `stack_bottom`; both stack bounds must be multiples of 16,
with `stack_bottom <= stack_top` and the top inside memory. A 65536-page memory can
be encoded, but the highest representable stack top is `0xfffffff0`.

The region below `data_floor` is reserved only for placement purposes: ordinary
accesses can still reach address zero, and there is no protected null page.

Data objects are placed in `Module.data` order starting at `data_floor`, each
aligned to its `DataInit.align` (a power of two up to 4096). Every object, including
zero-initialized ones, must end at or below `stack_bottom`. Items are packed with no
implicit padding using `DataLayout::WASM32`: integers use exactly 1/2/4/8
little-endian bytes and pointers exactly 4. Pointer items require object alignment
of at least 4 and a 4-byte-aligned cursor, and must target defined data in the
same module with an addend no larger than the target's size (one-past is allowed).
The resulting address must fit in memory.

Active data segments initialize bytes, integers and resolved pointers before any
exported call. Memory starts zeroed, so zero items and BSS emit no payload.
`DataInit.constant` is a producer promise; linear memory is not read-only.

### Static frames

Each function's static slots are packed in slot-table order at their alignment (a
power of two up to 16), and the frame is rounded up to 16 bytes. A private mutable
i32 global holds the stack pointer, initially `stack_top`.

On entry a function saves the incoming SP in a local, checks `SP >= frame_size` and
`SP - frame_size >= stack_bottom`, and traps before changing SP or running the body
if either fails. `StackAddr` is the new SP plus the slot offset. Every normal
`Return` restores the incoming SP. Recursive calls get independent frames.
Frames are not cleared, and nothing protects individual objects or stops a pointer
from outliving its frame.

**Any trap invalidates the instance; the host must discard it.** SP is not restored
on traps and earlier stores are not rolled back. There is no unwinding, recovery or
reentrancy ABI.

### Admission

Runtime emission verifies the real module with
`keel_ir::verify_module_with_layout(module, DataLayout::WASM32)`, then checks memory
options, placement, declarations, exports, every function and instruction, and
every frame. `validate_wasm32_runtime_module` performs exactly these checks without
cloning the module, encoding sections or expanding BSS; only the emitter validates
generated bytes.

## Code generation

Each SSA definition gets its own typed Wasm local. A general CFG is lowered as a
structured loop that dispatches on an i32 program-counter local; edge arguments are
copied through the operand stack so that swaps and cycles behave as parallel copies.
Some shapes are emitted more directly:

- A function with exactly one block ending in `Return` is emitted straight-line,
  without the dispatcher.
- A function with an entry `brif` to two distinct `Return` blocks becomes an MVP
  `if/else` (terminal-if).
- Adjacent `local.set L; local.get L` pairs become `local.tee L`
  (adjacent tee).
- Pure i32/pointer conversion chains that round-trip the low 32 bits are skipped
  (low-bit casts).

None of these change instruction order, access widths, pointer normalization or
stack checks. Every memory access uses a zero memarg offset so that wrapping
`ptradd` happens before the engine's non-wrapping address calculation.

## Testing

Tests require `node` on `PATH`. Every generated binary is validated with
`wasmparser` (WebAssembly 1.0 feature set) and then validated, instantiated and
executed by Node against independent BigInt and byte-level memory oracles.

```sh
cargo test -p keel-wasm
cargo test -p keel-wasm --release
```

| Test | Covers |
| --- | --- |
| `scalar` | Integer semantics, CFG shapes, calls, exports, unsupported families |
| `memory` | Access widths, bounds and OOB traps, pointer normalization, narrow conversions |
| `runtime` | Data placement and fixups, BSS, frames, recursion, stack exhaustion |
| `negation` | `ineg` in all three profiles |
| `preflight` | Admission parity with emission, nonmutation, large sparse BSS |
| `direct_return` | Straight-line emission and dispatcher fallback snapshots |
| `terminal_if`, `adjacent_tee`, `low_bit_casts` (plus `*_review`) | The emission patterns above |

Near-4-GiB memory and BSS cases are encoded and validated only; they are never
instantiated.
