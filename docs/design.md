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
- `ptr` is opaque. Memory is untyped bytes.
- **Layouts** are named declarations (`layout %Point = c { x: i32, y: i32 }`).
  `field.addr` and `elem.addr` are checked by the verifier and lowered to
  `ptradd` with a constant offset, so optimizations only see `ptradd`.
  Bitfields belong to layouts, not to the instruction set.

## Undefined behaviour

- Integer arithmetic wraps by default.
- `.trap` variants (`iadd.trap`) are defined: they trap on overflow, and
  later code may rely on the overflow not having happened.
- **The only source of UB is `assume`** (`assume.range`, `assume.nonnull`…).
  Debug builds turn assumptions into trapping checks; release builds trust
  them. UB is therefore opt-in, local and greppable.
- Uninitialized memory reads give an arbitrary but stable value.
- Out-of-bounds memory access cannot be defined at this level; memory safety
  is the job of a higher-level IR.

## Memory

- Memory operations are ordered by CFG order.
- Optional alias-class tags on loads and stores (`[stack]`, `[heap]`…)
  promise disjointness between classes.
- Atomics and fences follow the C++11 memory model.

## ABI

- Functions carry a calling-convention descriptor (`cc(c)`), resolved against
  the target. A single `abi-lower` pass turns calls into explicit register
  and stack moves, using layouts to classify by-value structs.

## Target

- First and only target until everything works end to end: **aarch64-apple**
  (Mach-O). Executables must carry an ad-hoc code signature
  (`LC_CODE_SIGNATURE`) or the kernel refuses to run them.
