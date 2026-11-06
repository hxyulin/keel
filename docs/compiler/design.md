# IR and backend design

Keel is a compiler toolchain: a C frontend, an LLVM IR importer, a low-level
SSA IR and optimizer, code generators for AArch64 and x86-64, assemblers, a
disassembler, linkers and a WebAssembly backend. Clang serves as a test oracle.
This page records the IR, the ABI, instruction selection, register allocation
and the SSA builder, and their limits. Target registers, frames and lowering
are in the [x86-64](../targets/x86_64/codegen.md) and
[AArch64](../targets/aarch64/codegen.md) code generation references.

## Crates and data flow

A native compile runs these steps in one process:

```text
C source ──keel-c──▶ keel_ir::Module ──keel-opt──▶ Module ──keel-codegen──▶ keel_asm_ir::Program
   ──keel-asm──▶ object bytes ──keel-link──▶ executable
```

`.ll` input enters through keel-llvm's importer and `.keel` text through
`keel_ir::parser::parse`. keel-wasm turns a Module into a WebAssembly module
instead of running keel-codegen. The `keel` driver owns all file access; the
library crates take and return bytes or values. Verification runs at two
points: the optimizer verifies every function a pass changed, and
`Allocation::validate` checks every allocation before emission.

| Crate | Owns | Workspace dependencies | Entry point |
| --- | --- | --- | --- |
| `keel` | The command-line driver | all compiler crates, `keel-util` | `crates/keel/src/main.rs` |
| `keel-c` | C preprocessing, parsing and IR generation | `keel-ir`, `keel-frontend`, `keel-opt` (mandatory inline-asm inlining), `keel-asm-ir` | `keel_c::preprocess`, `keel_c::compile` |
| `keel-frontend` | SSA construction from mutable variables | `keel-ir` | `FunctionBuilder` |
| [`keel-llvm`](../../crates/keel-llvm/README.md) | LLVM IR import, and diagnostic export | `keel-ir`, `keel-util` | `import::Importer`, `export` |
| `keel-ir` | IR types, text parser and printer, verifier, CFG, dominators and `LoopInfo` | `keel-asm-ir` (inline-asm register names) | `parser::parse`, `verifier::verify_module` |
| `keel-ir-interp` | Reference interpreter, a test oracle | `keel-ir` | `Interpreter` |
| `keel-opt` | Passes, tiers and the pass manager | `keel-ir` | `Pipeline`, `PASSES` |
| `keel-codegen` | Instruction selection, covers, register allocation, frames and emission | `keel-ir`, `keel-asm-ir`, `keel-asm`, `keel-util` | `Request` |
| [`keel-asm-ir`](../../crates/keel-asm-ir/README.md) | Structured assembly, the shared x86 tables and `Target` | none | `Program`, `Target` |
| [`keel-asm`](../../crates/keel-asm/README.md) | AArch64 and x86-64 assemblers | `keel-asm-ir`, `keel-util` | `ObjectAssembler::for_target` |
| [`keel-disasm`](../../crates/keel-disasm/README.md) | AArch64 and x86-64 decoders and formatting | `keel-asm-ir` | `decode_word`, `x86::decode` |
| [`keel-link`](../../crates/keel-link/README.md) | Mach-O and ELF linkers and archives | `keel-asm-ir`, `keel-util` | `link`, `elf::link` |
| [`keel-wasm`](../../crates/keel-wasm/README.md) | WebAssembly emission | `keel-ir` | `emit_module` |
| `keel-util` | LEB128 and atomic file replacement | none | `leb128`, `fs::replace` |
| `keel-suite` | The `cargo suite` workload runner | `keel-ir` | `cargo suite` |
| `keel-test-support` | Tool discovery, recorded skips, `TempDir`, native x86 program packets | none | `skip!`, `native_program!` |
| [`keel-cranelift`](../../crates/keel-cranelift/README.md) | Diagnostic Cranelift bridge, in its own workspace | `keel-ir`, `keel-llvm`, `keel-codegen`, `keel-asm`, `keel-util` | `crates/keel-cranelift/src/lib.rs` |

`Target` lives in keel-asm-ir because the assemblers, linkers and decoders need
it without depending on the IR.

## Program shape

- **CFG of basic blocks with block arguments, not phi nodes.** A value that
  depends on which edge was taken is passed by the jump (`jump block3(v2)`),
  so edits to the CFG stay local to the branch being edited, and two edges
  into the same block can carry different values.
- Values defined in a dominating block may be used directly; only merges go
  through block parameters.
- Terminators: `jump`, `brif`, `return`, `trap`, and `switch v0, block9,
  [0: block1, 7: block2(v3)]`, which compares an integer with distinct case
  values (bit patterns of its type) and otherwise goes to the default, listed
  first. Each backend chooses a jump table or a compare chain.
- **Pinned vs floating instructions.** Anything with side effects or that can
  trap (`load`, `store`, `call`, `udiv`, terminators) keeps its position.
  Pure instructions (`iadd`, `icmp`,
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
- **Editing.** Instructions are inserted at an `InsertPoint` (end of a block
  or before an instruction), moved and removed; blocks and block parameters
  can be removed. Removed entities keep their handles and nothing is
  renumbered; the verifier reports any use of a value whose definition was
  removed. `InstBuilder` (`func.ins(at).iadd(a, b)`) builds typed
  instructions. Passes also call `insert_inst` with an `InstData`, rewrite in
  place through `inst_data_mut` (keeping the result count and types), and
  redirect values with `replace_all_uses`.

**Decisions.** Instruction storage keeps contiguous per-block slices; there
is no linked list. The planned storage work adds `Function::remove_insts`
for passes that remove many instructions at once, position hints so an
insertion does not scan its block, a CFG stamp taken on read, and a 48-byte
`InstData` with indirect-call arguments and `invoke` kept inline. A
mid-block insertion still shifts the rest of the block. Functions are not
compacted to drop removed entities unless a workload shows the waste
matters.

## Types

- Scalar values: `i8 i16 i32 i64 i128 f32 f64 ptr`. Fixed-width vectors of 64,
  128, 256 and 512 bits, such as `i32x4` or `f64x8`; see [SIMD IR](simd-ir.md)
  for the shapes, operations, lane semantics and native lowering boundary.
- Signedness is on operations, not types. Scalar comparisons return `i8` 0/1;
  vector comparisons return integer vectors with all-ones/zero lanes.
- `ptr` is opaque. Memory is untyped bytes. `ptrtoint.i64` and
  `inttoptr.ptr` convert between a pointer and its address.
- There are no aggregate values: a struct lives in memory and is handled
  through a `ptr`. **Layouts** (`{i32, [4 x f32], {i8, ptr}}`, with C's
  rules for alignment and padding) only appear on signatures, where the
  calling convention needs a struct's shape (`ptr byval {..}`,
  `ptr sret {..}`; see ABI).

