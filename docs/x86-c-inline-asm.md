# x86-64 C inline assembly

Keel supports scalar GNU C inline assembly on `x86_64-unknown-linux-gnu`.
This implements scalar fallthrough asm and deferred immediate validation from
the [design](x86-c-inline-asm-design.md).
Explicit memory operands, bounded constraint choices and integer flag outputs
are supported. Comma-separated alternatives and asm goto remain unsupported.
Basic asm is supported at function and file scope on x86-64 Linux.

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

Register operands accept integers through 64 bits and pointers. Register constraints are
`r` and x86-64 `q`, and fixed `a`, `b`, `c`, `d`, `S`, `D` for RAX, RBX, RCX, RDX, RSI, RDI.
Outputs use `=` or `+`, optionally followed by early-clobber `&`. Matching input
constraints use an output number or `[name]` and require the same IR type.
Output addresses and input expressions are evaluated once. A read/write register output
captures its initial value and writes its final value to that captured address.
One input may initialize each tied output. Untied operands receive distinct
registers, even when GNU could reuse them. Fixed-register overlaps require a tie.
Register exhaustion produces a diagnostic rather than relaxing a constraint.

Scalar register outputs also accept `rm`, `mr`, `qm`, `mq` and `g` with `=` or
`+`, optionally with `&`. They select the existing GPR path, including captured
output locations and read/write ties. Use explicit `m` for non-GPR outputs.
Inputs accept nonduplicated combinations of `r`/`q`, `i` and `m` that include
`r` or `q`, such as `iq`, `ri`, `rm` and `irm`; `g` permits all three locations.
Scalar integers and pointers prefer numeric or supported non-TLS symbolic
immediates when `i` is allowed, then registers. This choice happens after
required inlining and constant cleanup, even without `-O`. Addressable non-GPR
objects use the existing memory path when `m` is allowed. These are unions
within one constraint column. Inputs also accept `er`/`re` and `Ir`/`rI`.
The `e` option accepts numeric constants in the signed 32-bit range; `I` accepts
0 through 31. Other values use a register. These predicates survive required
inlining in IR as `re` and `rI`. Symbols use a register for `er`, because Keel
does not establish a signed-32-bit symbol range through a code-model contract.
The [GCC x86 constraint definitions](https://gcc.gnu.org/onlinedocs/gcc/Machine-Constraints.html)
define these numeric predicates. Standalone `e`/`I`, other machine immediate classes, comma-separated alternatives
and memory retries after scalar register exhaustion remain unsupported. Keel does not infer instruction-specific immediate ranges from
arbitrary templates.

Flag outputs use `=@cc<condition>` and integer lvalues through 64 bits. Generated
x86 condition mappings accept the normal `SETcc` suffixes and their aliases,
including `z`/`e` and `c`/`b`. Results are zero or one in the complete integer
width. Flag outputs count in source operand numbering but cannot appear in
template substitutions or matching constraints. They reject `+`, `&`, and
alternatives. Multiple flag outputs observe the same final template flags.
`__GCC_ASM_FLAG_OUTPUTS__` is predefined as `1` only on x86-64 Linux. The
[GNU constraint rules](https://gcc.gnu.org/onlinedocs/gcc/Simple-Constraints.html)
and [flag output contract](https://gcc.gnu.org/onlinedocs/gcc/Extended-Asm.html#Flag-Output-Operands)
describe the source semantics; the selection limits above define Keel's subset.

`n` inputs require numeric constants through 64 bits. `i` also accepts one
symbol address plus a checked signed byte addend. Symbolic inputs use relocations;
TLS symbols require a separate relocation contract and are rejected by codegen.
Immediate legality is checked after required inlining and constant cleanup,
including with optional optimization disabled.
Templates use AT&T syntax, numeric/named operands, `%%`, unique `%=` labels,
and `%b`, `%w`, `%k`, `%q` register views. Default register width follows the
operand type; pointers use 64 bits. Immediate substitutions include `$` unless
`%c` requests the bare constant. `%n` prints a negated numeric constant without
`$`, using 64-bit wrapping negation; symbolic `%n` is rejected.
`%a` treats an operand as an address: a 32- or 64-bit register becomes
`(%reg)`, a numeric immediate becomes a bare absolute address, and a symbol
plus addend becomes `symbol+addend(%rip)`. It does not add a memory effect;
templates that dereference register/immediate addresses must declare that effect
through memory operands or the `memory` clobber. Memory operands and narrow
registers reject `%a`. These follow the
[GNU operand modifier rules](https://gcc.gnu.org/onlinedocs/gcc/Extended-Asm.html#Generic-Operand-Modifiers)
within the supported operand subset.
High-byte `%h`, dialect alternatives and other
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
before filling operand registers. Flag result slots are zeroed before the
template, then `SETcc` writes their low bytes immediately after it. These
captures preserve flags and require no GPR operands. It captures every output before restoring
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

Still diagnosed: comma-separated alternatives, unsupported constraint unions,
TLS-only immediates, local register variables, FP/SIMD register operands, asm goto
and file-scope asm. Supporting their instruction bytes in the assembler does
not provide the missing C operand or control-flow contract.

## Validation

`cargo test -p keel-c --test x86_inline_asm` verifies contracts and diagnostics,
pinned behavior, IR round trips, and byte-for-byte text/typed/chunked object
parity across 144 configurations: v1/v2/v3, three selectors, two allocators,
normal/empty pools, optimization on/off and debug info on/off. A custom allocator
adds a forced constant-rematerialization configuration.

The original 22-function fixture covers ties, fixed-register shuffles, early clobbers,
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

## Required inlining and deferred immediates

For x86 C translation units containing asm, C lowering keeps immediate inputs as
typed SSA values. It then expands direct `always_inline` calls before returning
public IR. This phase runs in both ordinary and profiling compilation, without
requiring an optimizer tier. It reuses the inliner's operand, symbol, stack-slot
and debug-scope remapping. Asm templates and early-clobber/tie contracts are
cloned; each emitted instruction receives its own `%=` identity.

Local unused `always_inline` bodies are omitted unless retained, address-taken,
or referenced by live code/data. Their callee declarations remain available to
IR metadata. Retained standalone bodies must satisfy their own immediate
constraints. A runtime argument does not become a constant because its helper
was marked `always_inline`.

Mandatory expansion retains existing binding, recursion, ABI and structural
legality checks. Missing definitions, noinline conflicts and unsupported calls
produce errors rather than profitability skips. Expansion is limited to one
million cloned instructions. Eight rounds of local constant cleanup are allowed
for pending immediates. Debug parameter homes can forward known stores into
loads; memory-writing barriers still invalidate those facts. Ordinary optional
inlining remains an optimizer decision.

For example, the kernel-style `rip_rel_ptr(&global)` helper can pass its pointer
parameter to an `i` operand. After required inlining, `%c1(%%rip)` becomes
`global(%rip)`. `n` rejects the same symbol. Unresolved runtime inputs fail final
IR verification before emission. There is no relaxed public verifier mode.

The fixture now adds two exported functions covering nested helpers, repeated
calls, numeric arguments, symbolic offsets and unique labels. The historical
290 native executions above qualify the earlier fixture. This increment passed
145 fresh native executions against GCC 13.3.0 on Linux x86-64, with five broken
controls rejected normally. The harness includes both new functions in all
512-input cases. Clang/GNU byte-and-relocation comparisons also passed.
A subsequent [unchanged-input audit](../suite/linux-kernel/audit-deferred-immediates.json)
on a reproduced corpus removes all 450 earlier immediate first errors. Accepted
C inputs remain four of 484 because unsupported constraints and section
placement become the next errors. Assembly stays at 326 of 506. No crashes or
timeouts occur, and no Keel object is linked into or run as the kernel.

Production changes passed `cargo test --workspace --no-fail-fast`: 2,762 passed,
zero failed and 13 ignored across 506 reported groups. During that run the
address witness was strengthened so the symbol base cannot cancel between two
XOR operands. Production code did not change. All seven tests in the affected
C target passed again with fresh exports from the final fixture; those 145
objects passed the native GCC run and all five broken controls. The full suite
was not repeated for that test-only adjustment. Workspace check, the profiling
feature build, both generator freshness checks, all 98 generator tests, three
audit harness tests, scoped formatting and diff checks also passed.

## Explicit memory operands

X86 C supports `m` inputs, `=m` write-only outputs and `+m` read/write outputs.
Each operand refers to an addressable object, including an array object. The
frontend captures its address once before evaluating explicit inputs. Memory
outputs require writable objects; directly atomic, const-qualified, bit-field
and vector-element outputs remain diagnosed. Memory operands do not accept
register-view modifiers, matching-register ties or early-clobber `&`.

```c
void add(unsigned long *p, unsigned long n) {
    asm("addq %1,%0" : "+m"(*p) : "r"(n) : "cc");
}
```

The backend emits the original object's memory reference. It does not load a
scalar copy and store it back after asm. An address gets its own GPR, distinct
from register outputs, other untied operands and clobbers. Input staging protects
it from scratch-register use and operand shuffles. Operand size follows the asm
instruction and its suffix, as in GNU AT&T syntax.

IR uses pointer arguments with `m`, `mwrite` and `mreadwrite` constraints.
Memory outputs have no SSA result. Register results precede these address
arguments internally; C template references and ties are remapped while GNU
source numbering and named operands keep their meaning. Required inlining
clones these pointer operands normally.

All asm remains pinned with unknown read/write memory effects. Optimizers may
simplify address computations and surrounding code, but must preserve memory
observations across the asm. No new optimizer algorithm or precise object-extent
alias model is introduced. This is deliberately conservative, including for a
read-only `m` operand. The constraint choices described above use these same
pinned effects.

The execution fixture adds memory reads/writes, mixed memory/register outputs,
read/write ties, single address evaluation, aliasing input/output objects,
byte/word/dword widths, array objects, unused outputs and aligned stack objects.
Its native controls include omitted writes, wrong addresses and wrong reads.
Earlier native and kernel audit counts above refer to their named checkpoints.

This increment passed all 2,766 workspace tests, zero failures and 13 ignored
across 507 reported groups at checkpoint `7572b6ef`. Workspace and profiling
feature checks, 98 generator tests, both freshness checks and scoped formatting
also passed. All 145 fresh native objects passed against GCC 13.3.0 with eight
normally rejected controls. The
[unchanged kernel-corpus audit](../suite/linux-kernel/audit-memory-operands.json)
accepts 402 assembly inputs and four C inputs. All 330 previously accepted
objects remain byte-for-byte identical. Later constraint failures still prevent
further C acceptance; these tests do not qualify kernel code generation.

## The x86-64 q register class

C inputs and outputs accept `q`, including `=q`, `+q` and the early-clobber
forms `=&q` and `+&q`. GCC's
[x86 constraint definition](https://gcc.gnu.org/onlinedocs/gcc/Machine-Constraints.html)
allows every integer register in 64-bit mode. The C frontend therefore
normalizes `q` to the existing IR `r` class. Backend allocation, reserved
register rules, ties and input staging are unchanged.

Operand type still selects byte, word, dword or qword views; `q` does not force
a byte-sized value. Pointers are accepted. Byte operands can use the low-byte
views of extended registers and RSI/RDI. Uppercase `Q`, which identifies a
different class, remains unsupported. This normalization does not implement
the restricted 32-bit `q` class or extend AArch64's constraint set.

The fixture adds four functions for write-only outputs, read/write values at
all scalar widths, early-clobber byte outputs forced beyond the legacy GPRs,
and pointer values. The native controls also corrupt a `q` output and a byte
operation. Earlier fixture and audit counts above belong to their named
checkpoints.

All 145 fresh objects passed native GCC 13.3.0 comparison for this 39-function
fixture, with all ten broken controls rejected normally. The
[unchanged-input kernel audit](../suite/linux-kernel/audit-q-registers.json)
removes all 254 prior `=q` first errors. Those files now stop at the `%a`
template modifier. Acceptance remains four C and 402 assembly inputs, and all
406 previously accepted objects remain byte-for-byte identical. There are no
crashes, timeouts or acceptance regressions.

At implementation checkpoint `b60de289`, the full workspace passed 2,767 tests,
zero failures and 13 ignored across 507 groups. Workspace and profiling checks,
all 98 generator tests, three audit tests, both generator freshness checks,
scoped formatting and diff checks also passed.

## Function-scope basic assembly

On x86-64 Linux, basic asm without colons accepts arbitrary assembler text.
Percent signs are literal, including register names such as `%rip`; `%0`, `%%`
and `%=` have no substitution meaning. The frontend escapes them when storing
the text in IR's extended-template representation. The shared assembler still
checks emitted instruction spellings, directives and feature requirements.
Unused inline definitions do not require their assembler text to be emitted.

Basic asm stays pinned. Nonempty templates carry memory and flags clobbers,
so optimization cannot reuse a global load across an opaque assembly write.
As in the [GNU basic-asm contract](https://gcc.gnu.org/onlinedocs/gcc-15.1.0/gcc/Basic-Asm.html),
the compiler assumes GPRs are preserved. Assembly must preserve the C stack and
register state and fall through. Use extended asm to describe register inputs,
outputs and changes. Opaque bytes do not establish instruction validity or
runtime support. AArch64 retains its empty-or-`nop` basic-asm subset.

## File-scope assembly

On x86-64 Linux, `asm("...");`, `__asm("...");` and `__asm__("...");`
accept adjacent narrow string literals. They use GNU basic-asm syntax: percent
signs remain literal and there are no operands, clobbers or template substitutions.
Qualifiers and extended-asm colons are rejected at file scope.

The module stores ordered fragments as `module_asm "..."` in text IR. They
survive optimization and round-tripping, and precede generated definitions in
both text and typed output. The fragments share AT&T syntax, macro definitions,
section state and numeric labels. Generated code resumes in `.text` with Intel
syntax. GNU preprocessing, feature checks, symbol binding and relocation handling
remain in the shared assembler. Unsupported directives still produce errors;
Keel does not forward object assembly to an external tool.

Static C definitions referenced only by opaque assembly need `used`, as in the
[GNU basic-asm contract](https://gcc.gnu.org/onlinedocs/gcc-14.1.0/gcc/Basic-Asm.html).
File-scope assembly is currently rejected by the AArch64, WebAssembly, LLVM bridge
and Cranelift bridge paths. This does not provide source-order placement between
C definitions, top-level extended asm or additional instruction support.

ELF section attributes now admit ordinary `.init.*` and `.fini.*` names rather
than treating them as special ELF sections. Known kernel text/data/rodata lifetime
sections validate their storage classes. Zero-initialized `.init.data`,
`.exit.data` and `.meminit.data` objects occupy PROGBITS bytes and can share a
section with initialized data. Special ELF names such as `.init_array`, TLS and
COMDAT retain their existing checks.

Section annotations on external declarations are accepted without allocating
storage or carrying placement into imported call contracts. A later definition
inherits the annotation and receives the ordinary storage checks. Reserved
section names, conflicting redeclarations and retain-without-definition still
produce diagnostics.
