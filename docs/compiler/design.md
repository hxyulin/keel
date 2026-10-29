# Keel design decisions

Keel is an educational compiler: a low-level SSA IR, optimizer, code
generators for AArch64 and x86-64, assemblers, linkers and a C frontend. Clang
serves as a test oracle. This page records the IR, ABI, register allocation and
SSA builder design and their limits.

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
  `ptr sret {..}`; see ABI).

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
everything passes need (signature, purity, memory effects) comes from the
registry in `intrinsic.rs`, generated from `tools/x86-isa/intrinsic-registry.json`.

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
  - The default CPU level is v1; `--cpu-level` and `--target-features`
    select more. Traps are `ud2`.
  - Not supported on x86-64: exceptions, i128↔float conversions, i128
    overflow multiply and `mulhi`, and `switch` on i128.
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
graph colouring would give.

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

   Holes matter in big `switch`-based functions: without them, a value
   defined in one case and used in a tail block far below would block a
   register through every case in between.

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

Target selection prepares instruction-kind and operand-use facts once per
function.

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
