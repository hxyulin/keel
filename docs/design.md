# Keel design decisions

Keel is an educational compiler backend: a low-level IR, optimizer, aarch64
code generator, assembler and Mach-O linker. This file records the decisions
and why they were made.

## Program shape

- **CFG of basic blocks with block arguments, not phi nodes.** A value that
  depends on which edge was taken is passed by the jump (`jump block3(v2)`),
  so edits to the CFG stay local to the branch being edited, and two edges
  into the same block can carry different values.
- Values defined in a dominating block may be used directly; only merges go
  through block parameters.
- **Pinned vs floating instructions.** Anything with side effects or that can
  trap (`load`, `store`, `call`, `udiv`, `.trap` variants, `assume`,
  terminators) keeps its position. Pure instructions (`iadd`, `icmp`,
  `ptradd`, constants…) float: their position in the text has no meaning
  beyond "the definition dominates the uses". A scheduling pass places them
  before instruction selection. This gives global value numbering and
  loop-invariant code motion almost for free while keeping a debuggable CFG.

## Data structures

- Function-local arenas indexed by `u32` handles (`Value`, `Inst`, `Block`).
- **No use-lists.** "Replace all uses" records a value alias that readers
  resolve. Dead code removal is mark-and-sweep from pinned instructions. Use
  counts are computed by a pass when it needs them. All mutation goes through
  methods, so use-lists can be added later without touching passes.

## Types

- Values: `i8 i16 i32 i64 i128 f32 f64 ptr`; vector types (`i32x4`…) later.
- Signedness is on operations, not types. Comparisons return `i8` 0/1.
- `ptr` is opaque. Memory is untyped bytes. `ptrtoint.i64` and
  `inttoptr.ptr` convert between a pointer and its address.
- **Layouts** are named declarations (`layout %Point = c { x: i32, y: i32 }`).
  `field.addr` and `elem.addr` are checked by the verifier and lowered to
  `ptradd` with a constant offset, so optimizations only see `ptradd`.
  Bitfields belong to layouts, not to the instruction set.

## Integer operations

- Arithmetic: `iadd isub imul ineg`, `udiv sdiv urem srem` (trap on a zero
  divisor; `sdiv` also on `MIN / -1`), `umulhi smulhi` (high half of the
  double-width product).
- Bitwise: `and or xor bnot`, shifts `shl ushr sshr` and rotates
  `rotl rotr`. Shift and rotate amounts may be any integer type and are
  taken modulo the bit width.
- Bit counting: `clz ctz popcnt` are defined at 0 (`clz 0` and `ctz 0` give
  the bit width). `bswap` needs at least `i16`.
- `select c, a, b` picks without branching; `icmp` gives `i8` 0/1.

**Instructions vs intrinsics.** Pure operations the optimizer should fold
and number are instructions. Adding one costs an enum variant in an existing
format, so there is no reason to hide them behind calls, as LLVM does for
`ctpop`/`bswap` because its instructions are expensive to add. Intrinsics are for
call-like or target-specific operations, written `intrinsic memcpy(v0, v1, v2)`
(no `@`: they are not symbols). They share one `Intrinsic { id, args }` format;
everything passes need (signature, purity) is in a per-intrinsic table in
`intrinsic.rs`. Current ones: `memcpy`, `memmove`, `memset`, `debugtrap`.
The aarch64 backend lowers the memory ones to libc calls for now.

