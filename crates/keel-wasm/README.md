# Bounded raw-IR wasm32 runtime

This library-only checkpoint adds static data and static software-stack frames
to the reviewed [fixed-memory backend](../../docs/wasm-memory-backend.md).
The original scalar and fixed-memory APIs, options and accepted subsets retain
their contracts. No C frontend, Target, CLI, native backend, dependency or
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
