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

`keel-opt` runs function passes (`fn(&mut Function) -> bool`) and module
passes through the `Pass::run_module` hook. The manager runs each pass over
the module before moving to the next and **verifies after every pass** each function the pass
reports changed: the IR is valid between passes,
always, so a broken pass is caught where it broke things. Passes compute the
analyses they need; there is no analysis cache yet.

| Pass | What it does |
|---|---|
| `inline` | module-level bounded private direct-call expansion, including fresh by-value snapshots and remapped CFG/SSA/stack slots |
| `small-memory` | expand ordinary constant memory intrinsics of 0–32 bytes with exact accesses; preload memmove sources before stores; see [bounded memory expansion](optimizer.md) |
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
| `tail-calls` | replace adjacent scalar direct call/return pairs when exact signatures fit native argument registers and a local call-graph proof excludes frame observers, pointers, unknown calls and implicit helpers |

Balanced, Size and Full run `tail-calls` after final private calling-convention
selection. Recognition requires matching nonvariadic signatures with at most six
single-register scalar arguments and one result. The entire direct-call closure
must contain only scalar arithmetic, control flow and calls to strong local
bodies with the same frame-independent property. Unknown callees, frame and stack
intrinsics, pointer values, memory accesses, assembly and implicit FMA helpers
block recognition. This preserves ordinary call semantics when a callee can
inspect its caller's frame. Explicit IR `return_call` remains available for
producer-proven cases outside this recognition boundary. The separate wasm32
basic pipeline does not run this native pass.

See the [simplification audit](simplification.md) for missing analysis/rewrite
families, concrete phase-order examples, and early/late/repeated cleanup sweeps.

The default pipeline (`-O`) runs `private-aggregate-cleanup`, then three bounded
rounds of `inline-cheap-reserve, simplify-cfg, canonicalize, instcombine,
call-contracts, simplify-cfg, gvn, small-memory, stack-forward, canonicalize, instcombine, dce, gcm,
adjacent-abs, loop-vectorize, loop-reduce`. Cleanup between inliner invocations exposes smaller
helpers for the next round. `inline-cheap-reserve` keeps the ordinary caller
and legality limits with a larger module growth budget, then retries cheap
helper sites denied only by that budget.
Public definitions require a `noninterposable` or `no_semantic_interposition`
producer promise. The C frontend derives the latter for strong hidden ELF
definitions or the explicit `-fno-semantic-interposition` mode.
The 48 round entries retain code motion and loop optimization between rounds,
followed by this 26-entry epilogue:
`load-reuse, stack-promote, phi-simplify, dce, simplify-cfg,
loop-strength-reduce, canonicalize, instcombine, dce, loop-reduce-multi,
loop-unroll, loop-rotate, phi-simplify, dce, constant-args, simplify-cfg,
known-bits, simplify-cfg, dce, empty-calls, dce, unused-functions, dce,
private-aggregate-cleanup, internal-fast-calls, tail-calls`.
Memory cleanup precedes wrapping recurrences. Extended sum reductions precede
unrolling and rotation: unrolling first leaves only a remainder loop too short
to vectorize. `loop-reduce-multi` includes the affine-offset and
same-lane cast support, so those passes are not scheduled separately.
If-conversion and an additional final GCM remain opt-in after Lua screening
found a large fib regression in broader candidates. The selected schedule
passed the kernel and Lua correctness checks; noisy identical-binary timing
controls prevent a placement-independent speedup claim. This fixed 75-entry
schedule is not an unbounded fixed-point loop.
An earlier 59-entry schedule's paired suite comparison on ARM and two x86 hosts measured 8.5–15.6% less
runtime than one round at 50–53% greater preprocessed-C-to-object compilation
time, with increased code size and uneven workload benefits. The
[suite comparison protocol](../../suite/README.md#native-platform-comparisons)
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
at most 128 sites and 2,048 added IR instructions. The module limit is 16,384
added instructions, or 65,536 in Full's `inline-cheap-reserve` policy, with hard
stack budgets (including by-value snapshots). These admission budgets reset on
each invocation; Full uses three invocations.
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
64-subset pass matrix and native spill/selector checks. On the Lua, QuickJS
and SQLite application corpus (Apple M5 Max) it gave about 2.7% lower runtime
at about 1% extra compile time versus the earlier Full pipeline.

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
assembly was identical in all 44 files and compile timing was neutral.
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
