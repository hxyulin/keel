# x86-64 C inline assembly

Keel supports scalar and SIMD GNU C inline assembly on `x86_64-unknown-linux-gnu`,
including explicit memory operands, bounded constraint choices, integer flag
outputs and asm goto. Basic asm is supported at function and file scope.
Comma-separated alternatives are unsupported.

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

Register operands accept integers through 64 bits, pointers and the bounded
aggregate representation described below. Register constraints are
`r` and x86-64 `q`, and fixed `a`, `b`, `c`, `d`, `S`, `D` for RAX, RBX, RCX, RDX, RSI, RDI.
SIMD constraints `x` and `v` accept `float`, `double`, and the frontend's
128/256-bit vector types. Both use registers 0–15; `v` does not enable EVEX
register allocation. The shared IR represents them with `w` (`FloatReg`) and
also accepts 64-bit vectors. Scalar integer SIMD operands, MMX, x87, ZMM and
registers 16–31 are outside this operand contract. Named local-register
variables are GPR-only.

SIMD templates use `%xmmN` for scalar and 64/128-bit operands, `%ymmN` for
256-bit operands. `%x0` selects the XMM view; `%t0` selects the YMM view of a
256-bit operand. GPR width modifiers cannot select SIMD register views.
XMM/YMM spellings alias the same physical register in explicit IR operands and
clobbers. A 256-bit operand requires AVX; instruction-specific requirements
(such as AVX2 for `vpaddd`) remain checked by the assembler.

Inputs are snapshotted before any asm operand register is initialized. Results
are captured before original register homes are restored. SIMD staging and
clobber preservation retain the whole live payload, including YMM upper halves;
result writeback uses each result's own width. Matching SIMD inputs/results
require identical IR types. Early-clobber, conflicting-register and clobber
checks apply independently to GPR and SIMD registers. Empty/comment-only
volatile templates retain opaque results and their declared memory effects.