## Integer operations

- Constants: `iconst.i8 -1` stores its 64-bit immediate sign-extended from
  the type's width, one form per value. The parser and `InstBuilder::iconst`
  accept any spelling and store its low bits in that form, so
  `iconst.i8 255` reads back as `iconst.i8 -1`. The verifier rejects any
  other immediate. An `i128` constant is the immediate sign-extended to 128
  bits.
- Arithmetic: `iadd isub imul ineg`, `udiv sdiv urem srem` (trap on a zero
  divisor; `sdiv` also on `MIN / -1`), `umulhi smulhi` (high half of the
  double-width product).
- Bitwise: `and or xor bnot`, shifts `shl ushr sshr` and rotates
  `rotl rotr`. Shift and rotate amounts may be any integer type and are
  taken modulo the bit width.
- Bit counting: `clz ctz popcnt` are defined at 0 (`clz 0` and `ctz 0` give
  the bit width). `bswap` needs at least `i16`.
- `select c, a, b` picks without branching; `icmp` gives `i8` 0/1.
- Overflow: `v2, v3 = uadd_overflow v0, v1` (and `sadd usub ssub umul
  smul`) gives the wrapped result and an `i8` flag, 1 if the exact result
  does not fit the type taken unsigned or signed. This is Rust's
  `overflowing_*` and the building block for checked arithmetic and
  bignums. One instruction with two results, rather than LLVM's struct
  result, keeps both as plain values; GVN and GCM treat the results
  together.
- `i128` has all of these but `umulhi smulhi umul_overflow
  smul_overflow`. It lives in a pair of registers (or two spill slots), and
  its division calls compiler-rt (`__udivti3` and friends) after the same
  trap checks.

**Instructions vs intrinsics.** Pure operations the optimizer should fold
and number are instructions. Adding one costs an enum variant in an existing
format, so there is no reason to hide them behind calls, as LLVM does for
`ctpop`/`bswap` because its instructions are expensive to add. Intrinsics are for
call-like or target-specific operations, written `intrinsic memcpy(v0, v1, v2)`
(no `@`: they are not symbols). They share one `Intrinsic { id, args }` format;
everything passes need (signature, purity, memory effects, trap possibility,
inlining) comes from the registry in `intrinsic.rs`, generated from
`tools/x86-isa/intrinsic-registry.json`.

`trap` is a terminator instruction, not an intrinsic: nothing after it runs,
and as an ordinary instruction every block using it would need a dummy
terminator (LLVM's `call @llvm.trap()` + `unreachable`). `debugtrap` can be
resumed by a debugger, so it is an intrinsic. General assume and a distinct
nontrapping unreachable instruction remain unimplemented/deferred;
`__builtin_unreachable` still lowers to `trap`.

## Float operations

- `f32` and `f64` follow IEEE 754 with round-to-nearest-even. Constants are
  `fconst.f64 1.5` (also `inf`, `-inf`, `nan`, or the raw bits in hex,
  `fconst.f64 0x7ff0000000000001`). The instruction stores bits, so it is
  hashable and keeps NaN payloads; the printer writes the shortest decimal
  that reads back exactly, and hex for NaNs.
- Arithmetic: `fadd fsub fmul fdiv fneg fabs fsqrt`, and `fmin fmax` with
  IEEE 754-2019 `minimum`/`maximum` semantics (NaN in, NaN out; -0 < +0),
  which is what aarch64's `fmin`/`fmax` do.
- Rounding to an integral float: `fceil ffloor ftrunc fnearest` (ties to
  even).
- `fma a, b, c` is `a * b + c` rounded once (`fmadd`). `fcopysign a, b` is
  `a` with `b`'s sign; like `fneg` it only moves the sign bit, NaN payloads
  included.
- `fcmp.<cond>` gives `i8` 0/1 for `eq ne lt le gt ge ord uno`. Every
  condition except `ne` and `uno` is false when an operand is NaN; `ne` is
  C's `!=`.
- Conversions: `fpext`/`fptrunc` between f32 and f64, `sitofp`/`uitofp`
  from `i8`–`i64`, `bitcast` between an integer and a float of the same
  size, and `fptosi`/`fptoui` to `i32`/`i64`, which **saturate** (NaN gives
  0), as the hardware and Rust's `as` do. That keeps them pure, with no UB
  and no trap. Narrower results are a `trunc` away, with whatever
  semantics the frontend wants.
- **Which NaN** an arithmetic operation returns is unspecified (as in
  WebAssembly). This lets the optimizer fold float operations on the host
  and treat `fadd`/`fmul` as commutative: only a NaN's payload could tell.
  Folding produces the canonical quiet NaN, so output does not depend on
  the host. `fneg`, `fabs` and `fcopysign` are exact: they only change the
  sign bit.

## Undefined behaviour

- Integer arithmetic wraps. There are no overflow-trapping arithmetic
  variants and no `assume` instructions; the overflow instructions return a
  wrapped result and a flag, and the C frontend does not support
  `__builtin_assume`.
- Uninitialized memory reads give an arbitrary but stable value.
- Out-of-bounds memory access cannot be defined at this level; memory safety
  is the job of a higher-level IR.

Instruction construction and verification share the opcode's result signature.
An in-place rewrite must retain its result count and types; changing either
requires a new instruction plus value replacement. Verification checks raw
value/block/callee/signature/slot/symbol references before CFG construction
or operand resolution, so invalid pass output produces diagnostics instead
of indexing panics. Result/parameter indices use u32, matching value handles;
they do not wrap at 65,536 entries.

## Memory

- Stack memory is declared per function, not allocated by an instruction:
  `ss0 = stack_slot 16, align 4` (alignment a power of two up to 64), and
  `stack_addr ss0` gives its address. Sizes are fixed, so the frame layout is
  known before codegen. The C frontend implements object alignment above
  16 bytes by reserving extra space in a fixed slot and rounding the
  object address up within it.
- A fixed slot is *marked* when `intrinsic lifetime_start(v0)` or
  `intrinsic lifetime_end(v0)` names it, where `v0` is its `stack_addr`. An
  unmarked slot is live for the whole activation. A marked slot is dead on
  entry. `lifetime_start` makes it live, with indeterminate bytes: a read
  before the next write gives an arbitrary value that stays stable until
  the next write or marker of the slot. A start on a live slot does the
  same. `lifetime_end` makes it dead, and an end on a dead slot does
  nothing. Liveness follows each execution path, so a loop body with a
  start and an end has one lifetime per iteration, and a path with no end
  keeps the slot live until the function returns or unwinds. Any access to
  a dead slot's bytes is undefined behaviour, including through an escaped
  pointer; the interpreter reports it as a trap. A slot keeps one address
  across its lifetimes, and markers do not capture it. For the optimizer a
  start is a write of indeterminate bytes to the whole slot. A known value
  may cross an end, since any access after it is undefined. No pass may move
  an access to a slot across one of its markers. Codegen may give slots
  whose lifetimes never overlap the same frame bytes.
- Dynamic stack memory, for `alloca` and VLAs, comes from intrinsics:
  `intrinsic stack_alloc(v0)` gives `v0` bytes, 16-byte aligned, until the
  function returns; `stack_save()` and `stack_restore(mark)` free
  everything allocated in between, as a VLA's scope ends (a VLA in a loop
  would otherwise grow the stack every iteration). They are intrinsics,
  not instructions: rare, pinned, and nothing for the optimizer to fold.
  A function that uses them addresses its fixed frame from a base
  pointer that the stack allocation does not move. There
  is no stack probing yet: an allocation larger than the guard page can
  skip past it.