`trap` is a terminator instruction, not an intrinsic: nothing after it runs,
and as an ordinary instruction every block using it would need a dummy
terminator (LLVM's `call @llvm.trap()` + `unreachable`). `debugtrap` can be
resumed by a debugger, so it is an intrinsic.

## Undefined behaviour

- Integer arithmetic wraps by default.
- `.trap` variants are defined: they trap on overflow, and later code may
  rely on the overflow not having happened. Overflow depends on signedness,
  so they come in pairs (`sadd.trap`/`uadd.trap`); not implemented yet.
- **The only source of UB is `assume`** (`assume.range`, `assume.nonnull`…).
  Debug builds turn assumptions into trapping checks; release builds trust
  them. UB is therefore opt-in, local and greppable.
- Uninitialized memory reads give an arbitrary but stable value.
- Out-of-bounds memory access cannot be defined at this level; memory safety
  is the job of a higher-level IR.

## Memory

- Stack memory is declared per function, not allocated by an instruction:
  `ss0 = stack_slot 16, align 4` (alignment a power of two up to 16), and
  `stack_addr ss0` gives its address. Sizes are fixed, so the frame layout is
  known before codegen; a separate `dynamic_alloca` can come later for VLAs.
- Memory operations are ordered by CFG order.
- Optional alias-class tags on loads and stores (`[stack]`, `[heap]`…)
  promise disjointness between classes.
- Atomics and fences follow the C++11 memory model.

## Modules and calls

- A file is one module: `decl @putchar(i32) -> i32` lines for external
  functions and `func` definitions, in any order. There is no linking of
  several keel modules yet.
- `v1 = call @f(v0)` calls a declared or defined function; a call's results
  follow the callee's signature and must all be named. Calls are pinned.
- Each function keeps a table of the functions it calls with their
  signatures, so a function can be printed, verified and compiled on its
  own. The module verifier checks those tables against the declarations.
- `v0 = symbol_addr @f` is the address of a function or data object; one
  instruction serves both, since both are just linker symbols. It is pure.
- `v3 = call_indirect v0(v1, v2) : (i64, i64) -> i64` calls through a
  pointer. The signature is written at the call, because a pointer has no
  type; functions keep these in a table without duplicates (`SigRef`), so
  instructions stay small and hashable.

## Data

Named data objects live at module level, next to functions:

```
data @msg   = const align 1 { b"Hi!\n\0" }
data @table = const align 8 { ptr @add, ptr @msg + 3, i32 7, zero 4 }
data @count = align 8 { zero 8 }
decl data @environ
```

- Items are byte strings, integers (`i8`–`i64`, little-endian), `zero n`
  and `ptr @sym [+ n]`. A `ptr` item is a relocation: the address is only
  known to the linker or loader, so the IR keeps the symbol name.
- `const` objects may go in read-only memory. Storing to them is not
  allowed (not checked yet).
- The verifier checks alignment (a power of two up to 4096), that objects
  are non-empty, that integers fit, that pointers name a symbol, and that
  pointers are 8-byte aligned, which Mach-O requires because the loader
  rewrites them in place.
- On Apple arm64, constant data without pointers goes in `__TEXT,__const`,
  constant data with pointers in `__DATA,__const` (the loader writes the
  pointers, then makes the page read-only), mutable data in `__DATA,__data`,
  and all-zero mutable data in zero-fill `__bss`. Data is exported, like
  functions.
- Code finds a symbol relative to the program counter (`adrp` + `add`), since
  the executable loads at a random address. Symbols from other images
  (`decl`s, which may be in a dylib) are read from the global offset table
  (`adrp` + `ldr` with `@GOTPAGE`): the loader fills it in at startup.

## ABI

- Functions carry a calling-convention descriptor (`cc(c)`, the default),
  resolved against the target. A single `abi-lower` pass turns calls into explicit register
  and stack moves, using layouts to classify by-value structs.

Current limits of the aarch64 C convention support: at most 8 arguments (no
stack arguments), at most one return value, no `i8`/`i16` return values or
arguments to external functions or through `call_indirect` (Apple's ABI extends those by C signedness,
which the IR does not record), and no variadic calls (Apple passes variadic
arguments on the stack, so `printf` needs the IR to mark a call variadic).

## Target

- First and only target until everything works end to end: **aarch64-apple**
  (Mach-O). Executables must carry an ad-hoc code signature
  (`LC_CODE_SIGNATURE`) or the kernel refuses to run them.

## Register allocation

**Linear scan** (Poletto & Sarkar 1999), in `keel-codegen/src/regalloc.rs`.
It runs in one pass over sorted intervals, needs no interference graph, and
is easy to follow, which matters more here than the last few percent that
graph colouring would give. The first backend spilled every value to the
stack; it was replaced once it had served as the baseline.

1. **Linearize.** Reachable blocks in reverse postorder, which is also the
   emission order. Unreachable blocks are not emitted. Each instruction has
   two program points: it reads operands at `2k` and writes results at
   `2k + 1`, so a result can reuse the register of an operand that dies
   there. Block parameters are defined at their block's header point, and
   block-call arguments are read by the predecessor's terminator.
2. **Liveness.** Backward dataflow over blocks for live-in/live-out sets.
   With block arguments instead of phis there is no special case:
   parameters are definitions and arguments are uses.
3. **Intervals.** One interval per value, `[start, end]`, covering its
   definition, its uses and every block it is live into or out of. A value
   used at the top of a loop body is live out of the latch, so its interval
   covers the whole loop. Single intervals without holes are conservative
   but always safe.
4. **Scan.** Intervals in start order. When no register is free, spill
   whichever active interval ends last (or the new one, if it ends last). A
   spilled value lives in a stack slot for its whole life: no splitting.
   Spill slots are shared by non-overlapping intervals with the same scan.

**Registers on aarch64:**

| Registers | Use |
|---|---|
| x0–x7 | call arguments and results; never allocated |
| x8, x16, x17 | scratch for reloading spilled operands, intermediate values, and parallel copies; never allocated |
| x9–x15 | allocatable, caller-saved |
| x18 | reserved by Apple; never touched |
| x19–x28 | allocatable, callee-saved |
| x29, x30 | frame pointer, return address |

Keeping x0–x7 out of the pool means a call's arguments can be moved into
place without moving live values out of the way first.

**Calls** (including the memory intrinsics, which are libc calls) clobber
the caller-saved registers. An interval live across a call (defined before
it and used after) may only take a callee-saved register or be spilled;
other intervals prefer caller-saved registers, which cost nothing to use.
The prologue saves and the epilogue restores the callee-saved registers the
function actually uses, with `stp`/`ldp` pairs.

**Parallel copies.** Block arguments move into parameters all at once, and
so do call arguments into x0–x7 and incoming arguments into their allocated
locations. `parallel_copy.rs` turns such a copy into ordinary moves: emit
any move whose destination nobody still needs to read; when only cycles are
left, save one destination in x16 and redirect its readers there. A
slot-to-slot move goes through x17. Moves are always 64-bit, since narrow
values have unspecified upper bits anyway.

**Frame layout,** from `sp` up:

```text
sp + 0              spill slots, 8 bytes each (only as many as needed)
...                 IR stack slots, each at its alignment
...                 padding to 16 bytes
x29 - 16 * pairs    callee-saved registers used, in pairs
x29                 saved x29, x30
```

Spill slots come first so `ldr x, [sp, #imm]` reaches them (at most
4095 × 8 bytes). The epilogue resets `sp` from x29 before popping the saved
registers, so it does not depend on the size of the rest of the frame.

Known gaps: no interval splitting (a value spilled anywhere is spilled
everywhere), no hints to place values in x0–x7 or to coalesce copies,
blocks are laid out in reverse postorder only, and constants are
re-materialized into registers rather than folded into immediates.
