# Bounded raw-IR wasm32 memory checkpoint

Based on main `4f9f49c0`, following the reviewed
[scalar backend](wasm-scalar-backend.md) and
[explicit IR storage layout](wasm-ir-layout.md). This implements a library-only
memory milestone from the [layout plan](wasm32-layout-plan.md). It does not enable
C compilation, a Target/CLI route, optimization passes, static data, a software
stack or a C/pointer call ABI. The private
[C layout](wasm-c-layout-foundation.md) and
[integer evaluation](wasm-c-integer-evaluation.md) work remains separate.
Native ABI, FP, ISA, memory and intrinsic behavior is unchanged.

## Additive API and memory ownership

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

The existing `Options { exports }` and `emit_module` API and accepted subset remain
unchanged. Only the new entrypoint accepts the memory/pointer/conversion subset
below. Both return complete validated bytes or `Diagnostic`, with no partial
publication or input mutation. Supply raw, unoptimized IR produced explicitly
for wasm32; native memory/layout optimizer assumptions have not been audited.
The caller owns layout provenance. An IR module does not record a target and
native C IR cannot be relabeled as wasm32.

The module defines exactly one owned unshared wasm32 memory, exports it under
`export_name`, and supplies no imports. Initial and maximum size both equal
`pages` in 64 KiB units. The inclusive page range is 0..=65536; validation uses
u64 byte arithmetic, including the valid 4 GiB maximum without host-size overflow.
Acceptance of a binary does not guarantee the host can allocate its memory.
Tests validate the 65536-page encoding without instantiating it.
The memory export name must be nonempty, exact UTF-8 and distinct from every
function export name. Unicode and embedded NUL are accepted as Wasm strings.
Function export rules retain the scalar contract (including exact names, unique
names and required defined functions). Neither function nor memory export names
are inferred. Invalid names/options return an error.

Memory starts zeroed; the host may supply bytes by writing the exported memory
and observe effects after synchronous calls. There are no data segments,
allocator, heap, memory-growth instructions, imported memories or extra memories.
The fixed maximum prevents host growth by positive pages, too. The low/null
region is **not reserved or protected**: address zero is ordinary accessible
memory when the configured size and access width permit it.
Engine bounds checks trap on an access whose complete footprint lies outside
memory. There is no individual object, lifetime or software-stack bounds claim.

## Pointer and scalar contracts

Function parameters/results, direct callee tables and declarations keep the
original i32/i64 normal scalar ABI. Pointer and i8/i16 function signatures are
rejected even for unexported/internal functions. A host passes an offset as an
i64 scalar, with `inttoptr.ptr` in the IR. An i32 scalar offset must first use
`uext.i64`, then `inttoptr.ptr`. These are explicit raw-IR operations, not a C ABI.
For example:

```text
func @read(i64) -> i64 {
block0(v0: i64):
    v1 = inttoptr.ptr v0
    v2 = load.i8 v1
    v3 = uext.i64 v2
    return v3
}
```

Internal pointer values use Wasm i32 locals; pointer storage footprint uses
`keel_ir::DataLayout::WASM32.pointer_bytes()` (four), never the host model.
The unchanged IR verifier requires i64 pointer offsets/conversions:

- `inttoptr.ptr(i64)` keeps the low 32 bits.
- `ptrtoint.i64(ptr)` zero-extends the normalized unsigned address to i64.
- `ptradd(ptr, i64)` adds the low 32 offset bits with i32 modular wrap.
  Every memory instruction has memarg offset zero; no folded offset can move
  this wrap after the engine's nonwrapping effective-address calculation.
- Pointer `eq`, `ne`, `ult`, `ule`, `ugt`, `uge` compare normalized address bits.
  Signed pointer comparison predicates are rejected.
- Pointer select/block parameters/parallel edge copies are supported. A pointer
  loaded from memory has the same normalized i32 representation. No function
  addresses, tables, pointer relocations or pointer-call signatures are supported.

`iconst` remains an integer operation: i8/i16/i32/i64 are accepted and constants
truncate to the IR width; `iconst.ptr` fails the unchanged IR verifier. Form
constant pointers with `iconst.i64` followed by `inttoptr.ptr`.

