# GNU C inline assembly: first frontend milestone

C assembly statements lower to the existing `InlineAsm` IR on
`aarch64-apple-darwin`. The x86-64 C frontend diagnoses assembly statements;
its IR backend also still rejects inline assembly. This is a bounded GNU C
subset, not general GCC constraint compatibility.

```c
long add(long a, long b) {
    long result;
    __asm__ volatile inline("add %x[out], %x[left], %x[right]"
        : [out] "=&r"(result)
        : [left] "r"(a), [right] "r"(b)
        : "cc");
    return result;
}
```

## Syntax and constraints

- `asm`, `__asm`, and `__asm__` statements accept narrow string literals,
  including adjacent literals. `volatile` and `inline` and their GNU spellings
  are accepted. All assembly is pinned, including non-volatile assembly:
  dead outputs do not remove it. `inline` currently has no size-hint effect.
- Basic assembly supports an empty template or `nop` only. Other basic templates
  diagnose: this stage requires extended assembly to declare register effects.
- Extended assembly accepts optional output, input and clobber lists, including
  empty lists: `asm("" ::: "memory")` is a pinned compiler memory barrier.
  It does not emit a hardware memory fence.
- Register outputs accept `=r`, `+r`, `=w`, `+w`, optionally with `&` immediately
  after `=`/`+`. `r` takes integer objects of 1, 2, 4 or 8 bytes and pointers;
  `w` takes `float` or `double`. Output expressions must be modifiable lvalues.
  Const/atomic objects, bit-fields and vector elements diagnose.
- Inputs accept `r` and `w` with the same value types, `m` for an addressable
  object lvalue (passed as its address), and `i`/`n` for an integer constant
  expression through 64 bits. Memory inputs may name arrays. The frontend
  materializes local memory operands in memory; it does not load the object
  to obtain the operand. An immediate is folded to an IR integer constant;
  runtime or side-effecting expressions do not become immediates.
- Matching input constraints use an output number (`"0"`) or output name
  (`"[out]"`). Input and output must have the same IR type; no implicit
  widening conversion is applied. An output may have only one tied input.
  `+` supplies that input itself, after the explicit inputs.
- Output lvalue locations are evaluated once, before explicit inputs, and reused
  for read/write initialization and writeback. Operand evaluation follows C's
  unspecified ordering; code must not depend on an order between independent
  operand expressions.
- Operands can have unique `[name]` declarations. `%0`, `%1`, etc. number all
  outputs followed by explicit inputs. `%[name]` and `%w[name]` become numeric
  substitutions, preserving the closing bracket when a literal digit follows
  (for example, `%c[imm]0` appends `0` to the substituted immediate).
  Hidden `+` inputs cannot be referenced by template numbers.
  `%%` is a literal percent. AArch64 views `%w`, `%x`, `%s`, `%d`, `%q`, `%h`,
  `%b`, and immediate `%c` use the existing backend checks. With no modifier,
  registers use the operand's type, memory becomes `[xN]` and immediates become
  decimal numbers. Include `#` in the template when the instruction needs it.
- Clobbers accept `memory`, `cc`, AArch64 `x0`–`x30`/`w0`–`w30` and
  `v0`–`v31` (also `q`, `d`, `s`, `h`, `b` views and `fp`/`lr` aliases).
  x18 and x29/fp are rejected. x19 is rejected by codegen in functions using
  dynamic stack allocation. Unknown names diagnose. `memory` and `cc` remain
  explicit IR clobbers; all asm conservatively invalidates memory knowledge.

## Backend contract and boundaries

The AArch64 backend places operands in its reserved operand pools: x0–x8,
x16–x17, v0–v7 and v16–v18, excluding declared clobbers. Distinct outputs and
untied inputs use distinct registers, so early clobbers are respected.
Tied inputs initialize their output registers. Register exhaustion and invalid
template/type combinations are codegen diagnostics, never discarded operands.
Caller-saved allocatable register clobbers induce allocator call barriers;
callee-saved clobbers are excluded from allocation and saved/restored.

The programmer must declare every register/memory/flag effect, initialize
outputs, and preserve the stack and normal fallthrough control flow. This stage
does not model calls or transfers to C labels inside templates. Template
instructions must also be supported by the AArch64 assembler; successful C
parsing alone does not guarantee object assembly. Use `\n` to separate
instructions on Darwin, where `;` starts a comment.

Unsupported forms diagnose: asm-goto (including a fourth colon/label list),
memory outputs, constraint alternatives/unions (`r,m`, `rm`), fixed-register
constraints and local register-variable asm labels, flag outputs, vector or
128-bit register values, symbolic immediates, and unimplemented template escapes
such as `%=` and label substitutions. Symbol asm-label declarations for
functions and objects with static storage retain their existing behavior.

Next stages are memory-output IR contracts, more target constraints and local
register variables, unique-label/template features, asm-goto with explicit CFG
edges and scope handling, and a separate x86 constraint/allocation/backend stage.

## Validation

`cargo test -p keel-c --test inline_asm` checks source diagnostics and backend
legality errors, verifies IR before/after optimization, checks pinned contracts
and emits actual Mach-O objects. On native Apple AArch64 it links and executes
those objects against a Clang `-O2` oracle in all four combinations of debug
metadata and optimization. Cases cover arithmetic, narrow integer and floating
registers, numeric/named ties, read/write and multiple outputs, side effects in
output addresses and input expressions, named views, constants, memory inputs,
caller/callee-saved register clobbers and a memory barrier.

Existing C program tests (including symbol renaming) and the IR backend's
`inline_asm.keel` native differential fixture remain separate regression suites.