- Memory operations are ordered by CFG order.
- Ordinary, non-atomic accesses follow program memory effects; a conflicting
  unsynchronized non-atomic access is outside the IR's defined behavior.
  Volatile is required for externally observable accesses, not synchronization.
- Non-atomic loads/stores have explicit access properties:
  `load.i32 volatile align 4 notrap v0` and
  `store volatile align 4 notrap v1, v0`. Each flag is optional; default
  alignment is 1 and accesses may fault. `align` promises a nonzero power
  of two. `notrap` promises only that this access cannot fault, not that
  it can cross a write or an object-lifetime change. Volatile accesses
  must execute as written; they are neither atomic nor hardware fences.
- A fault on an access without flags is observable: the program stops at
  the first faulting access, with every earlier store done. `trap_ub`
  (`load.i32 align 4 trap_ub v0`) makes a fault on that access undefined
  behavior instead, as C and LLVM IR do for ordinary accesses. A pass may
  then reorder the access against other accesses the program executes
  anyway, so a different access can fault first. It permits no deletion,
  and `may_trap()` stays true. `notrap` lets a pass delete a load whose
  result is unused; whether it permits speculation is unspecified until a
  pass needs it.
  The verifier rejects `volatile trap_ub`. The C frontend and the LLVM
  importer mark every non-volatile load and store `trap_ub`; IR from other
  producers keeps observable faults.
- `memory_effects()` distinguishes reads, writes, read/write operations and
  conservative barriers; `may_trap()` separately describes exceptional exits.
  Unknown calls, inline assembly, atomic operations and stack-lifetime
  changes invalidate memory facts. A compiler barrier here is conservative
  analysis behavior, not a hardware-fence instruction.
- The by-value ABI reads argument bytes at the call instruction. The C
  frontend's [aggregate rules](../frontends/c/README.md#lowering) describe
  how C assignment and arguments map onto that.
- Loads and stores stay pinned in GVN and GCM. DCE can discard an unused
  ordinary load only when it promises `notrap`; volatile and potentially
  faulting accesses stay. Alias classes and call read/write summaries are not
  implemented. The memory passes are described in the
  [optimizer reference](optimizer.md).
- **Atomics** follow the C11/C++11 memory model, with the ordering written
  after the opcode: `atomic_load.i32 acquire v0`, `atomic_store release v1,
  v0`, `atomic_rmw.add seq_cst v0, v1` (also `sub and or xor xchg umin umax
  smin smax`; each gives the old value), `v3, v4 = atomic_cas acq_rel
  acquire v0, v1, v2` and `fence acquire`. Types are integers up to
  `i128`, and `ptr` for loads, stores, `xchg` and `cas`; memory must be
  naturally aligned, so an `i128` needs 16 bytes. A load cannot be
  `release`, a store cannot be `acquire`, and a fence cannot be `relaxed`.
  x86-64 lowers `i128` atomics only with CX16.
- **Compare and swap** gives the old value and an `i8` flag, 1 if it
  stored. It takes a success ordering, then a failure ordering. The failure
  ordering cannot be `release` or `acq_rel`, because a failed one stores
  nothing. It may be stronger than the success ordering, as in C++17 and
  LLVM. `atomic_cas weak` may fail while memory holds the expected value; it
  never stores when the values differ. A lowering computes the flag only
  when something uses it.

## Modules and calls

- A file is one module: `decl @putchar(i32) -> i32` lines for external
  functions and `func` definitions, in any order, and `file` lines for debug
  info (see Debug info). There is no linking of several keel modules yet.
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
- **Tail calls** are guaranteed, not an optimization hint: `return_call
  @f(v0)` and `return_call_indirect v1(v0) : sig` are terminators that
  call and return the callee's results, replacing the caller's frame, so
  recursion through them runs in constant stack (functional languages,
  state machines, interpreters). The callee must return exactly what the
  caller returns, extensions included, and arguments must not point into
  the caller's frame, which is gone. On aarch64 the arguments go into
  registers, then the epilogue runs and `b`/`br` jumps; arguments that
  would go on the stack, structs, and callers returning a struct are
  rejected for now.
- Functions and data are exported unless marked `local`, like C's
  `static`: `func @f(i64) -> i64 cc(c) local {`, `data @n = local align 4
  { i32 0 }`. A local symbol still has a name in the object file, but
  without `.globl` other object files cannot see it, so two modules may
  each have their own `@count`. The debug info does not mark it external.
