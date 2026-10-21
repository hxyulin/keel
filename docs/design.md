# Keel design decisions

Keel is an educational compiler: a low-level SSA IR, optimizer, assembly
generators for aarch64 macOS and x86-64 Linux, and a C frontend. Clang
serves as a test oracle and supplies assembly and linking in the examples. This file records the implemented design, its limitations and
explicitly marked future work.

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
  first. On aarch64 dense cases (at least 4, spanning less than 3× their
  count) become a jump table, the rest a chain of compares.
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
  removed. `func.ins(at).iadd(a, b)` builds typed instructions.

## Types

- Scalar values: `i8 i16 i32 i64 i128 f32 f64 ptr`. Fixed 128-bit SIMD values:
  `i8x16 i16x8 i32x4 i64x2 f32x4 f64x2`; see [SIMD IR](simd-ir.md) for operations,
  lane semantics and the current native lowering boundary.
- Signedness is on operations, not types. Scalar comparisons return `i8` 0/1;
  vector comparisons return integer vectors with all-ones/zero lanes.
- `ptr` is opaque. Memory is untyped bytes. `ptrtoint.i64` and
  `inttoptr.ptr` convert between a pointer and its address.
- There are no aggregate values: a struct lives in memory and is handled
  through a `ptr`. **Layouts** (`{i32, [4 x f32], {i8, ptr}}`, with C's
  rules for alignment and padding) only appear on signatures, where the
  calling convention needs a struct's shape (`ptr byval {..}`,
  `ptr sret {..}`; see ABI). Named layouts with checked `field.addr` /
  `elem.addr`, lowered to `ptradd`, may come later for frontends.

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
- Overflow: `v2, v3 = uadd_overflow v0, v1` (and `sadd usub ssub umul
  smul`) gives the wrapped result and an `i8` flag, 1 if the exact result
  does not fit the type taken unsigned or signed. This is Rust's
  `overflowing_*` and the building block for checked arithmetic and
  bignums. One instruction with two results, rather than LLVM's struct
  result, keeps both as plain values; GVN and GCM treat the results
  together. On aarch64 the flag comes from the condition flags (`adds` +
  `cset hs/vs`), from the high half of the product for 64-bit
  multiplication, and from a range check for `i8`/`i16`, computed in 32
  bits where it cannot overflow.
- `i128` has all of these but `umulhi smulhi umul_overflow
  smul_overflow`. On aarch64 it lives in a
  pair of registers (or two spill slots), and its division calls
  compiler-rt (`__udivti3` and friends) after the same trap checks.

**Instructions vs intrinsics.** Pure operations the optimizer should fold
and number are instructions. Adding one costs an enum variant in an existing
format, so there is no reason to hide them behind calls, as LLVM does for
`ctpop`/`bswap` because its instructions are expensive to add. Intrinsics are for
call-like or target-specific operations, written `intrinsic memcpy(v0, v1, v2)`
(no `@`: they are not symbols). They share one `Intrinsic { id, args }` format;
everything passes need (signature, purity) is in a per-intrinsic table in
`intrinsic.rs`. Current ones: `memcpy`, `memmove`, `memset`, `debugtrap`,
`va_start`, `stack_alloc`, `stack_save`, `stack_restore`, `frame_address`.
The aarch64 backend lowers the memory ones to libc calls for now.

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
  included. aarch64 has no scalar instruction for it, so it splices the bit
  in general registers (`bfxil`).
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

- Integer arithmetic wraps by default.
- **Planned:** overflow-trapping `.trap` variants would trap on overflow,
  allowing later code to rely on it not having happened. Overflow depends
  on signedness, so they would come in pairs (`sadd.trap`/`uadd.trap`).
  They are not implemented; the existing overflow instructions return a
  wrapped result and a flag.
- **Planned:** `assume` instructions (`assume.range`, `assume.nonnull`…)
  would make optimizer assumptions explicit, with checks in debug builds.
  There are currently no assumption instructions or assumption checks,
  and the C frontend does not support `__builtin_assume`.
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
  `ss0 = stack_slot 16, align 4` (alignment a power of two up to 16), and
  `stack_addr ss0` gives its address. Sizes are fixed, so the frame layout is
  known before codegen. The C frontend implements object alignment above
  16 bytes by reserving extra space in a fixed slot and rounding the
  object address up within it.
- Dynamic stack memory, for `alloca` and VLAs, comes from intrinsics:
  `intrinsic stack_alloc(v0)` gives `v0` bytes, 16-byte aligned, until the
  function returns; `stack_save()` and `stack_restore(mark)` free
  everything allocated in between, as a VLA's scope ends (a VLA in a loop
  would otherwise grow the stack every iteration). They are intrinsics,
  not instructions: rare, pinned, and nothing for the optimizer to fold.
  A function that uses them addresses its fixed frame from a base
  pointer, x19 on aarch64 (see the frame layout in `aarch64.rs`). There
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
- `memory_effects()` distinguishes reads, writes, read/write operations and
  conservative barriers; `may_trap()` separately describes exceptional exits.
  Unknown calls, inline assembly, atomic operations and stack-lifetime
  changes invalidate memory facts. A compiler barrier here is conservative
  analysis behavior, not a hardware-fence instruction.
- C expression qualifier shapes now survive pointer arithmetic, dereferences,
  casts, array/function decay, members, typedefs, `typeof`, `__auto_type`,
  conditional pointers and function returns. A volatile pointer object and
  a pointer to volatile data qualify different accesses. Parameter and local
  initialization writes retain object qualification; volatile bitfield reads
  and writes retain it on their storage-unit accesses.
- Volatile aggregate reads are materialized with explicit qualified accesses
  before passing a value to assignment, return or the by-value ABI. Copies
  involving volatile storage cannot become ordinary `memcpy`/`memset` calls.
  Aggregate copies/initializers containing volatile subobjects conservatively
  retain volatility on the whole accessed region. Local objects too large
  for the IR's u32 stack-slot sizes are diagnosed rather than truncated.
- Aggregate assignment copies permit self-aliasing: large copies use
  `memmove`, not an unjustified `memcpy` non-overlap promise. The value of an
  aggregate assignment comes from its source, without rereading its destination.
  The by-value ABI reads argument bytes at the call instruction; the C frontend
  freezes them earlier when later arguments or callee evaluation may change
  memory. It can defer copying across expressions proven free of writes and
  volatile accesses. A regression previously returned 9 instead of 3 after a
  later argument mutated the assignment destination.
- Loads/stores remain pinned to the memory-unaware GVN/GCM passes. DCE can
  discard an unused ordinary load only when it explicitly promises `notrap`;
  volatile and potentially faulting accesses are retained. Live `notrap`
  loads still retain their memory order.
  The opt-in `memory-forward` pass uses explicit effects to eliminate
  redundant ordinary loads without moving accesses or removing the first
  potentially faulting access. Alias classes and call
  read/write summaries are not implemented; the old alias-class design
  note described future metadata, not fields present in the IR.
- **Atomics** follow the C11/C++11 memory model, with the ordering written
  after the opcode: `atomic_load.i32 acquire v0`, `atomic_store release v1,
  v0`, `atomic_rmw.add seq_cst v0, v1` (also `sub and or xor xchg umin umax
  smin smax`; each gives the old value), `atomic_cas acq_rel v0, v1, v2`
  (gives the old value; the frontend compares it with the expected one) and
  `fence acquire`. Types are integers up to `i64`, and `ptr` for loads,
  stores, `xchg` and `cas`; memory must be naturally aligned. A load cannot
  be `release`, a store cannot be `acquire`, and a fence cannot be
  `relaxed`. `cas` has one ordering for success and failure, the stronger
  of C11's pair.
- On aarch64, read-modify-writes and `cas` use the LSE instructions
  (`ldadd`, `ldclr`, `swp`, `cas`…, suffixed `a`/`l`/`al` for the
  ordering), which every Apple arm64 CPU has, so there are no
  load-exclusive/store-exclusive loops. `sub` adds the negation and `and`
  clears the complement, since LSE has neither. Ordered loads and stores
  are `ldar`/`stlr`; fences are `dmb ishld` (acquire) or `dmb ish`.

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
  `volatile` locals) is left alone as always.

## Inline assembly

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
  input initializes. Then clobbers: `~{x9}`, `~{d8}`, `~{lr}`,
  `~{memory}`, `~{cc}`.
- An `asm` is pinned and never removed: every asm is treated as C's `asm
  volatile`. Memory is never cached in registers across one, so
  `~{memory}` changes nothing yet; nor does `~{cc}`, as no value lives in
  the flags between instructions.
- On aarch64 operands go in registers no value is allocated to: x0–x8,
  x16, x17, v0–v7 and v16–v18. Inputs are moved in before the template
  and outputs out after it, like a call's arguments and results. That
  costs a move each way, but the allocator needs no special case, and as
  every operand gets its own register there are no early-clobber
  problems. Allocatable registers cannot be named as operands; the ones
  that can cover system calls (the number goes in x16 on Darwin, x8 on
  Linux).
- The template names a register by its operand's type: `x0` or `w0`, `d0`
  or `s0`. An `m` operand is `[x0]`, an `i` operand its number. Modifiers
  pick another view: `%w0`, `%x0`, `%s0`, `%d0`, `%q0`, `%h0`, `%b0`, and
  `%c0` for a constant; `%%` is `%`. Separate instructions with `\n`: on
  Darwin `;` starts a comment.
- Clobbering a caller-saved allocatable register (x9–x15, v19–v31) makes
  the asm count as a call to the register allocator, so no value stays in
  any of them across it. Clobbered callee-saved registers (x19–x28,
  d8–d15) are saved by the prologue and not allocated in that function.
  x30 may be clobbered, since the prologue always saves it; x18 (reserved
  by Apple) and x29 (the frame pointer) may not, nor x19 in a function
  that allocates stack, where it is the base pointer.

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

Limits: at most one return value (return structs through `sret`); at most
4080 bytes of stack arguments per call.

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

- Two targets, chosen with `--target <triple>` (default: the host):
  **aarch64-apple-darwin** (Mach-O) and **x86_64-unknown-linux-gnu**
  (System V, ELF, glibc). `keel_ir::Target` names them, so the C frontend
  (type sizes, `va_list`, symbol prefixes) and codegen agree.
- On Apple, executables must carry an ad-hoc code signature
  (`LC_CODE_SIGNATURE`) or the kernel refuses to run them.
- **x86-64** (`keel-codegen/src/x86_64.rs`, whose module docs give the
  register roles and frame) differs from arm64 mostly where the ISA does:
  - With 16 general registers, never allocating argument registers would
    leave five. So rsi, rdi, r8 and r9 are allocatable, and a call's
    arguments move into place as one parallel copy (`f(b, a)` with `a` in
    rdi and `b` in rsi is a swap, broken with r11).
  - Instructions are two-address (`add d, b` overwrites `d`), and many
    use fixed registers (`div` in rdx:rax, shift counts in cl). rax, rcx
    and rdx are therefore never allocated and serve as scratch.
  - The fixed frame is addressed from rbp, and only outgoing arguments
    from rsp. A dynamic allocation that moves rsp then leaves every
    slot's address alone.
  - Code is position independent (PIE, Linux's default). Defined symbols
    are `rip`-relative, and others go through the GOT (`@GOTPCREL`) and
    the PLT for calls. Thread-locals use local-exec (`fs:` + `@tpoff`)
    when defined in the module, and initial-exec (`@gottpoff`) otherwise.
  - The baseline is x86-64-v2 (`roundsd`, `popcnt`). Traps are `ud2`.
  - Not yet on x86-64: exceptions, inline assembly, i128↔float
    conversions, i128 overflow multiply and `mulhi`, `switch` on i128.
- Conditional branches reach ±1 MiB. A function whose code may be bigger
  (interpreter loops get there: QuickJS's is about 4 MB from keel) has each
  rewritten as the opposite branch over a `b`, which reaches ±128 MiB.
  Only those functions pay the extra instruction; the size is bounded by
  counting instructions, so no branch is measured.

## Debug info

- **Source locations** are an optional `SourceLoc` (file, line, column; 0
  is unknown, as in DWARF) on each instruction, written after it as
  `v2 = iadd v0, v1 !f0:12:5`. The module lists the files: `file f0 =
  "src/main.c"`, declared before use. Frontends set a location on the
  instruction builder (`func.ins(at).loc(..)`, or `set_srcloc` on the SSA
  builder for everything built after). Passes keep an instruction's
  location when they move or rewrite it; gcm gives its copies the
  original's.
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
graph colouring would give. The first backend spilled every value to the
stack; it was replaced once it had served as the baseline.

Allocation is selected through `RegisterAllocator`, independently of
`InstructionSelector` and IR optimization tiers. The default `Full` tier
uses the algorithm below with lifetime holes and optional selection
register affinities. `Fast` collapses each interval to its first and last
points and skips affinities; it uses the same liveness, ABI constraints,
and spill machinery. Both tiers can be compared with the same selected
instruction plan and register pools through `CodegenPipeline`.

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
   register is free, spill whichever active interval ends last (or the new
   one, if it ends last). A spilled value lives in a stack slot for its
   whole life: no splitting. Spill slots are shared by intervals that do
   not overlap (holes ignored).

   The first version gave each value a single range from its first point
   to its last. Holes matter in big `switch`-based functions: a value
   defined in one case and used in a tail block far below blocked a
   register through every case in between. In Lua's interpreter loop they
   cut stack accesses from 1,143 to 437.

**Registers on aarch64:**

| Registers | Use |
|---|---|
| x0–x7 | call arguments and results; never allocated |
| x8, x16, x17 | scratch for reloading spilled operands, intermediate values, and parallel copies; never allocated |
| x9–x15 | allocatable, caller-saved |
| x18 | reserved by Apple; never touched |
| x19–x28 | allocatable, callee-saved |
| x29, x30 | frame pointer, return address |
| v0–v7 | float arguments and results; never allocated |
| v8–v15 | allocatable, callee-saved (the ABI preserves only the low 64 bits, which is all a scalar uses) |
| v16–v18 | scratch |
| v19–v31 | allocatable, caller-saved |

Keeping x0–x7 out of the pool means a call's arguments can be moved into
place without moving live values out of the way first. It also gives the
`i128` lowerings free working registers: they copy their operands into
x0–x7 as pairs, compute there, and move the result out.

**Calls** (including the memory intrinsics, which are libc calls, `i128`
division, and inline assembly that clobbers allocatable caller-saved
registers) clobber the caller-saved registers. An interval live across a
call (defined before it and used after) may only take a callee-saved
register or be spilled; other intervals prefer caller-saved registers,
which cost nothing to use. An interval live across a call that returns
twice covers the whole function (see `returns_twice`). The prologue saves
and the epilogue restores the callee-saved registers the function actually
uses, with `stp`/`ldp` pairs.

**Parallel copies.** Block arguments move into parameters all at once, and
so do call arguments into x0–x7 and incoming arguments into their allocated
locations. `parallel_copy.rs` turns such a copy into ordinary moves: emit
any move whose destination nobody still needs to read; when only cycles are
left, save one destination in x16 and redirect its readers there (`fmov`
moves a float's bits into x16 and back, so one temporary serves both
classes). A slot-to-slot move goes through x17. Moves are always 64-bit, since narrow
values have unspecified upper bits anyway.

**Frame layout,** from `sp` up:

```text
sp + 0              outgoing stack arguments (for the call that needs most)
...                 copies of big structs passed by value
...                 a slot keeping a call's sret address across it
...                 incoming structs that arrived in registers, and a struct
                    result returned in registers