Outputs use `=` or `+`, optionally followed by early-clobber `&`. Matching input
constraints use an output number or `[name]`. Scalar GPR inputs and results
may have different widths and keep their own register views; a tie does not
promise an extension into unspecified input bits.
Output addresses and input expressions are evaluated once. A read/write register output
captures its initial value and writes its final value to that captured address.
Untied operands receive distinct registers, even when GNU could reuse them.
Fixed-register sharing follows the [shared register rules](#shared-registers).
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
define these numeric predicates. Standalone `e`/`I`, comma-separated alternatives
and memory retries after scalar register exhaustion are unsupported. Keel does
not infer instruction-specific immediate ranges from templates.

Inputs also accept `Nd`/`dN` for x86 ports. Constants from 0 through 255 use
an immediate; other values use RDX. Selection happens after required inlining
and constant cleanup, including without `-O`. Fixed-register conflicts and
clobbers still apply to the RDX alternative. `%w` prints DX for a register
operand and preserves the normal `$` prefix for an immediate, matching the
kernel's `in`/`out` templates. Width modifiers `%b`, `%w`, `%k` and `%q` also
preserve numeric immediate values. Standalone `N` remains unsupported. The
[GCC x86 constraint definitions](https://gcc.gnu.org/onlinedocs/gcc/Machine-Constraints.html)
define the unsigned-byte `N` alternative.

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

Clobbers accept `cc`, `memory`, generated GPR names/aliases, and XMM/YMM
registers 0–15. High-byte names AH/CH/DH/BH clobber their parent
RAX/RCX/RDX/RBX registers. RSP and RBP cannot be clobbered. R15 is also
reserved in functions whose aligned frame needs it as a base pointer.
Control/debug, tile, opmask and other register classes are outside the
operand and clobber contract. The IR keeps target-tagged physical registers and
early-clobber markers, including in textual round trips.

### Shared registers

One physical register has an input lifetime before the template and an output
lifetime after it. A non-early fixed output may share an input register without
an explicit matching constraint. Each output still needs its own register. A
fixed independent input cannot share an early-clobber output, but an explicit
matching input may initialize it. Clobbers cannot overlap operands.

Several inputs may share a fixed register or match one output when their
common low bits are provably equal. `asm::same_register_input` resolves SSA
aliases, strips scalar conversions that keep the observed bits, and compares
numeric constants modulo the common width and equal symbolic addresses.
Separate loads, unrelated parameters, the same C variable name and runtime
coincidence do not prove equality. Distinct volatile loads stay distinct.

The widest input initializes the register once. Narrower inputs keep their own
register views and need only their low bits. Neither a wider output nor a
template modifier extends a narrow input. Multiple output writes, incompatible
initializers and independent early-clobber overlaps are errors. The C parser
keeps repeated matching constraints, the IR verifier checks every pair of
matching inputs, and the x86 planner checks all inputs assigned to each
physical register. AArch64 still rejects multiple inputs in one register.

### Aggregate register operands

Complete structs and unions of 1, 2, 4 or 8 bytes may use a single GPR. Inputs
load the unsigned bit representation, including initialized padding; returned
and compound record values are materialized once. Outputs capture a modifiable
object's address once and store exactly that width. Read/write outputs initialize
the register from those same bytes. Same-width integer/record ties share the
normalized scalar IR type. Packed accesses retain alignment 1; whole-object and
member volatile effects remain observable.

Small records may choose a GPR for `g` and register/memory unions. Nonword and
wider records use the memory alternative where allowed; explicit `m` keeps its
addressable-object contract. Const-member outputs are rejected in both register
and memory paths. Atomic-member register packing, empty or nonword GPR values
and multi-register modes are unsupported. Aggregate `i`/`n` inputs are
diagnosed rather than passing an address as their value.

Padding checks use initialized lvalue objects; ABI argument/return copies do
not promise to retain indeterminate padding. Clang rejects some tied-indirect
generic forms such as a nonword `+g`; explicit `+m` remains the memory spelling.
The `x86_aggregate_asm` fixture covers bit patterns, padding, packed/volatile
objects, returned values, evaluation count, memory fallback and the kernel's
`WRITE_ONCE` switch.

Read-only global `rsp` bindings have a separate
[C observer contract](../../frontends/c/extensions.md). Their exact `+r` operand supports
Linux's `ASM_CALL_CONSTRAINT` through the balanced stack contract below.

### Memory effects

All asm is pinned, even without `volatile`, and is never made pure, deleted by
DCE or moved by GCM. `inline` has no size-hint effect. Memory effects follow
the declarations; the compiler does not inspect the template to infer missing
effects:

| Declaration | Memory effect |
|---|---|
| Register or immediate operands and GPR or flags clobbers only | None |
| Read-only memory operands | Read |
| Write-only memory operands | Write |
| Read/write memory operands, or a mixture of reads and writes | ReadWrite |
| `memory` clobber or reserved-stack dependency | Barrier |

`memory-forward` and `load-reuse` keep facts across register-only and
read-only templates. Explicit memory writes invalidate facts conservatively,
including facts about other addresses. A barrier is a compiler ordering
constraint, not a hardware fence. Condition-code clobbers are separate from
memory effects. See GCC's
[extended asm contract](https://gcc.gnu.org/onlinedocs/gcc/Extended-Asm.html).

## Lowering and boundaries

The backend saves the GPR homes that the template may overwrite: declared
register clobbers, assigned operand registers and the RAX staging scratch.
Memory and flags clobbers add no GPRs. Saved homes occupy compact frame slots,
and only affected callee-saved registers join the prologue and unwind
descriptions. Register inputs are staged before any operand register is
filled; an input whose home is saved is read from its slot. Flag result slots
are zeroed before the template, and `SETcc` writes their low bytes right after
it. Outputs and flags are captured before old homes are restored, then SSA
results are installed. This handles tied and early outputs, fixed-register
cycles, rematerialized inputs, forced spills and values live across declared
clobbers without changing the allocator. Affected homes are saved even when no
live value uses them. SIMD scratch registers are not used for staging.

Templates without register operands, outputs or GPR clobbers need no staging
frame. This includes `asm volatile("sti" ::: "memory")`, compiler-only memory
barriers and immediate-only templates. The assembler still validates their
instructions and target features.

Substituted text remains an explicit fragment in the enclosing object program.
The shared assembler processes directives, macro state, labels, relocations and
CPU-feature legality when it consumes that program. Emitting assembly text alone
does not validate the template's instruction syntax or encoding. The compiler
restores Intel syntax for its own instructions after each AT&T fragment.

The programmer must declare every effect, define each output, preserve the
stack/frame bases and the direction flag required by the ABI. Ordinary asm must
return through normal fallthrough; local branches may stay within the template.
Asm goto may also transfer to its listed C labels. Balanced calls require the
reserved-stack operand below. Terminal transfers need separate contracts;
the compiler does not infer these effects by inspecting arbitrary instructions
or directive bytes. Section switches must return to the function's section.

Still diagnosed: comma-separated alternatives, unsupported constraint unions,
TLS-only immediates, SIMD local register variables and unsupported register
classes.

## Balanced stack calls

X86-64 Linux asm accepts one exact `"+r"` operand naming a bound global RSP
variable. This supports Linux's `ASM_CALL_CONSTRAINT`:

```c
register unsigned long current_stack_pointer asm("rsp");
extern void helper(void);

void invoke_helper(void) {
    asm volatile("call %c[target]"
        : "+r"(current_stack_pointer)
        : [target] "i"(helper)
        : "cc", "memory");
}
```

The template must preserve the enclosing frame bases and direction flag,
restore RSP on every normal path, and maintain SysV call alignment at each
call. Temporary stack adjustments and manually placed stack arguments are
allowed when balanced. The compiler supplies an aligned entry stack and an
ABI caller-clobber barrier, including live SIMD values. It does not infer call
arguments or return types from the template. Unwinding, nonlocal exits and
terminal transfers are outside this contract.

The RSP operand retains its GNU source number and name. It prints physical
`%rsp`, accepting the default or `%q` view. Narrow views, ordinary writes,
`=r`, early-clobber, duplicate stack operands, ties to the stack operand and
output-bearing asm goto are rejected. Fixed-register sharing, including
Linux's `=D`/`D` form, follows the [shared register rules](#shared-registers).

IR represents the dependency as an input-only `rsp` constraint with an `i64`
snapshot from `stack_save` followed by `ptrtoint.i64`. The verifier resolves
value aliases and checks that provenance, type, uniqueness and reserved-register
conflicts. There is no SSA stack result or assignment to RSP. The snapshot
establishes ordering; substitution reads live RSP after all operands have been
evaluated. A later input `alloca` therefore cannot rewind the stack. AArch64
rejects this x86 constraint.

The `asm_stack_call`, `x86_stack_call` and IR `stack_pointer_asm` tests cover
source remapping, verifier failures, text/typed/chunked parity, ABI call barriers,
dynamic/aligned frames, stack arguments and Linux alternative-call metadata.

## Asm goto

On x86-64 Linux, `asm goto` accepts existing input constraints and clobbers,
with numeric `%lN` and named `%l[label]` references to the listed C labels.
Numeric labels follow source inputs and outputs, counting each read/write
output twice. Input expressions and output addresses are evaluated once.
All asm remains pinned, including an empty template. Duplicate destinations
and backward edges use the ordinary C label machinery.

Each label reference names an assembly-local trampoline. The trampoline writes
an early-clobber selector output with `MOV` and converges with fallthrough
before the backend restores GPR homes. An ordinary IR switch then selects the
C successor. This exposes every possible C edge to SSA, optimization and
liveness without allowing a raw assembly branch to bypass restoration.
`%=` gives each emitted instance distinct labels, including cloned inline
helpers. Label addresses in metadata also name these restoration trampolines.

Register, flag and memory outputs use the same operand contracts as fallthrough
asm. Real register outputs are written back after restoration and before C
successor dispatch, including taken edges and backedges. The hidden selector is
not a source operand; matching constraints and template references are remapped
around it. Reserved-stack outputs remain rejected. Functions containing cleanup
locals or variably modified types reject asm goto under the existing scope-exit
rules.
AArch64 asm goto is unsupported.

`cargo test -p keel-c --test x86_asm_goto` checks diagnostics, ordinary CFG
edges, IR round trips and text/typed/chunked object parity across selectors,
allocators, normal/empty pools and optimization on/off.

## Validation

`cargo test -p keel-c --test x86_inline_asm` checks contracts and diagnostics,
pinned behavior, IR round trips, and text/typed/chunked object parity across
v1/v2/v3, three selectors, two allocators, normal/empty pools, optimization
on/off and debug info on/off.

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

Mandatory expansion keeps the binding, recursion, ABI and structural
legality checks. Missing definitions, noinline conflicts and unsupported calls
produce errors rather than profitability skips. Expansion is limited to one
million cloned instructions. Eight rounds of local constant cleanup are allowed
for pending immediates. Debug parameter homes can forward known stores into
loads; memory-writing barriers invalidate those facts. Ordinary optional
inlining is an optimizer decision.

For example, the kernel-style `rip_rel_ptr(&global)` helper can pass its pointer
parameter to an `i` operand. After required inlining, `%c1(%%rip)` becomes
`global(%rip)`. `n` rejects the same symbol. Unresolved runtime inputs fail final
IR verification before emission. There is no relaxed public verifier mode.

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

Memory operands set the template's [memory effect](#memory-effects); a
read-only `m` operand is a read.

## The x86-64 q register class

C inputs and outputs accept `q`, including `=q`, `+q` and the early-clobber
forms `=&q` and `+&q`. GCC's
[x86 constraint definition](https://gcc.gnu.org/onlinedocs/gcc/Machine-Constraints.html)
allows every integer register in 64-bit mode. The C frontend therefore
normalizes `q` to the IR `r` class.

Operand type still selects byte, word, dword or qword views; `q` does not force
a byte-sized value. Pointers are accepted. Byte operands can use the low-byte
views of extended registers and RSI/RDI. Uppercase `Q` and the restricted
32-bit `q` class are unsupported.

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
runtime support. AArch64 accepts only empty or `nop` basic asm.

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
The AArch64, WebAssembly, LLVM bridge and Cranelift bridge paths reject
file-scope assembly. There is no source-order placement between C definitions
and no top-level extended asm.
