# x86-64 C inline assembly

Keel supports scalar GNU C inline assembly on `x86_64-unknown-linux-gnu`.
This implements the first milestone in the [design](x86-c-inline-asm-design.md).
Memory constraints, asm goto and file-scope asm remain unsupported.

```c
unsigned long add(unsigned long a, unsigned long b) {
    __asm__ volatile("addq %[rhs],%[result]"
        : [result] "+r"(a)
        : [rhs] "r"(b)
        : "cc");
    return a;
}
```

## Accepted contracts

Operands accept integers through 64 bits and pointers. Register constraints are
`r`, and fixed `a`, `b`, `c`, `d`, `S`, `D` for RAX, RBX, RCX, RDX, RSI, RDI.
Outputs use `=` or `+`, optionally followed by early-clobber `&`. Matching input
constraints use an output number or `[name]` and require the same IR type.
Output addresses and input expressions are evaluated once. A read/write output
captures its initial value and writes its final value to that captured address.
One input may initialize each tied output. Untied operands receive distinct
registers, even when GNU could reuse them. Fixed-register overlaps require a tie.
Register exhaustion produces a diagnostic rather than relaxing a constraint.

`i` and `n` inputs require integer constant expressions through 64 bits.
Templates use AT&T syntax, numeric/named operands, `%%`, unique `%=` labels,
and `%b`, `%w`, `%k`, `%q` register views. Default register width follows the
operand type; pointers use 64 bits. Immediate substitutions include `$` unless
`%c` requests the bare constant. High-byte `%h`, dialect alternatives and other
modifiers are not supported. Numeric local labels and GNU macros use the shared
assembler, including definitions used by a later asm statement.

Clobbers accept `cc`, `memory` and generated GPR names/aliases. High-byte
names AH/CH/DH/BH clobber their parent RAX/RCX/RDX/RBX registers. RSP and RBP
cannot be clobbered. R15 is also reserved in functions whose aligned frame needs
it as a base pointer. FP/SIMD, control/debug and other register classes are not
asm operands or clobbers in this milestone. The IR retains target-tagged physical
registers and early-clobber markers, including in textual round trips.

All asm is pinned and conservatively invalidates memory knowledge, even without
`volatile` or a `memory` clobber. This retains more code than GCC; it does not
emit a hardware memory fence. Basic asm currently accepts only empty text or
`nop`; use extended asm for effects. `inline` has no size-hint effect.

## Lowering and boundaries

The backend snapshots GPR homes and stages every register input in frame slots
before filling operand registers. It captures every output before restoring
old homes, then installs SSA results. This handles fixed-register cycles,
rematerialized inputs and values live across arbitrary declared GPR clobbers
without changing the general allocator. Functions with asm also save the
callee-saved GPRs in their normal prologue/CFI. Frame slots and stable frame-base
registers remain outside operand allocation.

This conservative implementation emits many saves/loads and grows stack frames.
It establishes correctness before reducing saves using liveness and precise
instruction-boundary constraints. It does not use SIMD scratch registers for
asm staging. This alone does not establish a kernel code-generation mode.

Substituted text remains an explicit fragment in the enclosing object program.
The shared assembler processes directives, macro state, labels, relocations and
CPU-feature legality when it consumes that program. Emitting assembly text alone
does not validate the template's instruction syntax or encoding. The compiler
restores Intel syntax for its own instructions after each AT&T fragment.

The programmer must declare every effect, define each output, preserve the
stack/frame bases and the direction flag required by the ABI, and return through
normal fallthrough. Local branches may stay within the template. Calls, terminal
transfers and branches to C labels need contracts this milestone does not model;
the compiler does not infer these effects by inspecting arbitrary instructions
or directive bytes. Section switches must return to the function's section.

Still diagnosed: memory operands/outputs, unions/alternatives, symbolic
immediates, flag outputs, local register variables, FP/SIMD operands, asm goto
and file-scope asm. Supporting their instruction bytes in the assembler does
not provide the missing C operand or control-flow contract.

## Validation

`cargo test -p keel-c --test x86_inline_asm` verifies contracts and diagnostics,
pinned behavior, IR round trips, and byte-for-byte text/typed/chunked object
parity across 144 configurations: v1/v2/v3, three selectors, two allocators,
normal/empty pools, optimization on/off and debug info on/off. A custom allocator
adds a forced constant-rematerialization configuration.

The 22-function fixture covers ties, fixed-register shuffles, early clobbers,
multiple outputs, narrow register views, constants, pointers, output-address
side effects, flags, memory barriers, local labels, shared macros, live values,
clobbers and aligned frames. The separate
[native runner](../suite/x86-inline-asm/README.md) checks the actual exported
objects against independently compiled GCC and Clang references, with broken
controls. AArch64's existing inline-asm suite remains a separate regression.

The full workspace run completed with 2,754 passed, three failed and 13 ignored
across 506 groups. The failures were the retired x86-asm rejection assertion,
the newly added high-byte-parent alias test, and the IR golden output that now
retains `&`. The alias mapping and both expected results were corrected. All
150 IR package tests and 20 affected C/branch tests then passed, and workspace
check passed. The full workspace was not repeated after these fixes. Both
generator freshness checks, all 98 generator tests and three audit tests passed.
The final 145 exported objects match the native-qualified bytes exactly; the
updated fixture also passed all 290 native executions and four broken controls.