...                 spill slots, 8 bytes each (only as many as needed)
...                 IR stack slots, each at its alignment
...                 padding to 16 bytes
...                 callee-saved FP registers used (d8–d15), in pairs
x29 - 16 * pairs    callee-saved general registers used, in pairs
x29                 saved x29, x30
```

Outgoing arguments must be at `sp` when a call is made. Spill slots come
right after them so `ldr x, [sp, #imm]` reaches them (at most
4095 × 8 bytes). The epilogue resets `sp` from x29 before popping the saved
registers, so it does not depend on the size of the rest of the frame.

Known gaps: no interval splitting (a value spilled anywhere is spilled
everywhere; the next step would be Wimmer & Franz's splitting, which
needs code generation to follow a value between locations), no hints to place values in x0–x7,
and blocks are laid out in reverse postorder only.

**Tree selection on the SSA DAG.** `keel-codegen/patterns.txt` describes
binary root/child patterns and register affinities; `build.rs` generates
the rule table and affinity matcher. The CLI `-O` preset enables all of these
target codegen passes, as does `--isel all`. `--isel trees,folds,hints` chooses
passes independently; explicit selectors override `-O` in either order, with
the last selector winning. Without `-O`, and in the library API, selection
defaults to baseline (`--isel none`). They run after any IR
optimization and before allocation,
visiting consumers first. A rule consumes a pure, single-use child in the
same block, preserving distinct source lines when debug locations exist.
Shared DAG nodes stay materialized, and unmatched instructions
use the existing target lowerings. The initial patterns cover 32/64-bit
multiply/add and multiply/subtract and shifted arithmetic/logic on aarch64,
and add/ptradd with a scaled shift (`lea`) on x86. Narrow integer and float
patterns retain their original semantics; floating multiply/add is never
implicitly fused because that changes rounding.

Aarch64 scalar loads and stores can also consume a single-use, same-block
`ptradd` with an i64 register offset. A single-use i64 shift can be absorbed
when its masked count is zero or log2(access bytes), producing a register-offset
address in the memory instruction itself. Accesses remain at their original
position and width, including volatile accesses; atomics and 128-bit/vector
accesses keep their existing lowering. Both address leaves are published before
allocation, with separate scratch registers for spilled bases, indices and store
values. Shared shifts stay materialized; shared or cross-block addresses and
distinct debug source lines prevent consuming those definitions. Narrow index
extension is not folded by this pattern.

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

## Optimizer

`keel-opt` runs function passes (`fn(&mut Function) -> bool`) and module
passes through the `Pass::run_module` hook. The manager runs each pass over
the module before moving to the next and **verifies after every pass**: the IR is valid between passes,
always, so a broken pass is caught where it broke things. Passes compute the
analyses they need; there is no analysis cache yet.

| Pass | What it does |
|---|---|
| `inline` | module-level bounded private direct-call expansion, including fresh by-value snapshots and remapped CFG/SSA/stack slots |
| `small-memory` | expand ordinary constant memory intrinsics of 0–32 bytes with exact accesses; preload memmove sources before stores; see [bounded memory expansion](small-memory.md) |
| `stack-forward-cfg` | opt-in stack forwarding followed by CFG fixed-point cleanup only when forwarding directly makes a previously nonconstant `brif`/`switch` operand constant; absent from every default pipeline |
| `stack-forward` | block-local byte forwarding for nonescaping fixed stack objects, including partial integer writes; remove private stores only after every load from the object disappears |
| `canonicalize` | resolve operand aliases, place integer literals on the right, order other interchangeable integer operand pairs by value ID, reverse integer comparison predicates when swapping |
| `simplify-cfg` | constant `brif` → `jump`, compose arguments through jump-only forwarding blocks, merge a block into its only predecessor, drop unreachable blocks |
| `const-fold` | evaluate instructions on constants with the machine's semantics (wrapping, shift amounts mod width, IEEE floats rounded at their own width); trapping divisions are left to trap |
| `instcombine` | consume canonical integer operands, fold constants, Boolean identities and wrapping integer cancellation; group constants in unshared associative chains; collapse integer conversion chains, and replace power-of-two unsigned division/remainder and multiplication with shifts/masks |
| `memory-forward` (opt-in) | block-local exact-address, same-type load reuse and store-to-load forwarding; every write invalidates all prior facts, and volatile accesses, calls, atomics and lifetime barriers clear them |
| `branch-ranges` (opt-in) | derive signed/unsigned intervals from dominating conditional edges and fold later comparisons against literals |
| `known-bits` (opt-in) | propagate integer known-zero/known-one facts through SSA, including carry-aware add/subtract/negate, remove proven redundant masks and fold comparisons from derived bounds |
| `phi-simplify` | replace uniform incoming block arguments with a dominating value, including anchored self-referential loop parameters; remove the corresponding edge arguments |
| `sccp` (opt-in) | propagate constants through executable edges and block parameters using sparse conditional constant propagation |
| `gvn` | scoped hash table over the dominator tree: an equivalent pure instruction that dominates wins. Commutative operands are sorted; reversed integer comparison operands reverse the predicate while retaining signedness. Loads are never merged. |
| `dce` | mark from pinned instructions except unused ordinary `notrap` reads, deduplicating resolved values when queued; a branch argument is only needed if its parameter is, so dead loop variables die with their parameters |
| `gcm` | Click's global code motion, the scheduler for floating instructions |
| `loop-vectorize` | guarded contiguous counted maps with packed arithmetic and scalar tails; see [SIMD IR](simd-ir.md) |
| `loop-reduce` | read-only wrapping i32/i64 sums with independent vector accumulators, fixed-offset stream loads, countdown and scalar tail; no floating reassociation |
| `loop-strength-reduce` | wrapping i32/i64 induction products replaced by same-width recurrences in two-block loops; at most 16 products per function, eight original header parameters and 128 body instructions; no memory operations or traps are moved |
| `fast-loop-reduce` | the same sum transform, restricted to functions with at most 256 instructions / 16 blocks, one stream address and at most eight vector-expression values |

See the [simplification audit](simplification.md) for missing analysis/rewrite
families, concrete phase-order examples, and early/late/repeated cleanup sweeps.