Integer and pointer loads/stores access exactly 1/2/4/8 bytes for i8/i16/i32/i64,
and exactly four for ptr, in little-endian order. FP, i128 and vectors are rejected
for both memory and value operations. Unaligned ordinary accesses work with
alignment one. Alignment flags must be nonzero powers of two; encoded alignment
hints are capped at natural access width and never widen access or insert an
alignment trap. `notrap` remains the producer's promise; it does not suppress
engine checks or cause loads to be removed/hoisted. Volatile and ordinary memory
operations emit in IR layout execution order, including across direct calls.
Volatile operations are neither atomic nor fences; shared memory/atomics are
unsupported. There are no hidden optimizer passes.

Internal i8/i16 values maintain canonical unsigned low-bit representations in
i32 locals. Loads zero-extend into that representation. Explicit `sext` interprets
the source sign bit; `uext` preserves unsigned bits. `trunc` discards high bits
and normalizes the destination. All widening/narrowing pairs among i8/i16/i32/i64
are supported; i8-to-i16 signed extension also normalizes its 16-bit result.
MVP shifts implement narrow sign extension, avoiding optional sign-extension
instructions. Narrow stores write only the low byte/word. Constants, loads,
select/block transport and integer conditions preserve normalization. Narrow
arithmetic, comparisons and switch data remain rejected; i8/i16 may be shift
counts for the existing i32/i64 shifts, with normal modulo-width count semantics.
No general narrow arithmetic, unary operations, FP/conversions or SIMD support
follows from the conversion subset.

## Preflight and remaining exclusions

Both entrypoints run the IR verifier, then preflight the whole module, including
unused declarations, direct callee and indirect signature tables, unexported
functions and unreachable instructions still in layout. Every unsupported
instruction/contract fails before encoding. The scalar rejection list remains
in force except for the explicitly listed additions above. In particular:

- All `Module.data`, including unused definitions and external declarations,
  are rejected, as are function data-symbol tables, even unused. No static
  layout/initializers/relocation/section/RTTI/TLS contract is provided.
- Stack slots, stack addresses, dynamic stack allocation, save/restore,
  introspection and all intrinsics remain rejected. There is no linear-memory
  software stack, frame placement, exhaustion check, restoration or reentrancy
  contract. Ordinary direct recursion and Wasm locals retain the scalar backend
  behavior; engine call-stack exhaustion is engine behavior.
- Imports, indirect/tail calls, variadics, aggregate/extended FP ABI, explicit
  operand ABI, multiple returns, non-C calling-convention spelling, returns_twice,
  EH, weak/ODR, lifecycle/retain/section contracts and architectural instructions
  remain rejected. No native helper or approximate-FP fallback is introduced.

The encoder and independent `wasmparser::Validator` retain the explicit
WebAssembly 1.0 feature set. No dependency versions/features or pins changed.
Node independently validates, compiles, instantiates and executes runtime
fixtures. No install, remote host, timing or performance campaign is required.

## Validation

`crates/keel-wasm/tests/memory.rs` covers fixed maximum encoding, export namespace
errors, all integer/pointer footprints with byte canaries, unaligned addresses,
zero/end-of-memory and OOB accesses, no partial OOB writes, host-written bytes,
four-byte pointer loads/stores/dereferences, high-bit normalization, full-width
negative offsets and wrap before addressing. BigInt width normalization provides
an independent arithmetic/conversion oracle. Further fixtures cover all 18
trunc/sext/uext width combinations, signed/unsigned loads, canonical narrow
constants/transport, pointer/narrow CFG copies, pointer unsigned predicates,
volatile order across aliasing direct calls/recursion, alignment/notrap flags,
unused unsupported contracts and scalar opt-in invariance.

Run using existing pins, shared lock and default cargo-managed storage:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-wasm-memory-unique.jsonl \
  cargo test --offline --locked -j2 -p keel-wasm
# Repeat the same command with --release for the release gate.
```

Generated evidence remains outside Git. Parent owns independent review,
integration and cleanup. These finite raw-IR tests do not establish Wasm C,
software-stack, native execution or performance support.