- `returns_twice` on a signature (`decl @setjmp(ptr) -> i32 cc(c)
  returns_twice`) marks calls that may return again later, from a
  `longjmp`. C only promises the local variables not changed since the
  `setjmp`; in keel those are the values live across the call. The second
  return can come after code that, as far as liveness goes, could reuse
  their registers and spill slots (a value live after the `setjmp` is not
  live on the path that calls `longjmp`). So the register allocator gives
  each of them one location for the whole function: `longjmp` restores the
  callee-saved registers, and nothing else writes that slot. The optimizer
  needs no change: SSA values never change, and memory (address-taken and
  `volatile` locals) is left alone as always. For the same reason frame
  layout gives every stack slot of such a function its own bytes, even
  when lifetime markers would let two share.

## Inline assembly

This section describes the IR contract. The C frontend maps GNU C inline
assembly onto it for [AArch64](../targets/aarch64/c-inline-asm.md) and
[x86-64](../targets/x86_64/c-inline-asm.md), whose pages give the accepted
constraints and template modifiers.

`v2 = asm "add %0, %1, %2", "=r,r,r"(v0, v1) -> i64` runs target
instructions with operands substituted. The constraint string is the one
GCC and LLVM use, so a C frontend passes `asm` statements through after
numbering their operands the way LLVM does: `+r` becomes an output `=r`
and an input `0` tied to it, and named operands become numbers.

- Outputs come first, one per result: `=r` and `=w` for a general-purpose
  or SIMD/FP register, `={x0}` for that one (`=&r` is accepted; see below
  why it means nothing more). Then inputs, one per argument: `r`, `w`,
  `{x16}`, `m` for memory at the address given, `i`/`n` for a constant
  (an `iconst`), and `0`, `1`… for the register of that output, which the
  input initializes. SIMD/FP constraints accept scalar floats and vectors;
  a vector tie requires the same lane type and width. Then clobbers:
  `~{x9}`, `~{d8}`, `~{lr}`, `~{memory}`, `~{cc}`.