The default pipeline (`-O`) runs three bounded rounds of `inline-noninterposable, simplify-cfg, canonicalize, instcombine,
call-contracts, simplify-cfg, gvn, small-memory, stack-forward, canonicalize, instcombine, dce, gcm,
loop-vectorize, loop-reduce`. Cleanup between inliner invocations exposes smaller
helpers for the next round. The inliner keeps its existing growth budgets;
public definitions require an explicit `noninterposable` producer promise.
The 45 round entries retain code motion and loop optimization between rounds,
followed by this 14-entry epilogue:
`load-reuse, stack-promote, phi-simplify, dce, simplify-cfg,
loop-strength-reduce, canonicalize, instcombine, dce, loop-unroll,
loop-reduce-multi, loop-rotate, phi-simplify, dce`.
Memory cleanup precedes wrapping recurrences; unrolling precedes extended sum
reductions and rotation. `loop-reduce-multi` includes the affine-offset and
same-lane cast support, so those passes are not scheduled separately.
If-conversion and an additional final GCM remain opt-in after Lua screening
found a large fib regression in broader candidates. The selected schedule
passed the kernel and Lua correctness checks; noisy identical-binary timing
controls prevent a placement-independent speedup claim. This fixed 59-entry
schedule is not an unbounded fixed-point loop.
The earlier paired suite comparison on ARM and two x86 hosts measured 8.5–15.6% less
runtime than one round at 50–53% greater preprocessed-C-to-object compilation
time, with increased code size and uneven workload benefits. The
[suite comparison protocol](../suite/README.md#native-platform-comparisons)
describes the measurement boundary and limits.
Fast uses `fast-cfg, fast-local, fast-cfg, dce,
fast-loop-reduce`; the reduction eligibility scan and CFG analysis are size-capped.
Basic uses `simplify-cfg, canonicalize, instcombine,
call-contracts, simplify-cfg, dce`. Initial CFG canonicalization precedes operand normalization
to avoid invalidating it immediately. The scalar cleanup order was chosen for
lower compile cost while preserving assembly on the earlier measured corpus.
Inlining precedes cleanup so constants and aggregate temporaries become visible;
private stack forwarding and a second combining pass simplify the exposed copies.
Vectorization then adds guarded vector loops where the supported pattern matches. A pass can request canonical form on entry; the manager
restores it after intervening changes, including lazy aliases. This applies to
explicit and custom pipelines too. The contract and its limits are documented
in the [canonicalization section](simplification.md#canonicalization-contract).
`const-fold` stays available for isolated numeric folding and historical
comparisons; the defaults use the broader combining pass before and after
private stack forwarding. The paired
pass ablation matrix
supports retaining CFG cleanup, GVN, DCE and GCM. Removing GCM regresses runtime
by 4.2% overall; removing GVN grows aggregate text by 5.1%, with mixed runtime
effects. Late phi cleanup avoids the early regression but adds compiler cost
without an established runtime gain in the earlier schedule. The current
memory/loop epilogue uses late phi cleanup to remove newly exposed parameters.
SCCP remains opt-in after neutral runtime in those applications.

### Bounded function inlining

`inline` uses frozen templates of same-module `local` definitions and expands
only direct calls present at pass entry. Newly cloned calls wait for another
explicit invocation, so function order cannot cause recursive expansion.
Ordinary profitability limits are 64 instructions, 8 blocks and 512 bytes of
fixed stack storage. `inlinehint` doubles these limits; `alwaysinline` bypasses
only these soft limits and `noinline` prohibits expansion. Each caller accepts
at most 128 sites and 2,048 added IR instructions, with a module limit of 16,384
added instructions and hard stack budgets (including by-value snapshots).
These admission budgets reset on each invocation; Full uses three invocations.
They bound each round's pre-cleanup growth, not total final module or frame size.
See [function attributes](function-attributes.md#inliner-policy-and-hard-boundaries)
for exact limits and skip reports through `inline_with_report`. Normal CLI
passes discard those reports. `alwaysinline` acts only when the pass runs.
There is no profile-guided profitability model yet. Non-local definitions,
variadics, direct recursive cycles, `returns_twice` calls, exception handling,
guaranteed tail calls, inline assembly, and frame/variadic/dynamic-stack
intrinsics are excluded. Ordinary indirect calls can be cloned but are not
inlined. Function definitions remain available for address-taking and other
callers.

The signature must match exactly. Each site gets fresh blocks, values and
fixed stack slots, remapped symbol/signature tables, and a continuation with
block parameters carrying all return values. Existing argument evaluation
stays in its original order. `byval` arguments up to 128 bytes are copied,
including padding, into private snapshots before callee effects; aliasing a
by-value argument with an ordinary pointer therefore preserves call semantics.
`sret` uses the original caller-provided destination. Callee source lines,
lexical scopes and stack variable homes are remapped into a fresh scope below
the call site; the IR currently has no DWARF inline-subroutine representation.

`stack-forward` recognizes constant-offset addresses of fixed stack objects.
An address passed to a call, returned, converted to an integer, stored as a
value, or otherwise used outside ordinary loads/stores and recognized address
construction marks the object as escaping. Volatile or atomic access also
excludes the object. Inline assembly, frame inspection and stack-state barriers
exclude the function. For private objects, exact typed stores can satisfy later
loads in the same block. Integer loads through 64 bits can also be reconstructed
from earlier stored bytes, using the supported targets' little-endian layout.
Unknown or uninitialized bytes remain memory loads. No facts cross block
boundaries, and no general pointer-alias assumption is used. Stores are removed
only when no load from the object remains anywhere in the function; debug-visible
homes retain both reads and writes so debugger edits remain observable. This is limited aggregate cleanup rather than full scalar
replacement of aggregates.

`load-reuse` carries at most 128 exact memory facts along eight
unique-predecessor edges. An ordinary store updates the cached value but does
not by itself establish that the address can be read. Store-to-load forwarding
therefore requires a prior successful read of that exact address/type, or a
`notrap` promise on the later load. The first potentially faulting read and all
stores remain observable. Aliasing writes, memory barriers, merges, backedges,
and debugger-visible homes retain their conservative handling.

For compiler-only ablation, keep the same cleanup pipeline and omit `inline`;
omit `stack-forward` too when measuring the combined inlining/aggregate-cleanup
change. None and Basic tiers retain their prior pipelines. The `inline` and
`stack-forward` names are also available through explicit `-p` pipelines.

Instruction combining uses a consumer worklist, so substitutions expose
further folds even when block layout differs from definition order. Integer
rules follow wrapping arithmetic; division identities require a known nonzero
divisor, and floating-point identities are excluded. The power-of-two rules
are exhaustively checked for i8 and at wide integer boundaries, with a
64-subset pass matrix and native spill/selector checks. The first paired
application measurement (Apple M5 Max) gives runtime ratio 0.973 and compiler
ratio 1.011 versus the historical Full pipeline; 38/44 translation units
change assembly. Raw samples and intervals are in
`instcombine-aarch64-2026-10-15.json`.

Literal extension-mask rules also run in the bounded Fast visit: an AND
whose literal covers every source bit of a widening zero extension is
redundant, and sign extension of an AND with its source sign bit cleared
becomes zero extension. These rules resolve aliases, inspect the mask at
its own width, and need no new integer literal, including for i128 results.
Full already removes redundant zero-extension masks through known bits.
Combining skips single-result producers whose result has already been
aliased, preserving the replacement while dead code removal cleans up the
stale instruction. Exhaustive i8 mask/input checks and widening-pair,
overflow-flag, alias, and native regressions cover these rules. The pipeline
and tier defaults are unchanged; Full retains three rounds. No performance
measurement is claimed for these local simplifications.

The final default-pipeline confirmation reproduces the runtime improvement
(2.7% geometric mean), with 1.5% extra compiler cost and 25,052 bytes less
aggregate text (0.85%). The `-O` and explicit combining-preset assembly match
in all 44 application files. Workspace/native tests, six application suite
configurations, Clippy, and the Tracy feature build pass. Debugger/debug-info
integration tests were excluded from the workspace run. Raw samples,
compiler/source hashes and commands are in
`default-pipeline-aarch64-2026-10-15.json`.

Trivial-phi elimination checks every incoming edge, ignoring a parameter's
own backedge value only when another incoming value anchors it. Replacement
requires a value defined in a strictly dominating block, so values defined
inside the loop and unanchored cycles cannot become copies. A block worklist
revisits consumers after substitution. Entry ABI parameters remain; removing
other parameters preserves existing aliases while renumbering the remainder.
Tests cover distinct edges, loop induction, SCCP aliases, reverse layout,
128 pass subsets, and native execution with restricted register pools.
Early placement remains excluded from defaults: placing it before GVN/GCM regressed Lua fib by
2.21× and QuickJS fib/sort by 1.20×/1.16× in paired application measurements.
See `phi-simplify-aarch64-2026-10-15.json`.

Forwarding composes the incoming argument tuple with the outgoing jump,
including permutations and repeated arguments. A forwarding block whose
parameter is used outside its own jump stays: bypassing it would lose the
definition dominating that external use. One forwarding step per cleanup
round also lets closed forwarding cycles collapse to self-loops without
changing nontermination.

**SCCP** uses the monotone value lattice from
[Wegman and Zadeck](https://cs.brown.edu/research/pubs/techreports/reports/CS-89-36.html):
unknown, a typed bit-exact constant, or overdefined. A worklist revisits
users when values change and activates branch edges as conditions become
known. Block parameters meet arguments from executable edges; two arms
from the same predecessor remain distinct. Backedges can invalidate an
initial constant, while unchanged loop arguments remain constant. Unknown
values remaining in executable code are conservatively made overdefined.
Analysis converges before rewriting; parameter constants are materialized
in their own block so they dominate uses even with untaken predecessors.
`simplify-cfg` and `dce` retain responsibility for CFG and parameter cleanup.
`sccp` remains opt-in: the initial paired Lua, QuickJS and SQLite measurements
showed roughly neutral application runtime and extra compiler cost. The
`keel-basic-sccp` and `keel-O-sccp` suite configurations add `sccp` before
`instcombine` while preserving the other stages. `keel compile -p ...` and
`keel cc -p ...` select explicit pipelines, overriding optimization tiers.
The numeric folding kernel is shared with `const-fold` and `instcombine`.
All three also fold both results of signed/unsigned checked add, subtract
and multiply at widths through i64. SCCP tracks the wrapped value and
overflow flag independently, allowing checked results to drive branches.
Every i8 operand pair is checked against Rust overflow operations, with
wide-boundary and native regressions. i128 folds remain conservative because
the constant immediate cannot represent general i128 results. Application
assembly was identical in all 44 files and compile timing was neutral; see
`checked-fold-aarch64-2026-10-15.json`.
Regression tests
compare SCCP followed by all 32 subsets of the original five passes against
an independent execution model, and
native codegen tests exercise constant loop parameters with ordinary and
restricted register pools. Tracy has separate analysis and rewrite scopes.

**GCM** places each pure instruction between its *early* block (the deepest
block defining an operand) and its *late* block (the common dominator of its
uses), choosing the block in the fewest loops and, among those, the latest.
That hoists invariants out of loops and sinks code into the branch that needs
it. Three choices made after measuring the end-to-end programs:

- An instruction that stays in its block keeps its place. Moving everything to
  just before its first use made one program's spills go from 11 to 75: a
  chain of `select`s sank to the end of `main`, keeping every condition alive
  across a dozen calls.
- Instructions without operands (constants, addresses) are rematerialized:
  each block using one gets its own copy. GVN merges every `iconst 1` in a
  function into one value, which otherwise lives from the top of the function
  to its last use (33 → 47 spills in the register-pressure test). A register
  allocator that rematerializes when it runs out of registers would do this
  better.
- Addresses of fixed stack or global objects with constant offsets stay
  at the late placement instead of being hoisted out of loops. QuickJS's
  interpreter has hundreds of temporary aggregate objects: hoisting their
  field addresses kept hundreds of pointers live across dispatch and calls,
  forcing spills. Keeping these cheap computations near their uses removed
  most of its `-O` slowdown. Pointer arithmetic on incoming pointers still
  gets ordinary loop-invariant code motion.

GCM deduplicates consecutive uses while scanning each instruction, uses
producer IDs as generation markers for user membership, and groups constant
users by block. This avoids repeated scans of high-fanout use lists without
changing placement decisions or within-block order. A constant used in one
block needs no grouping or copies.

With these choices, `-O` output is never larger than the unoptimized output on the
test programs, and programs whose checks are all constants fold away
entirely, which checks constant folding against results from the hardware.

The IR contract audit
records the memory contracts, qualifier propagation, rewrite-verifier and
index/size checks, plus the aggregate-copy/ABI regression (previously 9,
now 3, matching Clang). Validation covers parser round-trips, both target
code generators, native execution and spills, workspace tests, four
application-suite configurations, Clippy and the Tracy feature build.
Debugger integration tests were excluded. This is a correctness/enabling
change; memory optimizations remain separate passes and no runtime speedup
is claimed. Effect/trap classification is exhaustive so future opcodes
cannot silently inherit memory-free or non-trapping behavior.

The first memory optimization is measured separately in the
memory-forward sweep.
It forwards only resolved identical SSA addresses and identical types within
one block. An earlier successful ordinary load/store establishes the bytes;
all writes clear the table, then an ordinary store establishes its own value.
No access moves across a block boundary, no first access or store is deleted,
and no alias-freedom or call-purity claim is inferred. Existing loads remain
pinned to GCM/DCE. Tests cover partial and unknown-alias writes, volatility,
atomics, calls, stack restore, intrinsics, result types, aliases created by GVN,
and native aliasing cases with restricted registers and both selectors.

On the Apple M5 Max application corpus, this eliminates 118/5,398 static Lua
loads, 480/22,431 QuickJS loads and 1,160/29,829 SQLite loads. Executable text
falls by 500, 1,776 and 5,308 bytes respectively (0.26% combined). Seven paired
runtime samples give a geometric mean ratio of 0.9951; individual intervals
generally overlap 1.0, so this does not establish a reliable runtime speedup.
Compilation takes 2.4% longer. Another instcombine after forwarding produces
ratio 0.9964 with 6.5% more compile time. Both variants remain opt-in; the
existing default pipeline is unchanged. These are static instruction counts,
not counts of dynamically executed memory operations.

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

## C frontend

`keel-c` preprocesses C source and compiles one translation unit to one module:
`keel cc in.c -o out.s`. `keel cc -E` prints the preprocessed text. `.i` inputs
and the library's `compile` API bypass preprocessing; the `preprocess` API
returns text that `compile` accepts. Line markers (`# 12 "file.c"`) carry source
files and lines through to diagnostics and `-g`; output whitespace and columns
need not match Clang.

**Native preprocessing.** Phase 2 line splicing precedes comment removal and
preprocessing tokenization. Macro expansion uses per-token hide sets, argument
prescan, stringification, placemarkers for empty pasted arguments, and rescanning
with the remaining input. Both C variadics and GNU named variadics are accepted;
GNU `, ## __VA_ARGS__` distinguishes missing and explicitly empty arguments.
Conditional expressions use 64-bit signed/unsigned preprocessing integers,
operator precedence, short-circuit evaluation and conditional-operator type
conversion. `defined` produced by macro expansion is accepted as Clang does.

Quoted includes search the physical including file's directory first, then
`-I` directories, explicit `-isystem` directories, embedded compiler headers,
and system directories. Angle includes omit the first step. Include origins
implement `include_next` and `__has_include_next`;
include guards and canonical-path `#pragma once` suppress repeated inclusion.
`#line`, `#error`, pragma forwarding, `_Pragma`, `__FILE__`, `__LINE__`,
`__BASE_FILE__`, `__FILE_NAME__`, `__INCLUDE_LEVEL__`, and `__COUNTER__` are supported.
Includes and macro argument nesting are limited to 200 levels, and each
expansion has a token/work limit to bound runaway expansion.

**Target environment.** `predefined_macros` generates
integer types, ranges, literal suffixes, printf formats, endian markers, atomic
constants, floating-point limits and data-layout macros for the two supported
LP64 targets. Integer macro families come from shared type descriptions. The
important platform differences include Darwin's `long long` `int64_t` versus
Linux's `long` `int64_t`, a `long` `intmax_t` on both targets, signed Darwin
`wint_t` versus unsigned Linux `wint_t`, and double-compatible Darwin versus
80-bit Linux `long double`. Linux long-double layout/limit macros do not add
support for evaluating long-double values in the frontend.

Compiler identity is deliberately separate: `__KEEL__` and `__KEEL_VERSION__`
identify Keel, with pinned Clang 18.0.0 and GCC 4.2.1 compatibility identities.
These are source-selection profiles, not promises of full compiler parity.
CPU/vector feature macros are not copied from the host. Standard capability
macros declare the lack of complex arithmetic, VLAs, and a supplied C threads
implementation. Native feature queries use a conservative registry of supported
builtins, features, and semantic attributes; unknown capabilities return zero.
The fixed-type and generic overflow-builtin registry shares the parser's
family recognition.
Ignored attributes such as `cleanup` are not reported as implemented.

The local Apple Clang 21 audit produced 482 Darwin arm64 and 407 Linux x86-64
predefined macros. Differential tests compare over 300 generated ABI/standard
macros against Clang for each target, explicitly selecting macOS 11.0 so the
deployment macros are independent of the host release. Architecture feature,
compiler identity, and capability macros are audited separately, rather than
requiring a byte-for-byte copy of that toolchain's complete macro dump.
`keel cc -E -dM` dumps final definitions, including source and command-line
changes; it also permits inspecting the environment for either target.

**Sysroots and deployment settings.** `--sysroot` (also `-isysroot`) selects
the target filesystem/SDK. Native lookup uses `<root>/usr/include`, preceded
by `<root>/usr/include/x86_64-linux-gnu` on Linux, and Darwin framework lookup
uses `<root>/System/Library/Frameworks`. An explicit root adds no host system
include directories. Explicit `-I`/`-isystem` paths are not rebased. Native Linux
uses `/` when no root is specified and the compilation target matches the
host. The macOS driver consults `SDKROOT`, then `xcrun --sdk macosx
--show-sdk-path`; the library API leaves SDK selection to its caller. An
explicit root avoids external discovery commands entirely.

The macOS deployment version defaults to 11.0, the first arm64 macOS release.
`-mmacosx-version-min` and `MACOSX_DEPLOYMENT_TARGET` select the deployment
macros; SDK selection does not silently change them. This currently controls
preprocessing; Keel still emits assembly and does not drive the linker, whose
deployment flags must be selected consistently by the caller.

**Compiler-owned headers.** `include_str!` embeds ten original C11 headers in
the compiler, so installation needs neither the source checkout nor a Clang
resource directory. The same files serve both targets; conditional ABI macros
select target differences. Supporting another target requires its ABI description,
frontend/backend support, and a matching platform sysroot, rather than a full
copy of these headers. `stddef.h` supplies target types, `offsetof`, `max_align_t`
and libc's partial `__need_*` inclusions. `stdarg.h` exposes the frontend's ABI
implementation of `__builtin_va_list` and variadic builtins. `stdint.h` and
`limits.h` wrap sysroot headers with `include_next` in hosted environments,
with standalone integer definitions otherwise. `float.h` uses the generated
format limits; `FLT_ROUNDS` currently describes the default rounding mode.
`stdbool.h`, `stdalign.h` and `stdnoreturn.h` provide the C11 spelling macros.
`stdatomic.h` maps the C11 atomic API to Keel's existing atomic builtins.
`ptrcheck.h` supplies Darwin's optional pointer-annotation fallback with bounds
checking reported unavailable. Headers that require a different pointer ABI
or vector intrinsics are not fabricated.

Libc headers (`stdio.h`, `stdlib.h`, `string.h`, `math.h`, `inttypes.h`, platform
`sys/*` headers, and so on), libraries, startup objects, and runtime helpers
remain the target SDK/toolchain's responsibility. Keel is not implementing a
libc. Removing Clang from C-to-assembly does not remove assembly/linking tools
from the complete executable build.

Preprocessing always uses Keel's generated target environment and embedded
compiler headers. `-D` and `-U` apply in command-line order after predefined
macros. The preprocessor library runs no external commands; the driver may
use `xcrun` only for SDK discovery when no explicit root or `SDKROOT` is given.
Clang is a development/test oracle and can still produce `.i` inputs outside
Keel. There is no runtime Clang probing option or fallback.

This deliberately impersonates Clang while compatibility grows. It does not
establish full Clang preprocessor parity or imply that Keel supports every
feature Clang advertises. Current gaps include trigraphs, universal character
names in identifiers, C23 `__VA_OPT__` and newer directives, date/time/timestamp
builtins, dependency output, complete framework/subframework lookup,
Clang-specific pragma semantics, and exact diagnostic compatibility. `#warning`
is currently accepted without a warning sink; most pragmas are forwarded to
the existing lexer, whose implemented semantic pragma is `pack`. The CLI uses
C11 mode; language-mode and wider compiler-driver flag support remain
future work. The application suite still shares externally Clang-preprocessed
inputs across compiler configurations for comparable compiler measurements.

- **Lexer → parser → IR.** C cannot be parsed without knowing which names
  are types (`T * x;` is a declaration if `T` is a typedef), so the parser
  keeps scopes; since it then knows every name's type, it type-checks as
  it goes and builds a typed tree in which every implicit conversion is an
  explicit `Cast` (promotions, usual arithmetic conversions, assignment,
  array and function decay). IR generation only follows the tree.
- **Locals.** A scalar local whose address is never taken, and that is not
  `volatile`, becomes a variable of the SSA builder; everything else gets
  a stack slot. A declaration without an initializer starts a new value
  (zero): C makes the object's value indeterminate each time the
  declaration is reached, so a local declared in a loop must not carry its
  value round the loop. It did at first, and Lua's interpreter loop
  carried 29 such `lua_Number`s as block parameters through every block,
  making Lua's `fib` benchmark 2.2× slower. That is LLVM's `mem2reg`, decided while parsing, which has
  seen every `&x` before IR generation starts. Loops seal their header
  after the back edge; labels (`goto`) are sealed at the end of the
  function. With `-g` every named local is in a stack slot instead, so a
  debugger can see it (see Debug info), and a struct parameter is copied
  from the caller's memory into its own.
- **Debug types.** Type checking compares `Ty`s, in which a typedef is
  the type it names, an enum is `int` and qualifiers are gone, as C's
  compatibility rules want. A debugger wants them back, so a declaration
  also records a `Sugar` (`types.rs`): a tree of the same shape that adds
  typedef names, enums and `const`/`volatile` where the declaration had
  them and is `Plain` below that; `debug.rs` reads the two together.
  Keeping the names in `Ty` instead would have made every comparison look
  through them. A compound statement and a `for` are scopes; only those
  that declare variables become debug scopes. `static` locals are
  globals at file scope.
- **Aggregates** are addresses: a struct-typed expression evaluates to a
  pointer to its bytes, assignment copies (with loads and stores up to 64
  bytes, `memcpy` beyond), and calls pass them `byval` and return them
  `sret`. The ABI only looks at a struct's size and whether it is a
  homogeneous float aggregate, so the layout on the signature is an array
  of its float type or of integers as wide as its alignment.
- **Initializers** are flattened to (offset, value) items: designators,
  brace elision and string literals are resolved while parsing. Locals are
  zeroed and then stored to; objects with static storage are evaluated to
  bytes plus relocations (`&x`, `"str" + 6`, function addresses).
- **Only what is used is emitted.** Headers declare hundreds of functions
  and define `inline` ones a file may never call; external definitions are
  always emitted and everything else only when something emitted refers to
  it. An inline-only definition becomes a local copy; any file-scope
  `extern` or plain non-inline declaration, before or after it, requests public
  emission unless explicitly `static`. Block declarations merge contracts but
  do not request public emission. See
  [function attributes](function-attributes.md#c-declarations-and-contracts).
- **Target facts** come from `Options::target`. Both targets are LP64 with
  signed `char`.
  - Apple arm64: `long double` is `double`. `va_list` is a `char *`, so
    `va_copy` is an assignment and `va_arg` works on a passed `va_list`.
  - x86-64 Linux: `va_list` is SysV's one-element array of
    `__va_list_tag`, so `va_copy` copies the struct. `long double`, like
    glibc's `__float128` and `_Complex`, is an opaque type: it has its
    size, so headers that declare it parse, but a variable or value of it
    is an error. x87's 80-bit arithmetic is not implemented, and SQLite is
    built with `-DLONGDOUBLE_TYPE=double`.

Beyond ISO C11 it accepts extensions used by Apple's headers and real GNU
C programs: `__attribute__((..))`, `#pragma pack`, `__asm("_name")` symbol
renames, C23 enums with a fixed type, zero-length arrays and empty structs,
block pointers (`^`,
as plain pointers), `__int128`, `typeof`, `__auto_type`, `__extension__`,
statement expressions, `a ?: b`, case ranges (`case 'a' ... 'z':`), range
designators (`[0 ... 9] = x`), computed `goto`, selected `__builtin_`
operations and `_Generic`. `setjmp` and friends are marked `returns_twice`.

### Extension compatibility

Supported builtins include `offsetof`, `types_compatible_p`, `choose_expr`,
`constant_p`, variadic access, `bswap16/32/64`, `clz/ctz/popcount` and their
`l`/`ll` forms, floating-point classification, `fabs`, `alloca` and
`frame_address(0)` and `debugtrap`. The fixed-type overflow and hint
builtins are described below. GCC/Clang atomic builtin families are
supported for selected scalar operations, with the limits below. A `__builtin_` name
with a declared library equivalent can also resolve to that function;
this is not blanket support for every builtin.

Explicit [`__builtin_memcpy`, `__builtin_memmove`, and `__builtin_memset`](memory-builtins.md)
lower to existing memory intrinsics without needing libc declarations.
Ordinary library names retain ordinary call semantics.

Some accepted extensions have deliberately limited behavior:

- `__builtin_expect` and `__builtin_expect_with_probability` return their
  first operand converted to `long`, evaluating both runtime operands
  once. Probability must be a finite constant expression between 0 and
  1 inclusive; it carries no runtime effects. No branch-likelihood
  information is retained.
- `__builtin_assume_aligned` returns its pointer as `void *` without
  adjusting it or checking alignment at runtime. Alignment must be a
  nonzero power-of-two integer constant. The optional integer offset is
  evaluated once. Proven alignment is retained on subsequent IR loads and
  stores through that SSA pointer; a constant offset reduces the guaranteed
  alignment, and an unknown offset makes no additional promise.
- `__builtin_constant_p` recognizes integer expressions the frontend can
  evaluate, rather than asking the optimizer whether a value is constant.
- `__builtin_object_size` and `__builtin_dynamic_object_size` always
  return the unknown-size value. Fortified memory builtins (`memcpy`,
  `memmove`, `memset`) have canonical prototypes and call checked external
  runtime functions with all four arguments, with or without declarations.
  Conflicting declarations are diagnosed. Linking requires those symbols. Other
  fortified `__builtin___*_chk` calls handled by the legacy fallback still
  become plain library calls and lose the supplied bounds check.
- `__builtin_unreachable` lowers to a trap, as `__builtin_trap` does;
  it does not introduce an optimizer assumption.
- GNU `packed` and `aligned` control aggregate and member layout.
  GNU `aligned` and C11 `_Alignas` also control local, static, global and
  thread-local object alignment, with power-of-two requests up to 4096.
  `_Alignas(0)` has no effect; nonzero `_Alignas` cannot weaken natural
  alignment. Expression `__alignof__` queries retain object/member
  alignment without changing type compatibility. Explicit member alignment
  overrides packing; `#pragma pack` caps member alignment.
- Alignment-changing typedef attributes, alignment attributes on functions,
  parameters and type names, and by-value aggregates aligned above 16 bytes
  are diagnosed. On x86-64, by-value aggregates with unaligned fields are
  also diagnosed because their SysV MEMORY classification cannot be
  represented by the current IR signature. Packed bit-fields requiring
  partial or crossing storage-container accesses, unaligned atomic members
  and unaligned pointer relocations are diagnosed rather than emitted.
- Attribute syntax is accepted broadly. `returns_twice` affects calls;
  `_Noreturn` and GNU `noreturn` terminate direct calls with a trap if the
  callee unexpectedly returns, exposing the absent continuation to CFG
  simplification. These contracts are merged across function redeclarations.
  `inlinehint`, `alwaysinline`, `noinline`, `noreturn` and `nounwind` are
  transported on direct functions; GNU `nothrow` supplies `nounwind` promise
  metadata without C++ `noexcept` termination or call/invoke EH rewrites.
  [Attribute ownership, merging and emission](function-attributes.md) distinguish
  named declarations from returned-function types and indirect calls.
  GNU function-pointer `noreturn` attributes are not represented.
  Selected unsupported linkage and lifetime attributes, including `cleanup`,
  `weak`, `alias` and `constructor`, are diagnosed. Only default visibility
  is accepted. Other attributes, including `vector_size`, remain ignored;
  see the [C compatibility audit](c-frontend-coverage.md) for the exact boundary.
- `inline` affects emission and supplies `inlinehint` to the bounded
  private-only [inliner](#bounded-function-inlining). Block-pointer declarators (`^`) are
  parsed as pointers; block literals and the Blocks runtime are not
  implemented.
- `__has_builtin`, `__has_feature` and `__has_attribute` describe Keel.
  Externally Clang-preprocessed files instead reflect Clang's feature answers
  and can enable unsupported code.

### Optimization information at the C/IR boundary

The C frontend supplies all three application suites; Lua is C input, not a
separate language frontend. An audit of the pinned Lua 5.4.7, QuickJS
2024-01-13 and SQLite 3.46.1 sources found these optimization annotations:

- Lua's `llimits.h` defines `l_noret` using GNU `noreturn`, used for error
  paths such as `luaD_throw`. Its `luaconf.h` defines `luai_likely` and
  `luai_unlikely` using `__builtin_expect`, and `LUAI_FUNC` requests internal
  visibility on supported targets.
- QuickJS's `cutils.h` defines `likely`/`unlikely`, `force_inline`,
  `no_inline` and packed integer access wrappers. Formatting and unused
  annotations are diagnostic rather than optimization contracts.
- SQLite's amalgamation defines `SQLITE_INLINE`/`SQLITE_NOINLINE`, and
  requests eight-byte alignment for its small memory pool when enabled.

Lowering preserves signed comparison/division/shift and integer extension
semantics, ABI extension and aggregate layouts, immutable global data,
volatile memory operations, atomic orderings, object layouts/alignment,
`returns_twice`, direct function attributes, and direct `noreturn` control flow.
Access alignment is computed from actual stack slots, global declarations,
explicitly rounded local addresses, `__builtin_assume_aligned`, and pointer offsets. It is
written into `MemoryFlags`, so IR printing/parsing and downstream passes
retain it. Copy and initializer chunks get alignment at their actual byte
offsets, not the alignment of the aggregate base. Alignment never implies
`notrap`, immutable memory or alias freedom. Unknown pointers and unresolved
block parameters conservatively have alignment one; packed accesses are not
strengthened merely because their C scalar type normally has stricter
alignment. Alignment facts can therefore be lost across control-flow merges,
memory round trips and function boundaries.

Remaining information gaps require IR contracts and consumers, rather than
mapping source annotations onto unrelated flags:

- Branch likelihood and explicit probabilities are still discarded by the
  hint builtins. Preserving these on CFG edges and using them in block layout
  is a concrete next step for Lua and QuickJS. Probabilities are advisory;
  probability zero or one must not make an edge unreachable.
- `hot`/`cold` and visibility remain unrepresented. Inline policies now
  reach the private-only inliner under the
  [function attribute contract](function-attributes.md); public/interposable
  definitions retain calls.
- `restrict` and C effective-type alias information are discarded. Scoped
  alias contracts and alias analysis are needed before memory optimizations
  can use them. A pointer to `const` does not establish immutable storage.
- GNU `pure`/`const`, `malloc`, `alloc_size`, `alloc_align`, `nonnull`,
  `returns_nonnull` function contracts are ignored. Calls remain
  conservative memory/lifetime barriers. Declaration attributes must be
  distinguished from Clang's nullability spellings, which are not equivalent
  optimizer promises. Allocation contracts also need object/lifetime modeling.
- Signed C overflow and in-bounds pointer arithmetic have no corresponding
  IR flags; integer arithmetic remains wrapping. Do not substitute trapping
  arithmetic, `notrap` or unrestricted reassociation for those contracts.
- Object-size builtins still return unknown bounds, and unrelated fortified
  calls lose their supplied checks as described above. This is a compatibility gap,
  not evidence that the checks are redundant.

The direct-call and access-alignment changes expose facts without claiming a
suite speedup. Assess future contracts with IR regressions, both target
backends, the application correctness suites and paired measurements.
The alignment-offset interpretation follows [GCC's builtin documentation](https://gcc.gnu.org/onlinedocs/gcc/Other-Builtins.html).

GNU `__auto_type` follows the single-identifier and initializer scope
rules in [GCC's documentation](https://gcc.gnu.org/onlinedocs/gcc/Typeof.html).
Clang accepts some additional forms and rejects some GNU shadowing cases;
the frontend tests check those scope rules separately from the programs
compared against Clang.

### Implementation details

- **Debugger entry.** `__builtin_debugtrap()` emits the resumable
  `debugtrap` intrinsic (`brk #0xf000` on Apple ARM64, `int3` on x86-64).
  Instructions after it remain reachable, including with `-O`. A
  debugger or signal handler can resume execution; without one it normally
  raises SIGTRAP. This differs from the terminating `__builtin_trap`.
  See [Clang's builtin documentation](https://clang.llvm.org/docs/LanguageExtensions.html#builtin-debugtrap).
- **Checked arithmetic.** The fixed-type `__builtin_sadd/ssub/smul_*` and
  `__builtin_uadd/usub/umul_*` overflow families are supported for `int`,
  `long` and `long long` (the `l` and `ll` suffixes). Operands are converted
  as for the builtin's prototype; the wrapped result is stored through
  the third argument and the overflow flag is returned as `_Bool`. Every
  operand is evaluated once, and the store remains even when the flag is
  unused. Generic `__builtin_add_overflow`, `__builtin_sub_overflow` and
  `__builtin_mul_overflow` support mixed operand/destination integer types
  through 128 bits; `_overflow_p` remains deferred. See
  [generic overflow](generic-overflow-builtins.md) for conversions, side effects
  and validation. The largest 128-bit multiply fixture has a measured assembly
  size cost; no runtime speedup was measured.
- **Computed goto** without an indirect branch in the IR: `&&label` is
  the label's number in its function (from 1), and every `goto *p` jumps
  to one shared dispatch block, passing that number as a block argument.
  Its `switch` over the labels whose address is taken becomes one jump
  table. When every computed goto in a function is a direct subscript of
  the same immutable, nonvolatile pointer table, the frontend can instead
  switch on the evaluated subscript and map each entry to its label. This
  removes the numeric-label lookup without changing label values, ordinary
  table uses, or the shared SSA merge. The table must have an exact supported
  constant layout, at most 4096 entries and at most twice as many entries as
  address-taken labels. Duplicate/designated writes and implicit zeros retain
  their initializer semantics; zero destinations trap. Copied, mutable,
  volatile, mixed-table and computed-offset dispatch retains the numeric
  lowering for the whole function.
  Locals merge through SSA parameters at dispatch. Sharing avoids
  duplicating the table and its edge copies at every site: QuickJS and Lua
  otherwise spend far more code on dispatch than on their handlers. The
  dispatch block is emitted after the body, before sealing all blocks, so
  every dispatch edge and label is known. GNU only defines jumping to a
  label of the same function, where a number is as good as an address;
  and being constants, the numbers can fill static dispatch tables, and
  `&&a - &&b` offsets work. An indirect branch in the IR would need edge
  copies for block parameters on every possible target at once. Table
  fusion retains shared-state moves/stores and the backend's checked
  indexed branch; it does not provide direct threaded dispatch.
- **Atomics.** `_Atomic(T)` is a type of its own (`Ty::Atomic`), for
  integers and pointers: the one place a value is read from an object
  (`rvalue`) makes reading one an atomic load, and assignment, `+=` and
  `++` atomic stores and read-modify-writes, all seq_cst. Treating it as
  `T` would compile plain accesses to non-atomic ones silently. The
  builtins, GCC's `__atomic_*` and `__sync_*` and clang's `__c11_atomic_*`
  (which `<stdatomic.h>` expands to), are put in one shape and become the
  IR's atomic instructions; `compare_exchange` is a `cas` whose old value
  is stored back to `*expected`. Atomic pointer `++` and `--` scale by the
  pointee's size and use an integer RMW on the 64-bit address, converting
  the old (postfix) or new (prefix) address back to a pointer. Pointer
  compound assignments (`+=` and `-=`) are not supported yet.
- **Type inference.** GNU `__auto_type` declares one variable with a plain
  identifier and an initializer. The initializer determines its type,
  with array/function decay and atomic loads as for an ordinary value.
  The name enters scope after the initializer, so a same-named outer
  variable remains visible while it is parsed. Explicit qualifiers and
  storage classes apply to the inferred type; structs are copied as with
  explicitly typed declarations. Bit-fields and incomplete values cannot
  supply the type, and inference is not allowed in parameters, members,
  typedefs or type names.
- **Constants** are `i64`s, sign extended to wider types, so an unsigned
  64-bit constant above `i64::MAX` converted to `__int128` is not folded
  but extended at run time: folded, it came out negative, which broke
  QuickJS's decimal arithmetic (multiplying by `17014118346046923173`).

### Remaining coverage

Not supported: `_Complex` arithmetic, variable-length arrays, K&R
definitions, nested functions, `_Atomic` structs and floats, the generic
(non-`_n`) `__atomic_*` forms, vector types/intrinsics, and inline assembly
statements. The ARM64 IR backend has inline assembly; the C frontend does
not pass it through, and the x86-64 backend does not support it yet.

Useful candidates for small follow-up patches, rather than current support:

- **Hint optimization:** the alignment and branch builtins preserve their
  value and argument-evaluation behavior, but optimization from their
  metadata would require additional IR and optimizer support. See
  [GCC's builtin documentation](https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Other-Builtins.html).

Linkage attributes (`weak`, `alias`, `visibility`), scope-exit `cleanup`
and C inline assembly are useful larger additions. Attributes whose
semantics are required by a program must be implemented or diagnosed;
silently parsing them does not establish compatibility.

**Testing.** Test programs use external Clang preprocessing and Keel's native
preprocessing. Each compiles with Keel before and after optimization and must
behave like Clang. Native
fixtures execute on the host SDK; empty/synthetic sysroots cover headers and
ABI macros for both targets, without claiming Linux SDK runtime validation
when running on macOS.
Differential raw-token comparisons check focused preprocessing cases against
Clang; compiler-header/feature differences are checked through ABI assertions
and program execution rather than identical system-header token streams.
Focused cases cover
macro rescanning, variadics, pasting, stringification, conditional expressions,
include search order and error locations. Programs print what they compute
instead of hard-coding answers.
The overflow fixture checks all 18 fixed-type forms at integer boundaries,
the hint fixture checks argument side effects and returned values, and the
debugtrap fixture catches SIGTRAP and resumes twice. Separate frontend
tests verify and generate assembly for both targets, before and after
optimization. The signal-based breakpoint test does not require LLDB;
debug information is checked by the separate debugger tests.
The suite in `suite/` (run by `cargo suite`, see
[suite/README.md](../suite/README.md)) builds Lua 5.4.7 with POSIX extras
and computed gotos and runs its portable test mode, builds thread-safe
SQLite 3.46.1 and compares the checked-in query workload with Clang's
build, and builds QuickJS 2024-01-13 with BigNum and runs the commands
listed in its manifest. The default test configurations are keel with
and without `-O`. These configured workloads found bugs the small
programs missed (an array of pointers to `const` placed in read-only
memory, a variable used in its own initializer, `#pragma pack`, the
`__int128` constant, a function too big for conditional branches).

Deterministic indexed GNU and Darwin/BSD static archives are available through
`keel archive` and the library writer; see [static archives](static-archives.md)
for usage and validation. Static packaging is distinct from DSO creation.
The Darwin consuming linker's common-only extraction limitation remains.

The frontend cloning investigation rejected the scoped
expression change and implemented no performance gain. Initializer clone
incidence motivates a separate future session.

## Assembler

The first assembler target is A64 for Apple arm64 macOS. `keel-asm` separates
instruction encoding, assembly syntax, and the Mach-O object container. The
library can return encoded text and symbols or a complete relocatable object;
`keel asm input.s -o output.o` writes that object. Apple’s linker remains the
linker for now.

**In-process machine interface.** `keel-codegen` and `keel-asm` both depend on
`keel-asm-ir`; neither production crate depends on the other. This shared crate
owns `Program`, `Item`, `Instruction` and generated AArch64 `Opcode`/`FieldId`
identities, plus separate x86 opcode/operand identities. It has no object writer;
its x86 module also provides a source parser for standalone Intel syntax.
AArch64 opcode indices and the assembler's encoding records are generated
together from the pinned Arm
archive and tested for exact correspondence. These indices are not a stable
serialized format. Retain its bundled Arm license when distributing the tables.

`keel_codegen::compile_program[_with_pipeline]` produces this representation;
`keel_asm::assemble_program` returns the usual owned `Assembly`, and
`assemble_program_object` writes Mach-O bytes in memory. `encode_instruction`
can encode one exact instruction separately. Field count, duplicate/missing
fields, range, reserved bits and CPU feature checks still apply, but the native
path directly indexes the encoding table and does not parse operands. Small
field lists live inline. The APIs use ordinary owned values with no global
state, so a caller can keep the entire toolchain in one process.

Native labels and symbolic instructions share source assembly's layout, fixup,
relocation, linker-atom and unwind machinery. `SymbolicInstruction` carries
conditional/compare branches, local ADR references and page/GOT/TLVP references;
`Directive` carries sections, alignment, symbols, bytes, integer expressions,
LEB128, zero-fill, source locations and CFI. Expressions own their symbols and
numeric addends. Symbol differences and symbolic LEB128 use the same fixups and
iterative layout as source assembly. Typed CFI and source CFI apply the same
frame rules. Data/DWARF emission uses a common metadata sink for text and native
output. The x86 ELF backend uses the same typed metadata boundary, including
ELF sections, symbol types/sizes and DTPOFF64 debug references.

The AArch64 compiler now emits every generated instruction and metadata directive
through these typed paths. Register widths and memory operands are numeric before
encoding selection; no instruction text is formatted or reparsed on the native
path. Instruction/allocation comments are omitted. Functions at least 1 MiB in
estimated code size expand conditional branches into inverse-condition skips
and B instructions without re-emitting the function as text. Text emission
retains the equivalent source relaxation rule.

Only user inline assembly creates text items, whose statements do not continue
across item boundaries. The compiler's generic formatting sink rejects writes in
structured mode, and fixture tests require zero text bytes when the input has no
inline assembly. Inline assembly requiring macro expansion uses an explicit
validated source fallback; generated words, symbols and metadata are validated
before materialization. This fallback preserves the existing macro workflow.

`compile` and `cc --emit-object -o output.o` compose these stages without
intermediate assembly files. Existing assembly output remains the default;
`--emit-ir` is incompatible with `--emit-object`. The x86 Linux path also emits
typed instructions and metadata directly, with no intermediate assembly text.
Its standalone syntax remains Intel, and its compiler rejects inline assembly.
[Packed AVX arithmetic](x86-avx-subset.md#packed-arithmetic-extension) is opt-in
with the existing `--target-features avx`, supporting 128-bit `f32x4`/`f64x2`
arithmetic. It adds no AVX2, AVX-512 or wider-vector support.
The owned program retains its items through assembly;
a future streaming sink could avoid retaining that intermediate representation.

Source accepts LF, CRLF and bare CR line endings, Darwin `;` and `//` comments,
and C-style `/* ... */` comments. A block comment acts as whitespace, including
within an operand across physical lines; diagnostics retain physical source
lines. Mnemonics, registers and shift names are case-insensitive. Integers may
be decimal, hexadecimal (`0x`), binary (`0b`) or leading-zero octal. `x31`/`w31`
are zero-register spellings, not stack-pointer aliases, and remain invalid in
operands that reject the zero register. Numeric local labels (`1:`/`1f`/`1b`)
are not yet supported. AT&T/Intel x86 syntax and ELF objects are outside this
AArch64 Mach-O target's scope.

`keel asm --cpu apple-m5 one.s two.s --out-dir objects` writes one independently
assembled object per input in one process, amortizing process startup. Output
names use the input stem and `.o`; collisions and outputs aliasing inputs are
rejected before writing. Processing is serial and stops on the first error;
completed objects remain available. Each input's mapping and assembly state
are released before processing the next. Input files must stay unchanged while
being parsed. This interface has no persistent service or daemon state.

**Standalone source expansion.** Named `.macro` definitions end at `.endm` or
`.endmacro`; parameters are plain unique names and invocation arguments are
comma-separated positional values. `\name` substitutes a parameter and `\()`
separates it from following text for concatenation. Quote arguments containing
spaces or commas. `.rept`/`.endr` supports nested repetition with literal counts.
`.if`, `.ifeq`, `.ifne`, `.ifgt`, `.ifge`, `.iflt`, `.ifle`, `.elseif`, `.else`
and `.endif` support integer-literal tests, including literals substituted by
macros. Each invocation or expansion directive must occupy its own statement;
label-prefixed macro calls are not supported. Keyword/default/required/variadic
parameters, Darwin `$0` positional parameters, `\@`, symbolic/arithmetic
conditions, `.ifdef` and filesystem `.include` are unsupported.

Expansion is local to an assembly call and begins only when the ordinary parser
reports an unsupported expansion directive. Compiler output without these
directives avoids a preprocessing pass. Diagnostics and instruction metrics
retain original definition lines; expanded errors also name the invocation line.
Nesting is limited to 64, expansion work to 100,000 steps and expanded output to
8 MiB. These limits apply to sources requiring expansion; ordinary source has no
such size limit. The pure string assembly API does not read additional files.

**Generated encoding tables.** `crates/keel-asm/isa/a64.json` contains selected
instruction names, conditions, and encoding records copied from Arm’s
BSD-licensed AARCHMRS December 2025 release (schema 2.7.4). The source URL,
archive SHA-256, upstream metadata, and license notice are retained. The
subset currently has 1647 instruction/alias forms. Generated requirements retain
Arm’s `FEAT_FP`, `FEAT_AdvSIMD`, `FEAT_LSE`, `FEAT_SVE`, and `FEAT_SME`
availability checks, plus the numeric feature predicates described below;
the fixed Apple arm64 target enables only FP, AdvSIMD,
and LSE. Optional features require an explicit library opt-in described below.
Encoding conditions are distinct from feature requirements: for example, byte register-offset forms exclude
`option == 0b011`, which has a separate instruction form. The generator
handles conjunctions, disjunctions, negation, equality/inequality, and masked
set membership explicitly and rejects unknown features, fields, or AST forms
instead of discarding them. `SMSTART`/`SMSTOP` records retain the upstream
alias conditions with the parent `MSR_SI_pstate` encoding; `encoding_source`
records that provenance. Their feature and operand predicates are checked
at encoding time, including the parent instruction's exclusion predicate.

The generator extracts fixed bits, masks, and variable field layouts from
these records; the assembler does not duplicate opcode constants. Partially
variable fields retain their fixed-bit constraints. Operand interpretation
remains handwritten: register roles, immediate ranges, shifts, aliases, and
PC-relative displacements need rules beyond the bit layout.

Encoding-name dispatch is generated alongside the records as a Rust string
`match`, replacing a linear scan through the encoding table. The records live
in one static table; every generated arm returns the corresponding record.
Field range, reserved-bit, and feature checks still run after lookup. A unit
test checks dispatch for every record and rejects unknown names.

The parser borrows already-lowercase tokens and uses inline storage for the
first eight operands, falling back to a vector for longer lists. Temporary
encoding names are formatted into a 128-byte stack buffer with a heap fallback;
branch names and memory-transfer tags use static strings where possible.
These capacities do not impose syntax limits. Uppercase normalization,
diagnostics, and the encoding validation rules are preserved.

Each assembly layout pass also owns a bump arena for fixup queues, label
ordering, symbol-name sorting, debug rows/frames and CFI rules/events. Symbol
bookkeeping borrows source names; generated names live in the arena. Local
collections drop their elements before the arena, including on errors, and
each symbolic LEB pass destroys its own scratch. Escaping assembly buffers,
names and errors remain independently owned. Ordered-map nodes and some
parser/Mach-O scratch still use the system allocator; the parser module
documents this allocation boundary and arena growth costs.

Line and comment scanning share one structural scan. On little-endian AArch64
targets with NEON enabled, a 16-byte block generates masks for newline,
semicolon, slash, quote and backslash; subsequent lines reuse unconsumed mask
bits. Full-line comments skip to newline. Other targets and short tails use
scalar masks. The scanner also recognizes CR line endings. Block comments
allocate joined statement text in the pass arena only when needed; ordinary
statements retain borrowed slices. Quoting state resets at statement endings;
newlines inside block comments count toward physical lines without splitting
the surrounding statement. UTF-8 strings and comments remain accepted.

Normal Cargo builds use the checked-in Rust tables and need no network,
Python, or ISA download. To regenerate, with Python 3 and rustfmt available:

```sh
python3 crates/keel-asm/tools/generate.py
python3 crates/keel-asm/tools/generate.py --check
```

To reproduce the subset from the original archive:

```sh
curl -L --fail -o /tmp/aarchmrs.tar.gz \
  https://developer.arm.com/-/cdn-downloads/permalink/Exploration-Tools-OS-Machine-Readable-Data/AARCHMRS_BSD/AARCHMRS_OPENSOURCE_A_profile_FAT-2025-12.tar.gz
python3 crates/keel-asm/tools/generate.py --archive /tmp/aarchmrs.tar.gz --check
```

The importer verifies SHA-256
`4dc5da62a5c856d7b1086b895075f54807f821ea21a333049cb0f40f9479cecc`
before reading the archive. Arm-derived data and tables carry BSD-3-Clause
terms; retain `crates/keel-asm/isa/LICENSE-ARM` with distributions.

**Integer arithmetic and control flow.** `movz`, `movk`, `movn`; `add`, `sub`, `adds`, `subs` with
immediates or shifted registers; `cmp`, `cmn`; register `mov` (including SP)
and single-instruction immediate `mov`; `b`, `bl`, `ret`, and `nop`. Both 32-bit
and 64-bit forms are supported where applicable. Move-wide instructions
accept `lsl` in 16-bit steps; add/sub immediates accept `lsl #0` or `lsl #12`;
shifted registers accept `lsl`, `lsr`, and `asr` within the register width.
Add/sub also supports extended-register forms, including SP arithmetic,
with `uxtb/uxth/uxtw/uxtx` and `sxtb/sxth/sxtw/sxtx` and shifts 0..4.

Logical operations include `and`, `ands`, `orr`, `eor`, `bic`, `bics`, `orn`,
and `eon` with shifted registers (`lsl/lsr/asr/ror`). `and/ands/orr/eor`
also accept A64 logical bitmask immediates: repeated, rotated runs of ones,
excluding zero and all ones. Signed immediates and hexadecimal bit patterns
are range-checked against W/X width. `mov #imm` chooses `movz`, `movn`, or
`orr` as a single instruction; arbitrary multi-instruction constants must
still be written as `movz`/`movk` sequences.

Other integer operations are `adc/adcs/sbc/sbcs`; `mul/mneg/madd/msub`,
`smulh/umulh`, `smull/umull/smaddl/umaddl/smsubl/umsubl`; `sdiv/udiv`; register and
immediate `lsl/lsr/asr/ror`; `clz/cls/rbit/rev/rev16`, and X-only `rev32`.
Bitfield forms include `sbfm/ubfm/bfm/extr`, `bfi/bfxil/ubfx/sbfx`,
`sxtb/sxth/sxtw`, and W-only `uxtb/uxth`. `neg/negs/mvn/tst` aliases are
supported. Shift and bitfield bounds are checked before encoding.

Conditional operations include `csel/csinc/csinv/csneg`,
`cset/csetm/cinc/cinv/cneg`, and `ccmp/ccmn` (register or 5-bit immediate,
with 4-bit NZCV). Conditions accept the usual names and `hs/cs`, `lo/cc`
synonyms. Inverting aliases reject `al/nv`; canonical forms accept them.
Branches include `b.<condition>`, `cbz/cbnz`, and `tbz/tbnz`, with checked
signed byte-displacement ranges of ±1 MiB and ±32 KiB respectively. Test
branches allow X names for low bits, but W names require bit indices <32.
Out-of-range branches are errors; the assembler does not relax them.
`br/blr` require X registers; `brk` and `svc` take a 16-bit immediate.
`ngc`/`ngcs` lower to SBC/SBCS with a zero-register first input, for W/X
operands including zero registers.

**Floating-point and population count.** Scalar S/D forms include
`fmov/fabs/fneg/fsqrt/frintn/frintp/frintm/frintz/frinta/frintx`,
`fadd/fsub/fmul/fdiv/fmin/fmax/fminnm/fmaxnm`, `fmadd/fmsub/fnmadd/fnmsub`, `fcmp/fcmpe`
(register or `#0.0`), and `fcsel`. `fcvt` converts S ↔ D; `scvtf/ucvtf`
and `fcvtzs/fcvtzu` convert W/X ↔ S/D. S/D fixed-point GPR conversions
remain unsupported in this baseline path.
Optional FP16 arithmetic and H/GPR fixed-point conversions are described below. `fmov` also supports S/W and D/X bitwise transfers and
all 256 scalar FP immediate encodings (zero needs a register transfer).

Keel’s scalar popcount lowering uses the supported SIMD `cnt` (8B/16B)
and `addv` reductions (8B/16B, 4H/8H, 4S). `movi` supports all six modified
immediate forms: byte replication (8B/16B); halfwords (4H/8H) with LSL #0/#8;
words (2S/4S) with LSL #0/#8/#16/#24 or MSL #8/#16; scalar D and vector 2D
64-bit masks whose bytes are each 00 or FF. Byte and 64-bit mask forms take
no shift operand.

**Baseline NEON coverage.** `crates/keel-asm/isa/neon.txt` inventories all
519 baseline Advanced SIMD encoding records in the pinned Arm archive:
`asimd*`/`asisd*` classes whose feature predicates require only AdvSIMD.
The checksum-verified importer checks that this inventory exactly matches the
upstream records. Scalar SIMD and structured memory are included. This is
the baseline NEON milestone; it does not claim optional M5 numeric, crypto,
or scalable extensions.

The parser implements all 519 records, including legal arrangements and
high-half variants. Coverage includes:

- Integer arithmetic, comparisons (register and zero), logical operations,
  halving/rounding/saturating arithmetic, absolute differences, polynomial
  byte multiplication, pairwise arithmetic, widening/narrowing, and reductions.
- Register/immediate shifts, narrowing/widening shifts, lane-indexed
  multiply/accumulate, lane duplication/insertion/extraction, MOV aliases,
  ZIP/UZP/TRN permutations, EXT, and TBL/TBX register tables.
- S/D vector FP arithmetic, comparisons, reciprocal estimates/refinement,
  rounding, integer/fixed-point conversions, pairwise arithmetic/reductions,
  indexed multiplication, precision widening/narrowing (including half-format
  conversion), vector FP immediates, and MOVI/MVNI/ORR/BIC modified immediates.
- Full-vector LD1/ST1 lists of 1..4 registers; interleaved LD2–LD4/ST2–ST4;
  lane loads/stores; LD1R–LD4R replication. Each supports base-only and
  immediate/register post-index forms. Lists are consecutive modulo 32 and
  accept explicit lists or start-end ranges. Immediate post-index amounts
  must match the number of bytes transferred; offsets/writeback-pre-index
  are not valid structure forms.

Operand validation rejects reserved sizes, scalar/vector mismatches, invalid
lane indices, nonconsecutive lists, and invalid shift/multiplier registers.
The polynomial widening form is restricted to baseline byte elements;
64-bit polynomial multiplication requires the later crypto extension phase.
Optional FP16 arithmetic, BF16, dot products, RDM, and complex arithmetic
are selected through the numeric features below; default checks remain unchanged.

`tests/neon-cases.txt` names one source case for every inventory entry. A unit
test requires an exact inventory match and verifies each assembled word
against that Arm record's fixed-bit mask. Clang independently checks all 519
cases, while additional differential tests enumerate arrangements, lane
indices, table/list lengths, high-half variants, and boundary shifts. Native
tests check lookup/permutation, signed widening and shifts, saturating
narrowing, structured memory, FP/integer conversions, indexed multiplication,
and widening reductions. This establishes encoding-form coverage, not an
exhaustive test of every operand combination. Regenerate the inventory cases:

```sh
python3 crates/keel-asm/tools/neon_cases.py
python3 crates/keel-asm/tools/neon_cases.py --check
```

**M5 numeric extensions.** `TargetFeatures` provides independent opt-ins
`with_fp16/with_fhm/with_bf16/with_dotprod/with_i8mm/with_rdm/with_fcma`,
plus `with_jscvt` and `with_frintts`. `TargetFeatures::apple_m5()` selects the
baseline, these nine numeric features, SME/SME2, and the optional SME
FP64, I16-to-I64, non-widening FP16, and non-widening BF16 types; it does not enable ordinary
SVE or imply implementation of other M5 instruction families. This follows
LLVM's [M5 feature definition](https://raw.githubusercontent.com/llvm/llvm-project/main/llvm/lib/Target/AArch64/AArch64Processors.td).
Existing APIs and CLI assembly default to the fixed Apple baseline.
The explicit command-line selection is:

```sh
keel asm --cpu apple-m5 input.s -o output.o
```

`--cpu baseline` selects the baseline explicitly; unknown CPU names are errors.
CPU selection only affects assembly and does not change compiler codegen,
probe the host, or enable ordinary SVE.

`isa/m5-numeric.txt` inventories 217 non-scalable records from the pinned
archive whose feature predicates select these extensions without requiring
other optional features. The importer checks that inventory against the
archive and retains all feature conditions. The implementation covers all
217 records:
SDOT/UDOT/USDOT/SUDOT, SMMLA/UMMLA/USMMLA, BF16 conversion/dot/matrix and
bottom/top multiply-accumulate, SQRDMLAH/SQRDMLSH, FCMLA/FCADD, widening
FP16 FMLAL/FMLSL variants, FJCVTZS, and FRINT32/64Z/X. Indexed forms validate
group sizes, lane bounds, restricted multiplier registers, and rotations.
H-element complex arithmetic additionally requires FP16. FP16 support includes
scalar and vector arithmetic, rounding, comparisons/conditional comparisons,
select, fused multiply-add, reciprocal estimates/refinement, reductions,
indexed arithmetic, immediates, bit transfers, and integer/fixed-point
conversions. Shared SIMD fixed-point encodings additionally check FP16 for H
elements; GPR fixed-point scales use the W/X width (1..32/64), while H/H scales
use 1..16. H↔S/D format conversions use their upstream baseline FP requirement,
without silently enabling FP16 arithmetic.

`tests/numeric-cases.txt` names a case for each of the 217 records. Unit tests
require exact inventory coverage, check each selected Arm fixed-bit mask, and
reject every case under default features. Clang compares all cases plus
arrangements, lane boundaries, rotations, conversion scales, conditional
codes, and all 256 FP16 immediate values. Native tests verify signed dot and
matrix products, RDM rounding/saturation, BF16 dot products, widening FP16,
complex multiplication, JavaScript conversion, bounded rounding, scalar and
vector FP16 math, scaling, and bit transfers. The inventory covers non-scalable
forms only; SME/SME2 numeric forms belong to the scalable phase. Regenerate:

```sh
python3 crates/keel-asm/tools/numeric_cases.py
python3 crates/keel-asm/tools/numeric_cases.py --check
```

Clang comparisons use the explicit `-mcpu=apple-m5` reference target. Native
numeric tests query each required `hw.optional.arm.FEAT_*` flag before
execution. This session's M5 Max advertises all nine. Encoding selection
never probes the host or silently changes the default target.

**Optional SVE/SME coverage and Apple availability (2026-10-15).** NEON
(Advanced SIMD) uses fixed-width V registers; SVE uses scalable Z/P registers;
SME adds streaming execution mode and ZA matrix state. They are distinct
capabilities. LLVM's [Apple processor definitions](https://github.com/llvm/llvm-project/blob/main/llvm/lib/Target/AArch64/AArch64Processors.td)
include NEON for M1–M5, SME/SME2 for M4/M5, and explicitly note that M4 lacks
ordinary SVE. Do not infer optional features merely from an Armv9 label.

Apple's [XNU SME documentation](https://github.com/apple-oss-distributions/xnu/blob/main/doc/arm/sme.md)
says current Apple implementations expose SVE registers/instructions only in
streaming mode. XNU preserves that state across kernel entry and context
switches. SM/ZA are separate state bits; mode transitions clobber vector and
predicate registers, and many NEON instructions are illegal while streaming.
SME therefore does not imply non-streaming SVE availability. CPU/OS support
must be checked at runtime; [Arm's macOS validation guide](https://learn.arm.com/learning-paths/laptops-and-desktops/kleidicv-on-mac/run-test-2/)
uses `sysctl hw.optional.arm.FEAT_SME` for this purpose.

`assemble`/`assemble_object` and the CLI default to the fixed baseline. Library
callers may use `assemble_with_features`/`assemble_object_with_features`
with `TargetFeatures::default().with_sme()` or `.with_sve()` (or both).
These flags permit encodings only; they neither probe hardware nor insert
state transitions, change Mach-O CPU subtypes, or promise execution support.
SME does not set the SVE flag; Arm's shared `SVE || SME` requirements accept
either independently. There is no global mutable feature selection. Callers
manage mode, calling convention, and ZA lifetime, including preserving any
live state around transitions.

The first scalable increment supports:

- Unpredicated `add/sub` on Z B/H/S/D elements; `and/orr/eor` on Z D elements;
  `fadd/fsub/fmul` on Z H/S/D elements.
- `ptrue` on P0–P15 B/H/S/D with named or raw 5-bit patterns; `pfalse` with
  `.b`; signed 8-bit `dup`, optionally shifted by 8 for H/S/D; `lastb` from
  Z to W/X with P0–P7; `rdvl` with signed 6-bit multiplier;
  `cntb/cnth/cntw/cntd` with patterns and multipliers 1..16.
- SME `smstart/smstop` (both state bits by default, or `sm`/`za`), `zero {za}`,
  and FP32 `fmopa/fmops` with ZA0–ZA3.S and P0–P7/m.

Clang 21 differential tests use local reference flags `-march=armv8-a+sve`
and `-march=armv8-a+sme`; no default target features are broadened. Native
SME testing first checks the OS flag, then runs a leaf with balanced mode
transitions and preserved D8–D15. On this session's Apple M5 Max / Darwin
27.0.0, SME and SME2 flags were 1; NEON and streaming SME tests passed.
Non-streaming SVE was not executed.

The matrix increments cover 123 selected Arm records (two overlap the initial
FP32 forms), inventoried in `isa/sme.txt` with one named encoding case per
record in `tests/sme-cases.txt`. `tools/sme_cases.py --check` reproduces the
cases; the regular Arm generator imports the records from the same pinned,
checksum-verified archive and retains their exact feature predicates.
It supports:

- FP32/FP64, widening FP16/BF16, non-widening FP16/BF16 outer products;
  signed/unsigned/mixed 8-bit-to-32-bit and 16-bit-to-64-bit products;
  SME2 signed/unsigned 16-bit-to-32-bit products, each with add/subtract forms.
- `addha/addva` for S/D tiles; partial `zero` lists of matching B/H/S/D tiles
  (including the empty list).
- Predicated horizontal/vertical `mov/mova` between Z vectors and ZA slices
  for B/H/S/D/Q, using W12–W15 selectors and element-dependent offsets.
- Predicated `ld1b/h/w/d/q` and `st1b/h/w/d/q` ZA slice transfers, with base
  or scaled X-register offsets; whole-ZA vector `ldr/str` with matching ZA
  and `mul vl` offsets; `rdsvl/addsvl` with signed 6-bit multipliers.
- SME2 unpredicated `mov/mova` between aligned groups of two/four Z registers
  and horizontal/vertical B/H/S/D slice ranges, or `.d` ZA arrays. Slice
  selectors are W12–W15; array selectors are W8–W11 with offsets 0..7.
  Optional `vgx2/vgx4` must match the register group length.
- SME2 ZA-array `add/sub` on S/D elements, using one group, two groups, or a
  group plus Z0–Z15. `fmla/fmls` on H/S/D arrays use a group plus a vector,
  indexed element, or matching group. Indexed lanes cover a 128-bit segment;
  indexed and two-group sources are aligned; scalar-vector groups can wrap.
  D arithmetic also requires the optional SME FP64 type; H requires SME FP16.
- SME2 `ldr/str zt0, [base]` and `zero {zt0}` for the 512-bit lookup table.

`with_sme2()` implies SME. The independent `with_sme_f64f64()` and
`with_sme_i16i64()` builders enable SME and their named matrix extension.
`with_sme_f16f16()` and `with_sme_b16b16()` also enable SME2. SME2 alone
permits none of these four optional types. No builder enables ordinary SVE.
For example, `assemble_with_features(source, TargetFeatures::apple_m5())`
or `keel asm --cpu apple-m5 input.s -o output.o` selects these implemented
M5 encodings. `TargetFeatures::default().with_sme2()` selects just the
SME/SME2 families without the four optional matrix types.

```sh
python3 crates/keel-asm/tools/sme_cases.py --check
python3 crates/keel-asm/tools/generate.py --archive /tmp/aarchmrs.tar.gz --check
cargo test -p keel-asm --test sme --test scalable
```

Clang comparisons use `-mcpu=apple-m5` only for the reference assembler.
Native tests check each relevant `hw.optional.arm.FEAT_*` flag before entering
streaming mode. They verify FP32, FP64, FP16, signed I8-to-I32,
I16-to-I32, and I16-to-I64 matrix results after a store/clear/reload round trip.
Separate tests verify widening/non-widening BF16 results, two/four-register
horizontal/vertical and array transfers with different register values,
FP32 multiply-accumulate/subtract and integer accumulation in ZA arrays,
and all 64 bytes of a ZT0 load/store/zero round trip.
Leaves preserve D8–D15, make no calls while streaming, enter with inactive
ZA, and exit with both state bits clear. They exercise only advertised ISA.

The practical-kernel increment adds 46 streaming-compatible forms, selected
by `STREAMING_NAMES` in the Arm generator and covered by
`tests/streaming-cases.txt` (`tools/streaming_cases.py --check`):

- `whilelt/whilele/whilelo/whilels` for P0–P15 B/H/S/D, using matching W or X
  operands (including ZR). These build predicates for partial vectors.
- Contiguous `ld1b/h/w/d`, including zero extension to larger elements;
  signed-extending `ld1sb/sh/sw`; truncating `st1b/h/w` and `st1d`.
  Each supports a single braced/unbraced Z register, P0–P7 (loads require
  `/z`), base X/SP, signed four-bit `mul vl` offsets, or X0–X30 indexes
  shifted by the memory transfer width. Extending loads and truncating
  stores retain Arm's architectural `mul vl` scaling.
- `addvl` with X/SP and signed six-bit multipliers; `dup` from W/WSP for
  B/H/S or X/SP for D (ZR is not the register-31 spelling for this form).

Every form retains its upstream `SVE || SME` requirement, including the
register-index encodings' exclusion of Rm=31. Neither default assembly nor
the M5 profile enables ordinary SVE. Native testing enters streaming mode
only after the OS advertises SME. Additional execution tests verify signed
extension, truncation, and untouched output tails for every active length
up to the host vector length.

`tests/fixtures/sme-kernels/{fp32,bf16,int8}.s` are complete correctness
fixtures for packed matrix multiplication, with loops over row tiles,
column tiles, and K blocks. They use direct Z loads, `whilelo` predicates,
outer products, and masked Z stores. Inputs are packed as
A[K-block][M][group] and B[K-block][N][group], with groups of one FP32,
two BF16, or four INT8 values (always four bytes); incomplete K groups are
zero padded. Output is row-major FP32 or INT32. All three require only SME,
not SME2 or the optional non-streaming numeric features. They are ordinary
PCS leaves with inactive ZA on entry, balanced streaming/ZA transitions,
and preserved D8–D15. Their private fixture ABI takes pointers plus M, N,
and the number of K blocks; it is not a new public library API.

The native driver compares every output element against scalar references
in 216 cases: three types, eight dimension pairs relative to the runtime
streaming vector length, and nine K sizes. Cases include zero dimensions,
zero K, odd K, partial row/column tiles, exact tiles, and multiple tiles.
Signed inputs and exactly representable quarter-valued FP/BF16 data make
expected results exact. Input panels have no row/column tail padding and
end at inaccessible guard pages; output ends at a guard page and has a
checked prefix canary. Clang byte comparisons cover all three kernels;
C reference code is compiled without optional ISA flags. These validate
end-to-end assembly and bounds, not tuning, throughput, arbitrary numerical
edge cases, or compatibility with most third-party SME2 kernels.

```sh
python3 crates/keel-asm/tools/streaming_cases.py --check
cargo test -p keel-asm --test streaming --test sme_kernels
```

Remaining scalable coverage includes predicated Z arithmetic, gather/scatter
and fault-first memory, vector shifts/conversions, SME2 grouped Z arithmetic
and loads/stores with predicate counters, integer/BF16 ZA dot products and
multiply-accumulates, widening ZA multiply-accumulates, lookup operations,
and SME2.1 (including `movaz`). These instruction forms remain unimplemented.
SME2.1 is not enabled by the current M5 profile; ordinary SVE2 remains
unmodeled. Remaining ordinary M5 coverage includes crypto, pointer
authentication, and newer scalar ISA. These are explicit coverage boundaries,
not complete A64/SVE/SME support.

`x31`/`w31` are rejected: SP and ZR must be named explicitly and are checked
against their operand roles. `fp` and `lr` name x29 and x30.

**Load/store syntax.** Single transfers include `ldr`/`str` for W/X and
scalar B/H/S/D/Q registers; `ldrb`/`strb`, `ldrh`/`strh` for W registers;
`ldrsb`/`ldrsh` for W/X destinations; and `ldrsw` for X destinations. Each
has its explicit unscaled counterpart (`ldur`, `stur`, `ldurb`, `sturb`,
`ldurh`, `sturh`, `ldursb`, `ldursh`, `ldursw`). FP/SIMD register numbers run
from 0 to 31; general register 31 still requires `sp` or `zr` explicitly.

Memory bases are X registers or SP. Bracket-aware operand splitting keeps
commas inside addresses separate from instruction operands. Supported forms:

- `[base]` or `[base, #offset]`: unsigned 12-bit offsets scaled by access
  size. Like clang, the ordinary mnemonic selects the unscaled encoding
  for small negative or unaligned offsets. Explicit unscaled mnemonics
  always use a signed 9-bit byte offset (-256..255).
- `[base, #offset]!` and `[base], #offset`: pre-index and post-index
  writeback with signed 9-bit byte offsets, without scaling. Explicit
  unscaled mnemonics do not support writeback.
- `[base, Xm]`, `[base, Xm, lsl #shift]`, `[base, Xm, sxtx {#shift}]`,
  or `[base, Wm, uxtw/sxtw {#shift}]`: register offsets. The shift is
  zero or log2(access size); an explicit byte-transfer `#0` preserves
  Arm’s distinct S-bit encoding. Register offsets do not support writeback.

`ldp`/`stp` transfer matching W, X, S, D, or Q register pairs using offset,
pre-index, or post-index addresses. Their signed 7-bit offsets are scaled by
one register’s size (-64..63 elements). Repeated load-pair destinations
and integer writeback-base overlaps are rejected because their behavior
is architecturally constrained unpredictable. FP registers and general
registers with the same number are distinct and do not overlap.

`ldar`/`stlr` support W/X registers, with `ldarb`/`stlrb` and `ldarh`/`stlrh`
for byte/halfword W transfers. These accept only `[base]` or `[base, #0]`,
without writeback. Scalar LSE atomics support LDADD/LDCLR/LDSET/LDEOR,
LDSMIN/LDSMAX/LDUMIN/LDUMAX, SWP, and CAS, each with relaxed, acquire (`a`),
release (`l`), and acquire-release (`al`) variants. B/H suffixes select byte
and halfword W transfers; unsuffixed W/X forms transfer 32/64 bits. Both data
registers must have the same width; SP is allowed only as the address base,
and zero registers are accepted as data operands. Addresses are `[Xn|SP]`
with optional #0 and no writeback/register offsets. Store-only aliases
STADD/STCLR/STSET/STEOR/STSMIN/STSMAX/STUMIN/STUMAX support relaxed/release
ordering and map the result to ZR. Acquire behavior with a ZR destination is
exactly as specified by Arm; no ordering bits are silently rewritten.

DMB and DSB accept OSH/NSH/ISH load/store/full variants and LD/ST/SY. ISB
accepts SY, its default when omitted. All three also accept a raw 4-bit
numeric option. LSE requirements are retained in generated records and
explicitly enabled for the fixed Apple Silicon target.

Baseline exclusive accesses include `ldxr/ldaxr/stxr/stlxr` for W/X, B/H
variants for W, paired `ldxp/ldaxp/stxp/stlxp` for W/X, and `clrex` (default
option 15 or an explicit 4-bit immediate). Addresses allow only a base and
optional #0. Store status is W and must not overlap a source or a non-SP
base; repeated non-ZR load-pair destinations are rejected. Differential tests
cover every width/order and native tests exercise contended exclusive loops.

Literal `ldr` supports W/X/S/D/Q and `ldrsw` supports X, with numeric byte
displacements or same-text-atom symbols plus addends. Displacements must be
4-byte aligned and in -1048576..1048572. External, cross-section, and
cross-atom references are rejected because Mach-O has no literal-load
relocation. Automatic `ldr =constant` pools are unsupported. Pair CAS,
non-temporal pairs, and unprivileged accesses remain unsupported. Symbolic
page-offset memory operands are supported below.

Named labels are case-sensitive; mnemonics and register names accept either
case. Branches resolve forward or backward within text, or accept a numeric
byte displacement from the instruction address. Signed range and four-byte
alignment are checked. Numeric local labels and general symbolic
expressions are not supported; symbol-plus-integer expressions are covered
below. Semicolons and `//` begin comments.

Directives are `.text`, `.section __TEXT,__text` (optionally with
`,regular,pure_instructions`), `.globl`/`.global`, and `.p2align N` for
0 <= N <= 16. Alignment pads text with NOPs and raises the object section’s
alignment. `.globl`/`.global` declare external symbols, including undefined
ones retained for the linker. `.extern` validates its operands without changing
binding, matching Clang. `.private_extern` declares a non-local external symbol
with Mach-O `N_PEXT`: other objects can resolve it, but the native linker omits
private definitions and private common storage from the dynamic export trie.
A public common declaration makes coalesced common storage public; an ordinary
definition takes precedence over common storage. The typed assembly transport
also exposes `Directive::PrivateExtern`; ELF rejects this Mach-O directive.

**Symbol addressing and relocations.** `adr` accepts a defined text label
with an optional integer addend, resolved as a signed 21-bit byte displacement
from the instruction. Numeric `adrp` takes a 4-KiB-aligned byte displacement,
encoded as a signed 21-bit page count. Both require X destinations (including
XZR) and check their range.

Symbolic `adrp` uses `symbol@PAGE`, `symbol@GOTPAGE`, or `symbol@TLVPPAGE`.
`add Rd, Rn, symbol@PAGEOFF` and scaled single load/store addresses
`[base, symbol@PAGEOFF]` supply the offset within that symbol’s page.
GOT/TLVP page-offset operands require `ldr Xt, [base, symbol@GOTPAGEOFF]`
or `@TLVPPAGEOFF`; they load a symbol's address from the GOT or a thread-variable
descriptor's address. Symbolic memory operands do not support writeback or unscaled
forms. The linker determines page-offset scaling from the instruction.

Expressions accept one symbol and integer addends: `symbol+8`,
`symbol@PAGE+8`, and `symbol+8@PAGE` (and their negative forms). Addends must
fit signed 24 bits. Nonzero ordinary PAGE/PAGEOFF and external branch
addends use `ARM64_RELOC_ADDEND` immediately before the matching relocation,
with the signed payload masked to 24 bits. GOT/TLVP addends are rejected.
Branch addends must be four-byte aligned.

Defined text branches resolve locally while the text section is kept intact.
Undefined non-local `b`/`bl` targets produce `ARM64_RELOC_BRANCH26` entries.
Page references produce PAGE21/PAGEOFF12, GOT_LOAD_PAGE21/PAGEOFF12, or
TLVP_LOAD_PAGE21/PAGEOFF12 entries. These always refer to symbol-table
indices; instruction immediate fields stay zero until linking. Undefined
references are retained as external symbols. Names beginning with `L` are
local by convention and require definitions unless explicitly declared
external. `adr` and conditional/test branches require definitions in this
file because Mach-O has no applicable external instruction relocation.

**Data directives and sections.** `.byte`, `.short`, `.long`, and `.quad`
emit little-endian integers, with comma-separated operands. Constants accept
decimal/hexadecimal values and addition/subtraction; values must fit the
signed-or-unsigned range of their width. `.space`/`.zero` reserve bytes with
an optional byte fill value. Raw strings can be emitted as byte lists or
with `.ascii`/`.asciz` (see debug/data support below). Floating-point data
directives remain unsupported; Keel emits their integer bit patterns.

`.data`, `.const`, `.const_data`, and `.bss` switch to `__DATA,__data`,
`__TEXT,__const`, `__DATA,__const`, and `__DATA,__bss`. `.section` also
accepts those names with their regular/zerofill types, and
`__DATA,__thread_data,thread_local_regular`,
`__DATA,__thread_bss,thread_local_zerofill`, and
`__DATA,__thread_vars,thread_local_variables` for TLS definitions.
`.zerofill __DATA,__bss,symbol,size,log2_align` (or `__DATA,__common`, where
Clang places `-fno-common` tentative definitions) and
`.tbss symbol,size,log2_align` allocate named zero-fill storage without
switching the current section. Zero-fill storage reserves virtual space
without allocating or writing bytes; nonzero initializers and relocations
in zero-fill sections are errors.

Labels record a section ID and a section-relative offset. `.p2align`
raises that section’s alignment and pads data with zeros. Text padding uses
zero bytes up to the next instruction boundary, then NOPs. Instructions
require aligned `__TEXT,__text` locations; integer data may appear there
for jump tables. `adr` and conditional branches must stay within that text
section; cross-section addresses use page relocations.

`.quad symbol [+/- integer]` emits an absolute pointer relocation.
`.long`/`.quad A-B [+/- integer]` resolve directly when both labels belong
to the same section (or the same atom with subsection splitting), including
forward references. Other differences use
`ARM64_RELOC_SUBTRACTOR` for B followed immediately by `ARM64_RELOC_UNSIGNED`
for A. Data addends are written into the 4-/8-byte word rather than into an
ADDEND relocation. Repeated identical symbols cancel even when undefined.
General algebra, symbol multiplication, and 8-/16-bit symbolic relocations
remain unsupported.

**Objects and limits.** The writer emits little-endian arm64 `MH_OBJECT`
with one unnamed segment containing text, initialized data, and zero-fill
sections. File-backed sections precede zero-fill sections in virtual layout.
The writer remaps parser section IDs to on-disk ordinals and writes symbol
values as section addresses plus offsets. Text bytes remain in
`Assembly::text`; additional sections are in `Assembly::sections`, with
separate byte and zero-fill contents. Relocations identify their section,
width, and optional subtracted symbol.

Relocations are ordered by descending offset within each section, preserving
ADDEND and SUBTRACTOR pairs. Symbol indices follow local definitions,
external definitions, then undefined symbols. TLS descriptor sections set
`MH_HAS_TLV_DESCRIPTORS`. The writer checks section/symbol IDs, sizes,
alignment, duplicate names, overlapping relocations, missing targets,
relocation widths, and addend constraints. Objects include the macOS 11.0
build-version command; Apple’s linker still produces the executable.

`.subsections_via_symbols` sets `MH_SUBSECTIONS_VIA_SYMBOLS`, allowing the
linker to reorder or discard symbol-defined atoms. Non-temporary labels and
global temporary labels start atoms; aliases at one address share one atom.
Temporary labels retain source-order ownership, so a function's end label
belongs to it even if the next function starts at the same offset. Local
references are rebased onto atom symbols with explicit addends. This avoids
introducing temporary-symbol boundaries and preserves pointers to atom ends.
Sections with no preceding atom symbol receive a synthetic local anchor.

B/BL references across atoms use branch relocations. Fixed-displacement ADR
and conditional branches require targets in their own atom; ADR across atoms
must use ADRP/ADD instead. Data differences across atoms retain SUBTRACTOR /
UNSIGNED pairs rather than freezing their original offsets. Symbolic LEB128
differences must remain within one atom because their variable-width fields
have no linker relocation. Explicit constant assignments continue to compute
same-section differences using the original layout, matching compiler use of
`.set` for DWARF compilation-unit sizes. Other instructions, sections, and
directives produce source diagnostics. Literal loads and other missing forms
still prevent assembling arbitrary Keel compiler output.

Tests compare every supported encoding form and operand boundaries against
clang’s arm64 Mach-O output, inspect our objects with an independent object
reader, check invalid operands and branch limits, and on Apple Silicon link
and execute the examples and C drivers checking memory, arithmetic,
conditions, indirect calls, and FP conversions. Logical-immediate tests cover
all 1,302 W and 5,334 X bitmasks, and FP-immediate tests cover all 256 values.
Integration tests compile small integer, FP, and popcount functions with Keel,
assemble their unmodified output, link, and run them. Instruction relocation
differential tests isolate instruction records from unwind metadata. Native execution tests have a ten-second
timeout and reap a timed-out process. Symbol tests compare instruction bytes
and normalized relocation records against clang and run cross-object calls,
page-offset integer/SIMD accesses, imported libc functions, GOT and TLVP
references, and signed addends. Negative addends are also checked with an
independent object reader and linked programs; some installed Apple clang
versions emit malformed flags for negative ADDEND records.


Data tests compare section bytes, sizes, symbols, and normalized relocations
against clang, including file-backed/zero-fill layout and TLS descriptors.
Linked programs check absolute-pointer tables and cross-section differences.
The existing Keel globals, TLS (including multiple threads), switch/jump-table,
and local-linkage programs run through the assembler with all directives
retained, including CFI, source debug metadata, and subsection splitting.
Native subsection tests reorder functions/data with an order file, enable
`-dead_strip`, verify unused code is removed, and check pointers/differences
after layout changes. C++ catches and cleanup also execute after reordering
and dead stripping.


**Unwind and debug metadata.** `.cfi_startproc`/`.cfi_endproc` collect
address-indexed CFA and saved-register rules. Supported rules include
`.cfi_def_cfa`, `.cfi_def_cfa_register`, `.cfi_def_cfa_offset`, `.cfi_offset`,
`.cfi_restore`, `.cfi_same_value`, `.cfi_undefined`, and balanced
`.cfi_remember_state`/`.cfi_restore_state`. AArch64 X registers have DWARF
numbers 0–30, SP is 31, and D/V registers are 64–95. The assembler emits
DWARF CIE/FDE records in `__TEXT,__eh_frame`, using PC-relative pointers and
Mach-O symbol-difference relocations. Apple’s linker and libunwind consume
these records. Each function has a `__LD,__compact_unwind` entry. For
representable layouts it contains an ARM64 frame or frameless encoding,
with absolute personality and LSDA pointer relocations when present. Other
layouts use DWARF mode so the linker includes their FDEs in the runtime unwind
index. Functions with compact encodings omit their FDEs; when all functions
qualify, the assembler does not create an `__eh_frame` section.

Compact frame mode requires CFA = X29 + 16, X29 saved at CFA - 16, and X30
at CFA - 8. Optional X19/X20 through X27/X28 and D8/D9 through D14/D15 pairs
must be packed immediately below FP/LR in increasing register-pair order,
with all integer pairs before FP pairs. Within a pair, the lower-numbered
register must have the higher address (X19 at CFA - 24, X20 at CFA - 32).
Absent pairs consume no space. Selection uses register/offset semantics rather
than CFI directive order. Frameless mode supports functions with no saved
registers and one `.cfi_def_cfa_offset` declaration: 0–65520 bytes, aligned
to 16 bytes. A procedure with no CFI rules has zero stack adjustment.

Single-register saves, reversed pairs, gaps, nonstandard CFA rules, duplicate
rules, and state changes retain DWARF. Keel's current code generator places
its paired registers in the opposite order, so those functions retain DWARF;
its simple FP-only functions can use compact frame mode. Compact personality
support is limited to the system C++ and Objective-C routines
(`___gxx_personality_v0` and `___objc_personality_v0`), preserving the limited
Darwin personality-index slots. Custom personalities always retain a CIE/FDE.
Tests compare all 512 canonical pair subsets, leaf stack-size boundaries,
and canonical personality records with Clang. Mixed compact/DWARF objects
are checked independently. A native exception test with no assembly FDEs
verifies restoration of integer and FP callee-saved registers through compact
frames, including after function reordering and dead stripping.

For DWARF fallback, `.cfi_personality 155, symbol` attaches an indirect,
PC-relative signed-32-bit personality reference through the GOT;
`.cfi_lsda 16, symbol` attaches a PC-relative native-width LSDA pointer.
Unsupported encodings and duplicate personality/LSDA directives are errors.
CIEs are shared only between procedures with the same personality and LSDA
encoding. LSDA data lives in `__TEXT,__gcc_except_tab`. Its type-table entries
use `.long typeinfo@GOT-label`, with the subtraction label at that word's
address and no addend; the writer emits `ARM64_RELOC_POINTER_TO_GOT`.

Darwin temporary `L…` labels stay in the in-memory assembly model. The object
writer omits unreferenced temporary labels from nlist: treating interior
labels as linker entries creates spurious ranges without unwind information.
Absolute pointers to temporary labels use local section relocations, keeping
them out of nlist too. Other required temporary text symbols are marked
`N_ALT_ENTRY`. Native tests cover C++ typed catches, catch-all handlers,
cleanup/rethrow, and propagation across a plain assembled frame containing
an address-taken interior label. LSDA bytes and GOT relocation records are
compared with Clang; unresolved GOT placeholder bytes are normalized.

`.file number "path"`, `.file number "directory" "name"`, and
`.loc file [line [column]] [basic_block] [prologue_end] [epilogue_begin]
[is_stmt 0|1] [isa N] [discriminator N]` generate a DWARF 4 line-number
program in `__DWARF,__debug_line`. As in LLVM, `is_stmt` persists across
`.loc` directives while the other options apply to one row. A directory is
kept verbatim as an include directory and its name unsplit, matching Clang's
DWARF 4 tables; a one-string path keeps directory index 0. As in LLVM,
`.file 0 "directory" "name"` selects a DWARF 5 table instead, with optional
`md5` and `source` operands on every `.file` (see [dwarf5.md](dwarf5.md));
without it the DWARF 4 output is unchanged, and `md5`/`source` are errors.
Unnumbered `.file` is an error. File numbers are contiguous from 1. The line
program covers the text section and terminates at its end. Explicit debug
sections support `__debug_info`, `__debug_abbrev`, `__debug_line`,
`__debug_ranges`, `__debug_str`, `__debug_loc`, Clang's DWARF 4 accelerator
tables (`__apple_names`, `__apple_objc`, `__apple_namespac`,
`__apple_types`), and the DWARF 5 sections `__debug_line_str`,
`__debug_str_offs`, `__debug_addr`, `__debug_rnglists`, `__debug_loclists`
and `__debug_names`, with
`regular,debug` attributes. Their Mach-O `S_ATTR_DEBUG` flag excludes them
from the loaded image. Local debug address relocations use section ordinals
and preserve original addresses in the data, allowing tools to read unlinked
objects. `.ascii`/`.asciz` preserve quoted punctuation and support standard
newline/tab/carriage-return, quote/backslash, and octal escapes.
`.uleb128`/`.sleb128` encode integer constants and same-section symbol
differences, including forward labels. Symbolic LEB128 lengths affect label
positions, so assembly repeats layout passes until positions stabilize, with
a 32-pass limit that diagnoses cyclic layouts. Constant-only input takes one
pass. Undefined/cross-section LEB128 expressions, other `.loc` options
(such as `view`), and arbitrary string concatenation remain errors.
Constant assignments (`name = expression` or `.set name, expression`) accept
constants and same-section differences, including forward labels; they can
be used in fixed-width numeric directives.

The compiler remains responsible for source-level types, variable locations,
and lexical scopes. Its existing DWARF data directives now assemble without
stripping metadata. The assembler supplies the address-dependent line and
unwind tables. On macOS, the linker records an object-file debug map and
`dsymutil` combines the referenced DWARF into a dSYM bundle. Tests compare
line rows and data encodings against Clang, verify Keel’s types/scopes/variables
with `dwarfdump`, verify a linked dSYM, and execute a backtrace crossing an
assembled frame. Cross-atom conditional branches and ADR remain errors;
unrepresentable layouts retain their DWARF unwind records.


**Compiler-output coverage.** The assembler suite checks every backend
fixture with normal/reduced register pools and after optimization, and every
C fixture with/without optimization and debug information. These checks use
Keel's assembler for the complete generated source, retaining all directives;
Clang remains the reference assembler in the separate compiler suites. Native
assembler tests run the atomic driver (including multithreaded increments and
CAS locks), 128-bit arithmetic, and inline assembly containing SVC and MOVI.
Differential tests cover all scalar LSE ordering/width forms, store aliases,
barrier options, and all modified-immediate MOVI values for supported lane
arrangements. The initial end-to-end coverage audit assembled, linked, and ran 170
configurations, with DWARF verification for the 22 C debug configurations;
the automated assembly checks discover newly added fixtures as the suite grows. Fixture coverage
does not imply complete A64 ISA or GNU assembler syntax coverage.


**M5 compatibility additions (October 2026 audit).** The nine confirmed probes
now assemble, with useful families around each: the classic PAC/AUT/XPAC
register, SP, zero-modifier, branch and return forms; BTI targets; AES rounds,
SHA-1/SHA-256, CRC32/CRC32C, 64-bit PMULL; CASP in both widths and all four
orderings; scaled, unscaled, register-offset and local-literal prefetch; selected
MRS/MSR registers and DC/IC operations, plus numeric SYS syntax. Compiler output
also required CSSC integer min/max, count/abs/ctz, RCpc acquire loads, LDPSW,
bitfield insert aliases, scalar FP rounding conversions, conditional FP compare
and fixed-point conversions. Optional features have separate library builders;
`apple_m5()` selects them. Default FP/NEON/LSE behavior stays gated as before.
New instruction fields use stack arrays and stack-formatted encoding names.

Compiler-output compatibility also includes all 13 scalar LRCPC2 unscaled
acquire-load/release-store encodings (`ldapur`, `ldapurb/h`, `ldapursb/sh/sw`,
`stlur`, `stlurb/h`). `with_lrcpc2()` enables them and implies `with_lrcpc()`;
LRCPC alone does not enable LRCPC2. `apple_m5()` includes both. The default
FP/NEON/LSE profile is unchanged. Their offsets are signed byte offsets in
`-256..255`, with no scaling, register offset or writeback. Loads preserve the
specified destination width and sign extension; stores preserve access width
and release semantics. They are never lowered to ordinary loads/stores or to a
different acquire ordering. LRCPC3 FP/SIMD transfers remain unsupported.

Apple SIMD destination suffixes also normalize the widening multiply,
accumulate and subtract families (including saturating doubled variants and
upper halves), with indexed multipliers using the narrow source element.
FP comparisons preserve register versus positive-zero-immediate forms and
existing FP16 feature gates. Normalization uses the ordinary operand validators
and encoders. Differential tests cover every signed LRCPC2 offset; native tests
check access width/sign extension, SIMD signed/unsigned arithmetic, upper halves,
lane selection, saturation, NaNs, infinities and signed zero against C oracles.

The instruction layouts and feature predicates are imported through the same
checksum-verified Arm archive. `isa/m5-compat.txt` inventories selected records.
`isa/system.json` retains selected Registers.json names, architecture conditions
and accessor encodings/directions. The generator validates their AArch64
conditions and accessor predicates; it does not model runtime access/trap rules.
System/cache instructions may require privilege or OS permission: assembler
acceptance is permission to encode, never a promise of EL0 execution. The Arm
PMULL record has a baseline AdvSIMD predicate; the 64-bit polynomial form adds
an explicit FEAT_PMULL gate in operand handling, preserving baseline byte PMULL.

The capability audit compared installed Apple Clang 21's
`--print-enabled-extensions -mcpu=apple-m5` against LLVM's
[Apple M5 inventory](https://github.com/llvm/llvm-project/blob/main/clang/test/Driver/print-enabled-extensions/aarch64-apple-m5.c)
and Apple's [pointer-authentication documentation](https://developer.apple.com/documentation/xcode/enabling-enhanced-security-for-your-app).
LLVM explicitly identifies MPAM and SEL2 as over-broad architecture defaults in
its list. Neither that list nor Clang accepting a spelling establishes executable
hardware parity. No ordinary SVE was enabled, nor were all listed extensions.
SHA-512/SHA-3, MTE, most system-register names, generic encoded system-register
syntax and newer PAC extensions remain outside this implementation. There is
no full M5 ISA or Clang syntax parity claim.

Clang source support preserves `.build_version` deployment/SDK in
LC_BUILD_VERSION, literal/C-string section types, `.comm` as Mach-O common
symbols (including alignment), and all eight `.loh` kinds in
LC_LINKER_OPTIMIZATION_HINT. LOH labels must resolve to aligned text within one
linker atom. Optional `.p2align` fill/maximum operands, CFI W and FP register
aliases, C string backspace/form-feed escapes, local shifted symbol differences
and the audited Apple SIMD suffix/lane spellings are supported. The suffix
names the destination arrangement; listed mnemonics derive each source from
LLVM's Apple asm variants (same-width, long, wide, narrow, dot product,
`dup` from a GPR or `vN[i]`, structure lists, and 16B tables), then use the
ordinary operand and feature checks. Other suffixed mnemonics are errors. Shifted
expressions follow LLVM's logical 64-bit right-shift semantics; unresolved
shifted relocations remain errors. `.cfi_negate_ra_state` emits the AArch64
DWARF opcode and forces DWARF unwind retention so signed-RA state is preserved.

`suite/audit_clang_assembler.py` compiles sources unchanged with Clang -S -O2,
assembles with both tools, and compares code/data bytes, normalized relocations,
section type/size/alignment, build version and linker hints. The checked-in
October 15 source audit covers all 18 repository C fixtures and 33 Lua translation
units, with input/assembly/binary hashes. DWARF and compact-unwind bytes are
excluded from this raw-byte audit because equivalent representations can differ;
focused Rust tests exercise the object metadata and call-frame paths. This is
an inventory of those exact sources/options, not every compiler mode or syntax.


The final compatibility evidence is indexed in
`suite/reports/assembler-m5-compat-summary-20261015.json`. All nine original
probes match Clang; all 51 unchanged sources pass the byte/relocation/metadata
audit, and the 33 linked Lua objects pass guarded sorting, arithmetic, string,
error-handling and coroutine checks on M5 Max. Validation passed 158 assembler
and 11 CLI tests, one doctest, generator `--check` and `cargo fmt --check`.

Performance remains a tradeoff: nine paired final-build Lua runs observed
+0.6% serial CLI time and +6.0% aggregate in-process parsing versus 84b5630.
The final nine-pair comparisons observed +3.6% CLI / +4.8% parsing for 64 MiB
functions, and +4.7% CLI / +6.2% parsing for dense instructions. The 1 MiB old
feature workloads observed CLI changes of +4.6% NEON, +4.3% numeric, -1.1% SME
and -1.0% streaming; corresponding parse changes were -0.2%, -3.5%, -0.7% and
+3.2%. Small changes, especially CLI startup costs, are sensitive to run noise;
raw samples are retained, and these are observed medians rather than a claim of
isolated causality. Coverage was not removed to improve these numbers.

Final command timings were about 10.5x faster than direct Clang -cc1as on old
Keel-generated Lua input, 5.0x on the new PAC/crypto/system encoding workload,
and 5.6x on the 51 unchanged Clang-generated C/Lua sources. These are warm-cache
complete command comparisons, not pure parser comparisons or equivalent-facility
claims. The seven-pair Clang scale reference report predates only the final
zero-alignment diagnostic fix; the final-build old/new comparison records exact
final binary hashes and verifies unchanged text. `generate_m5_compat_inputs.py`
reconstructs feature inputs from checked-in correctness seeds;
`compare_assembler_builds.py` provides paired old/new measurements when a Clang
reference run has already established correctness.