- An `asm` is pinned and never removed: every asm is treated as C's `asm
  volatile`. Memory effects follow the declared operands: `m` reads memory,
  `mwrite` writes it, and `mreadwrite` does both. `~{memory}` is a barrier
  for memory not named by operands. Register-only asm without that clobber
  does not invalidate memory facts. `~{cc}` declares changed condition flags.
- The AArch64 backend places operands in reserved registers no value is
  allocated to, moving inputs in before the template and outputs out after
  it. Clobbering a caller-saved allocatable register makes the asm a call for
  the register allocator; clobbered callee-saved registers are saved by the
  prologue. The x86-64 backend saves the homes it overwrites instead.

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
  and all-zero mutable data in zero-fill `__bss`. Data is exported unless
  `local`, like functions.
- Code finds a symbol relative to the program counter (`adrp` + `add`), since
  the executable loads at a random address. Symbols from other images
  (`decl`s, which may be in a dylib) are read from the global offset table
  (`adrp` + `ldr` with `@GOTPAGE`): the loader fills it in at startup.
- **Thread-local data** (`data @n = thread_local align 8 { i64 0 }`, `decl
  data thread_local @n`) has one copy per thread, and `symbol_addr` gives
  the current thread's. It cannot be `const`, and no data object may point
  to it, since its address differs per thread. On Darwin the symbol names
  a descriptor in `__thread_vars` (a getter, a key, and the initial value
  in `__thread_data` or `__thread_bss`); code loads it with `@TLVPPAGE`
  and calls the getter with the descriptor in x0, which returns the
  address in x0. The getter preserves every other register except x16,
  x17 and x30, so the access is not a call to the register allocator.
  `symbol_addr` stays pure: within one function the thread does not
  change, so GVN may merge two accesses.

## ABI

- Functions carry a calling-convention descriptor (`cc(c)`, the default),
  resolved against the target. A single `abi-lower` pass turns calls into explicit register
  and stack moves, using layouts to classify by-value structs.

Apple arm64's C convention, as implemented:

- Integers and pointers go in x0–x7 and floats in v0–v7, each class counted
  separately (`f(i64, f64, i64)` uses x0, d0, x1).
- Once a class runs out, its arguments go on the stack in order, **packed at
  their natural size and alignment** (an `i32` takes 4 bytes). Standard
  AAPCS64 gives every stack argument its own 8-byte slot; Apple does not.
- **Variadic** arguments always go on the stack, 8 bytes each, even when
  registers are free: Apple's `va_list` is a pointer that steps 8 bytes at a
  time. A signature marks this with `...`: `decl @printf(ptr, ...) -> i32`.
  Calls pass the named parameters and then any number of `i32`, `i64`,
  `ptr` or `f64`, the types C's default argument promotions leave.
- `i8`/`i16` parameters and results may be marked `sext` or `uext`
  (`decl @f(i8 sext) -> i16 uext`): C widens `char` and `short` to 32 bits
  by their signedness, the caller for arguments and the callee for results.
  Keel code never relies on it (narrow values have unspecified upper bits),
  but C does, so external functions and indirect calls need the attribute on
  narrow parameters, and every defined function on a narrow result, since it
  is exported. On the stack, narrow arguments take their own size and need
  no extension.
- An `i128` takes the next two x registers, low half first, with no
  rounding to an even register (AAPCS64 rounds; Apple does not). If only x7
  is left it goes to the stack, 16 bytes at 16-byte alignment, and closes
  the x registers. It is returned in x0:x1.
- **Structs by value.** A parameter `ptr byval {i32, f64}` passes the struct
  at that address by value: the callee receives the address of its own
  copy. `ptr sret {..}` as the first parameter (of a function returning
  nothing) is where the callee writes a struct result. Classification
  (`keel-codegen/src/abi.rs`), as checked against clang:
  - one to four floats of one type, however nested (a homogeneous float
    aggregate), go one per v register; on the stack they keep their natural
    size and alignment;
  - other structs up to 16 bytes go in one or two x registers, or on the
    stack in 8-byte-aligned slots rounded up to 8;
  - a struct that does not fit in the registers left goes whole to the stack
    and closes that register class to later arguments;
  - bigger structs are copied by the caller, which passes the copy's
    address;
  - results come back the same way, with bigger ones written to memory whose
    address the caller passes in x8.

  Struct bytes are moved with exact-size loads and stores, so nothing reads
  or writes past a struct's end.
- Calls store stack arguments at `sp`, in an outgoing-argument area at the
  bottom of the frame sized for the largest call, so `sp` never moves after
  the prologue. A function reads its own stack parameters at `x29 + 16`.

- **Defining** a variadic function: `intrinsic va_start(v0)` initializes
  a `va_list` at `v0`, and `v1 = va_arg.i64 v0` reads the next argument and
  advances it (types as for calls). A `va_list` is target-defined memory, 8
  bytes on Apple arm64 (a pointer to the next 8-byte slot), so a frontend
  can pass one to `vprintf` by loading it as a `ptr`. `va_arg` works in
  any function, on a `va_list` it was passed (as `vprintf` reads its
  arguments); only `va_start` needs a variadic function.

- **Several results** (`-> (ptr, i64)`) come back where C returns a struct
  of them in natural layout, so C code declares such a function returning
  that struct. Results may be integers up to 64 bits, pointers, `f32` and
  `f64`, with no `sext` or `uext`, since struct fields are not extended.
  Version 1 accepts only the structs that give each result its own register:
  one to four floats of one type in v0–v3, or up to 16 bytes of integers and
  pointers with one result in each of x0 and x1. A struct that C would pack
  into one register, return in memory, or return with a float in an x
  register is rejected. `cc(fast)` places results as it places arguments,
  integers and pointers in x0–x7 and floats in v0–v7, and rejects more than
  eight of a class. Every signature `cc(c)` accepts is placed the same way
  under `cc(fast)`.

Limits: results that need memory go through `sret`; at most 4080 bytes of
stack arguments per call.

System V x86-64's C convention (`keel-codegen/src/sysv.rs`, whose module
docs list the rules) is placed by the same IR signatures. The differences
from Apple arm64 that matter to a frontend:

- Every stack argument takes its own 8-byte slot, and variadic arguments
  use registers like named ones. The caller sets `al` to the number of
  vector registers used.
- Structs up to 16 bytes are split into eightbytes, classed INTEGER or
  SSE, each going to the next register of its class (`{double, long}` in
  xmm0 and rdi). Bigger structs are copied onto the stack, not passed by
  address.
- Several results follow the same struct rule: each eightbyte comes back in
  the next of rax and rdx or of xmm0 and xmm1, so `(i64, f64)` is accepted
  here. `(f32, f32)`, which packs into xmm0, and anything over 16 bytes are
  rejected. `cc(fast)` takes integers and pointers from rax,
  rdx, rdi, rsi, rcx, r8 and r9, and floats from xmm0–xmm7.
- A `va_list` is 24 bytes, `struct { u32 gp_offset, fp_offset; ptr
  overflow_arg_area, reg_save_area; }`. A variadic function saves the
  argument registers into a 176-byte area that `va_arg` reads from
  before moving to the stack. In C it is an array of one such struct,
  so a `va_list` parameter is a pointer.

## Exceptions

- Modeled on LLVM's landing pads, which is what the platform's unwinder
  and C++'s personality expect. A function that catches names its
  **personality** (`func @f() personality @__gxx_personality_v0 {`), the
  routine that reads its tables while an exception unwinds.
- `v1 = invoke @f(v0), block1, block2` is a call terminator: if `@f`
  returns, control goes to `block1` with `v1` defined; if it throws, to
  the landing pad `block2`. Results may only be used where `block1`
  dominates. Neither destination takes arguments, since the copies would
  happen after the call and the call clobbers caller-saved registers;
  values reach the landing pad through dominance, in callee-saved
  registers or spill slots, which the unwinder restores.
- A landing pad block starts with `v2, v3 = landingpad catch @_ZTIi, catch
  null, cleanup`, which gives the exception object and the selector, and
  can only be reached by unwinding. The personality tries the clauses in
  order: `catch @typeinfo` matches a type, `catch null` anything, and a
  final `cleanup` lands anyway (selector 0) to run destructors.
  `eh_typeid @_ZTIi` is the selector for a type, to compare against.
- Rethrowing and resuming are ordinary calls (`__cxa_rethrow`,
  `_Unwind_Resume`) followed by `trap`: no special instruction needed.
- On aarch64 the call-site range covers just the `bl`. Each function with
  a personality gets an LSDA in `__gcc_except_tab` (call sites, action
  chains, a type table through the GOT) and `.cfi_personality`/`.cfi_lsda`.
  Every call must be in the call-site table, so the gaps between invokes
  get entries with no landing pad. Each LSDA starts with a non-temporary
  label: with `.subsections_via_symbols`, the linker splits sections into
  atoms at symbols, and without one it resolved every function's LSDA to
  the same address.
- Not yet: `invoke` through a pointer, filters (`throw()` specs), and more
  than 63 caught types per function.

## Target

- Four targets, chosen with `--target <triple>` (default: the host):
  **aarch64-apple-darwin** (Apple's AAPCS64 variant, Mach-O),
  **x86_64-unknown-linux-gnu** (System V, ELF, glibc),
  **aarch64-unknown-linux-gnu** (base AAPCS64, ELF) and
  **wasm32-unknown-unknown**, which only keel-wasm compiles.
  `keel_asm_ir::Target` names them, so the C frontend (type sizes,
  `va_list`, symbol prefixes), codegen, the assemblers and the linkers agree.
- On Apple, executables must carry an ad-hoc code signature
  (`LC_CODE_SIGNATURE`) or the kernel refuses to run them.
- Register roles, frames, code models, branch reach and unsupported
  operations are target facts. The
  [x86-64](../targets/x86_64/codegen.md#registers-and-frame) and
  [AArch64](../targets/aarch64/codegen.md) code generation references own them.

## Debug info

- **Source locations** are an optional `SourceLoc` (file, line, column; 0
  is unknown, as in DWARF) on each instruction, written after it as
  `v2 = iadd v0, v1 !f0:12:5`. The module lists the files: `file f0 =
  "src/main.c"`, declared before use. Frontends set a location on the
  instruction builder (`func.ins(at).loc(..)`, or `set_srcloc` on the SSA
  builder for everything built after). Passes keep an instruction's
  location when they move or rewrite it; gcm gives a rematerialized copy
  the original's, and an address copied to a load or store that access's.
- `keel compile -g` gives an input that names no files its own lines, so a
  debugger steps through the IR, with the `.keel` file as the source.
- **Line tables come from the assembler.** Codegen emits `.file` and `.loc`
  directives and the assembler encodes `__debug_line`, so keel never
  writes line-number programs itself. It does emit a small `__debug_info`
  (a DWARF 4 compile unit with a subprogram per function): the linker only
  records an object in the executable's debug map if it has a compile unit,
  and lldb finds the line table through that map.
- **Call frame information** is emitted for every function, with or without
  locations: C++ exceptions and debuggers unwind through keel frames with
  it. The system assembler can turn it into compact unwind info when
  saved registers follow Apple's pairing, or `__eh_frame` otherwise. Keel's
  assembler also selects compact encodings for representable ARM64 layouts,
  retaining `__eh_frame` for the rest.
- **Variables and types.** The module has a table of source-level types
  (`dtype dt0 = base "int" signed 4`, pointers, structs and unions with
  byte or bit-field members, arrays, typedefs, `const` and `volatile`,
  enums, function types), globals (`dglobal "count" dt0 @count`,
  thread-local ones too) and the source language (`dlanguage c11`, so
  lldb reads expressions as C). A function has its return type (`dreturn
  dt0`), where it is declared (`dloc f0:12:5`, the prologue's line) and
  variables, each in one of its stack slots (`dvar "x" dt0 ss1`, `dparam`
  for parameters). Codegen turns them into DIEs: the subprogram's frame
  base is `x29`, which stays put even when dynamic allocations move `sp`,
  and a variable is at a fixed `DW_OP_fbreg` offset from it. An
  over-aligned local instead stores its address in a pointer-sized slot:
  `dvar "x" dt0 ss1 indirect` adds `DW_OP_deref` to that location. lldb then
  prints, changes and passes them (`p x`, `expr x = 3`, `p f(x)`,
  `finish` showing the return value).
- **Lexical scopes**, so that a variable is only shown in its block: a
  function lists its scopes (`dscope ds1 in ds0`), a variable names its
  scope (`dvar "i" dt0 ss2 in ds1`), and so does each instruction's
  location (`!f0:5:9 ds1`). Scopes are on locations, as in LLVM, so that
  passes moving or copying an instruction take its scope with it. Codegen
  puts a label wherever the scope of the code changes; a scope's code is
  what lies between its labels, plus that of the scopes in it, and
  becomes a `DW_TAG_lexical_block` with one address range or, if passes
  or the block layout scattered it, a range list (`__debug_ranges`, each
  list based at its function so that its entries are differences the
  assembler computes, as for the unit's size).
- **Why variables only live in stack slots.** A value in a register has a
  location only between its definition and last use, and after
  allocation; describing that takes location lists, built from the
  allocator's intervals, and a variable whose value is never computed
  (its uses folded away) has none at all. A slot is valid for the whole
  function, and a store in the debugger is seen by the code that loads it
  next. So a frontend asked for debug info keeps variables in memory,
  like C compilers at -O0, and pays for it in speed: `keel cc -g` runs
  at clang -O0's speed rather than keel's. Passes do not touch stack
  slots, so `-g -O` stays correct.
- On ELF the same DWARF is emitted, with differences:
  - Linux debuggers read it straight from the executable, with no debug
    map. Section offsets are therefore labels the linker relocates
    (`.long .Ldebug_abbrev`), where Mach-O has 0.
  - The frame base is rbp.
  - Thread-local globals are located with `DW_OP_const8u sym@dtpoff;
    DW_OP_GNU_push_tls_address`.

  gdb sets breakpoints past the prologue and prints arguments and
  locals.
- Not yet: locations of values in registers, `DW_AT_decl_line` on
  variables, macros, inlined call sites.

## Register allocation

**Linear scan** (Poletto & Sarkar 1999), in `keel-codegen/src/regalloc.rs`.
It runs in one pass over sorted intervals, needs no interference graph, and
is easy to follow, which matters more here than the last few percent that
graph colouring would give.

Allocation is selected through `RegisterAllocator`, independently of
`InstructionSelector` and IR optimization tiers. `CodegenPipeline` pairs any
selector with any allocator, so tiers can be compared on the same selected
instruction plan and register pools. `--regalloc` picks the tier; `-O1` uses
Fast and the other `-O` levels use Full.

| Tier | Build | What it does |
| --- | --- | --- |
| `Full` | default | The algorithm below, with lifetime holes, selection register hints and loop-weighted spill choice |
| `Fast` | default | One contiguous interval per value, no hints, spills the interval that ends last |
| `Local` | `experimental-regalloc` | Block-local register cache: cross-block values keep stack homes, and local homes appear only on eviction |
| `Split` | `experimental-regalloc` | Linear scan, then spilled values stay in registers that are free at both ends of a lowering sequence, written back to their homes at block edges. Functions with vectors or `returns_twice` calls use Full |

All tiers share liveness, ABI constraints and the spill machinery, and every
allocation passes `Allocation::validate` before emission. The experimental
tiers stay only while they build, pass clippy and pass the
[experimental codegen lane](../development/testing.md#experimental-codegen-features).

Full runs these steps; Fast shares the first two and builds one range per
value.

1. **Linearize.** Reachable blocks in reverse postorder, which is also the
   emission order. Unreachable blocks are not emitted. Each instruction has
   two program points: it reads operands at `2k` and writes results at
   `2k + 1`, so a result can reuse the register of an operand that dies
   there. Block parameters are defined at their block's header point, and
   block-call arguments are read by the predecessor's terminator.
2. **Liveness.** Backward dataflow over blocks for live-in/live-out sets.
   With block arguments instead of phis there is no special case:
   parameters are definitions and arguments are uses.
3. **Intervals.** One interval per value: the ranges of points where it
   is live. In each block that is one range, from the block's start (live
   in) or the definition to the block's end (live out) or the last use;
   ranges of consecutive blocks join. A value used at the top of a loop
   body is live out of the latch, so its interval covers the whole loop.
   Between ranges are *lifetime holes*, where the blocks laid out there do
   not need the value. An `i128` gets two intervals with the same ranges,
   one per half, so its halves are allocated (and spilled) independently.
4. **Scan.** Intervals in start order, as in Wimmer & Franz (2010): the
   *active* intervals are live at the current point; *inactive* ones hold
   a register but are in a hole, and a new interval may take that register
   if it fits in the hole. Integers and floats are separate register
   classes (general-purpose and SIMD/FP registers) that never interfere:
   each interval only competes for registers of its own class. When no
   register is free, Fast spills whichever active interval ends last (or the
   new one, if it ends last). Full spills the interval with the fewest
   loop-weighted uses per live point, ties going to the furthest end. A
   spilled value lives in a stack slot for its whole life: no splitting.
   Spill slots are shared by intervals that do not overlap (holes ignored).

   Holes matter in big `switch`-based functions: without them, a value
   defined in one case and used in a tail block far below would block a
   register through every case in between.

**Calls** (including the memory intrinsics, which are libc calls, `i128`
division, and inline assembly that clobbers allocatable caller-saved
registers) clobber the caller-saved registers. An interval live across a
call (defined before it and used after) may only take a callee-saved
register or be spilled; other intervals prefer caller-saved registers,
which cost nothing to use. An interval live across a call that returns
twice covers the whole function (see `returns_twice`). The prologue saves
and the epilogue restores the callee-saved registers the function actually
uses.

**Parallel copies.** Block arguments move into parameters all at once, and
so do call arguments into argument registers and incoming arguments into
their allocated locations. `parallel_copy.rs` turns such a copy into
ordinary moves: emit any move whose destination nobody still needs to read;
when only cycles are left, save one destination in a reserved scratch
register and redirect its readers there. Each target's codegen reference
names its scratch registers.

**Rematerialization.** Under `experimental-regalloc`,
`RematerializingAllocator` wraps Fast or Full and replaces some spill homes
with recipes that recompute the value at each use: integer constants, stack
addresses and null pointers (`RematRecipe`). `--remat constants`, `stack` and
`all` select recipes; `pressure` also prefers recipe values as spill victims;
`null` substitutes only canonical null pointers after allocation and rejects
Local and Split. Each target lowers a recipe without changing flags. Default
builds produce no recipe homes.

The `null` mode accepts only a live `inttoptr.ptr` result whose input is a
live `iconst.i64 0`, through at most eight i64 aliases. Computed zeros, other
widths, result aliases and removed definitions are ineligible, and functions
with debug variables keep their null homes. Register assignments and spill
victims are unchanged; only the spill home of an eligible value becomes a
recipe. x86 lowers it as `mov r32, 0`, AArch64 as a zero move that does not
set NZCV. Combining `null` with another recipe mode returns the base
allocation unchanged. Its tests run with
`cargo test -p keel-codegen --features experimental-regalloc --test null_rematerialization`,
including native AArch64 fixtures for calls, `setjmp`, stack restore and
`invoke`. No performance gain is claimed for any mode.

Known gaps: no interval splitting outside the experimental Split tier (a
value spilled anywhere is spilled everywhere), and blocks are laid out in
reverse postorder only.

**Decisions.** The register-assignment design moves register effects, fixed
operands and ABI preferences into a shared policy. Its settled choices: x86-64
keeps r10 and r11 reserved, for 12 allocatable general registers. Frameless
x86-64 scalar C leaves land only if the suite shows no retired-instruction
regression. On AArch64, x0–x7 become allocatable and leaves frameless on
Darwin first, and on Linux after the ARM container interoperability test
runs. Values with no preference take x0–x7 after x9–x15, once both orders are
measured. The Fast tier gets register effects and fixed operands but no ABI
preferences. The FullAffinity tier was deleted. Before x0–x7 are allocatable,
the AArch64 indirect call must read its target as part of the argument copy.

## Instruction selection

**Tree selection on the SSA DAG.** `keel-codegen/patterns.txt` describes
binary root/child patterns and register affinities; `build.rs` generates
the rule table and affinity matcher. Selection has four tiers
(`SelectionTier`, or `--isel-tier`), each adding to the one before: Baseline
runs none of the passes below and no cover, Covers enables every cover, Trees
adds tree selection, and Full also folds immediate operands and adds register
hints. `-O0` uses Covers, `-O1` Trees and the other `-O` levels Full. Without
`-O`, and in the library API, selection defaults to Covers. `--isel
trees,folds,hints,covers` chooses passes on top of Baseline, so `--isel none`
is Baseline and `--isel covers` is Covers; `--isel all` enables everything.
Explicit selectors override `-O` in either order, with the last selector
winning. They run after any IR
optimization and before allocation,
visiting consumers first. A rule consumes a pure, single-use child in the
same block, preserving distinct source lines when debug locations exist.
Shared DAG nodes stay materialized, and unmatched instructions
use the existing target lowerings. The initial patterns cover 32/64-bit
multiply/add and multiply/subtract and shifted arithmetic/logic on aarch64,
and add/ptradd with a scaled shift (`lea`) on x86. Narrow integer and float
patterns retain their original semantics; floating multiply/add is never
implicitly fused because that changes rounding.

Target selection prepares instruction-kind and operand-use facts once per
function.

### Covers

A cover is a hand-written fused pattern: it claims one root
instruction, consumes adjacent instructions, and lowers the root with one
target sequence. `crates/keel-codegen/src/cover.rs` is the registry: one
`Cover` variant per pattern with a stable name, such as `x86-mask-test` or
`compare-branch`. Selection-stage covers (the address modes, `zero-test`,
`x86-division-remainder`, `x86-lea-mul3`, the x86 memory operands and the
AArch64 `ubfx` and multiply tuning) run inside `Selection::compute_with_uses`
and also need tree selection. Backend-stage covers run afterwards in a fixed order in
each target's `Covers::select`.

`Selection` records the owner of each claimed root in a per-instruction
`Vec<Option<Cover>>`. `Selection::claim` is the only writer, and a root has
one owner. The cover covers the instructions it consumes and publishes the
root's allocation operands. `Selection::is_free` is the shared eligibility
check: the instruction is not covered, has no owner or tree match, and has no
constraint. `compare-branch` and the select covers may consume a compare
that another cover claimed, so they check coverage, constraints and the owners
they exclude. Use counts and debug locations are still checked by each cover:
scalar covers use the machine use counts of `FunctionFacts`, vector covers its
raw counts, and source locations must match by file and line, or exactly,
depending on the cover. Each backend's `cover()` lowers a claimed root with one
exhaustive `match` on the owner.

`SelectionOptions::covers` lists the covers that may run. Baseline selection
runs none, so it is the reference for every cover; Covers, Trees and Full
enable all of them, and selection-stage covers fire only with tree selection. The [x86-64](../targets/x86_64/codegen.md#covers) and
[AArch64](../targets/aarch64/codegen.md#covers) references list each target's
covers. `--isel all,-x86-mask-test` drops one cover, and `--isel covers` or a
cover name adds them. `--codegen-report` counts the roots each cover claimed
per function. To add a cover: add a variant to the registry, write a claim
function that checks `enabled` and `is_free` and calls `claim`, call it from
`Covers::select` (or `Selection::compute_with_uses`), add its arm to the backend's
`cover()` match, and test it at a tier that enables it.

Each backend's `FunctionCompiler::new` (`new_for_target` on aarch64) runs
the per-function phases in a fixed order: preflight checks, the call area
at the bottom of the frame, selection and the target covers, allocation,
then the spill area, the private slots, IR stack slots and the frame size.
The phases are in `x86_64/plan.rs` and `aarch64/plan.rs`. Emission first
lowers an instruction through the cover that claimed it, if any, then
tries the i128 lowering (and on x86 the AVX and x87 ones), and otherwise
lowers it by instruction family in `<target>/lower_*.rs`. Target-independent
intrinsics lower in `x86_64/intrinsic_family.rs` and `aarch64/intrinsic.rs`.

Allocation sees the selected instruction's leaf operands at the root,
rather than the intermediate result. This both removes the consumed
result's interval and keeps its inputs live until the fused instruction,
including across intervening calls. Register hints are preferences among
free, permitted registers: they never override interference, register
classes, or call preservation. Generated hints prefer the destructive
input of x86 binary instructions and the source of integer bit-preserving
conversions on both targets.

Immediate and address folding remain target-specific:

- a `stack_addr` used only as the address of loads and stores is not
  computed when the target can address the slot directly;
- constants encodable as immediates, shift amounts, or stored zeroes are
  not materialized if every selected use folds them;
- an `icmp`/`fcmp` used only by the `brif` right after it branches on its
  flags, instead of materializing and testing a boolean.

With the `folds` pass, immediate operands are omitted from the allocation
graph. x86 stack addresses are omitted too; aarch64 stack offsets depend on the spill frame,
so address folding is finalized after allocation. A constant formerly
folded into a consumed multiply must be materialized if the fused
multiply/add reads it in a register. A breakpoint on a folded constant's
line stops at the instruction using it. Compare/branch fusion retains its
existing flag lowering.

## Adding an operation

First decide between an instruction and an intrinsic (see
[instructions vs intrinsics](#integer-operations)). An intrinsic's signature,
purity, memory effects, trap and inline facts come from
`tools/x86-isa/intrinsic-registry.json`; edit it, run
`python3 tools/x86-isa/generate.py` and check with
`python3 tools/test.py generators`. For an instruction, using `umulhi` as the
model of a binary operation:

1. **IR.** Add the variant to its opcode enum in `crates/keel-ir/src/inst.rs`
   and its name in that enum's `Display`. If it does not fit an existing
   `InstData` format, add a format and its result signature, which
   construction and the verifier share. Add the spelling to
   `crates/keel-ir/src/parser.rs` and a builder method to `builder.rs`.
   Review `can_trap`, `memory_effects` and `may_trap`, which decide whether
   the optimizer may move or delete it.
2. **Interpreter.** Add its semantics to `crates/keel-ir-interp/src/ops.rs`
   and a case to the matching family under
   `crates/keel-ir-interp/tests/interp/`. The interpreter and the name are
   the only exhaustive matches; every other consumer matches with a wildcard,
   so the compiler does not list the places below.
3. **Optimizer.** Add constant evaluation to `crates/keel-opt/src/const_fold.rs`,
   which `instcombine` and Fast share. Check `gvn::is_commutative`, which GVN
   and the canonical form share, `if_convert.rs` (speculation) and any
   `instcombine` rule family that should see it.
4. **Backends.** Lower it on [x86-64](../targets/x86_64/codegen.md#adding-an-operation)
   and [AArch64](../targets/aarch64/codegen.md#adding-an-operation). If the
   backend must find it before lowering, add a flag to `InstKinds` in
   `crates/keel-codegen/src/kinds.rs`.
5. **Other consumers.** keel-wasm, the keel-llvm exporter and keel-cranelift
   reject what they do not handle; add a lowering or a clear rejection.
6. **Tests.** Add a parse-and-print case under `crates/keel-ir/tests/golden/`
   (`BLESS=1` rewrites `.out` files), a const-fold golden in
   `crates/keel-opt/tests/golden/`, and a program under
   `crates/keel-codegen/tests/programs/`. Run
   `python3 tools/test.py ir optimizer codegen --quick`, plus
   `python3 tools/test.py cranelift` if keel-cranelift changed. An operation
   that no frontend emits yet must leave suite output unchanged:
   `cargo suite codegen-diff --baseline <keel before> --candidate <keel after> --expect-identical`.

## Optimizer

`keel-opt` runs function passes and module passes. The manager runs each pass
over the module before the next and verifies every function the pass reports
changed, so the IR is valid between passes and a broken pass is caught where
it broke things. CFG, dominator and loop-depth analyses are cached while a
function's CFG is unchanged.

Pure instructions float and are placed by GCM, so GVN and loop-invariant code
motion need no CFG surgery. Effects follow the explicit memory-effect model
above. The tiers (None, Fast, Basic, Balanced, Size, Full) are fixed pass
lists in `crates/keel-opt/src/lib.rs`; Full runs three bounded rounds of
inlining, scalar and memory cleanup, code motion and loop optimization,
followed by a memory and loop epilogue. It is not an unbounded fixed-point
loop. See the [optimizer reference](optimizer.md) for pass contracts and
[optimization levels](optimization-levels.md) for the tiers and flags.

## Frontend

`keel-frontend`'s `FunctionBuilder` builds SSA from mutable variables
(`declare_var`, `def_var`, `use_var`) with the algorithm of Braun et al.
(2013): reading a variable looks for its definition in the block, then in
the predecessors, adding a block parameter where several meet. Blocks are
*sealed* once all their predecessors exist; reads in unsealed blocks add
incomplete parameters that sealing fills in. Parameters whose arguments are
all one value are removed, and `finalize` repeats that until nothing changes,
since removing one can make another trivial.
Reading a variable that nothing defined gives zero (C's uninitialized
locals, code after a `return` that no branch reaches, a label nothing
jumps to), rather than a panic.
