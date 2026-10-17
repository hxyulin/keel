# Adjacent x86 memory comparisons

Tree selection can encode the right operand of an integer comparison directly
as memory instead of materializing an adjacent load:

```text
v2 = load.i64 v0
v3 = icmp.ult v1, v2
```

This becomes `cmp register, qword ptr [address]`. The initial implementation is
limited to ordinary 64-bit loads, a single resolved use, and immediately
adjacent instructions in the same block. It preserves the original subtraction
operand order and therefore every signed/unsigned comparison condition. The
load still reads exactly eight bytes at its original position in the IR effect
order; neither alignment nor `notrap` is assumed. No intervening instruction,
including a pure instruction or another read, is crossed.

Volatile and atomic operations, shared results, left-hand loads, narrow or
128-bit loads, aliased result definitions, and debug-located functions keep the
existing lowering. A function with source locations, a declaration location,
or debug variables is excluded so independently breakable source positions and
debug materialization do not change. Baseline selection and the ARM target are
unchanged. Trees/full selection enable the rule, including the normal `-O`
selection path.

Selection transfers the load's actual address leaves to the comparison before
allocation. Existing pointer-offset selection remains usable. Compare/branch
fusion then transfers the same leaves to the branch. Both register allocation
tiers and fully spilled inputs use separate scratch registers for the integer
left operand and memory address. The text and structured-object emitters share
the same selected CMP; no opaque assembly peephole or new instruction is added.

The motivating manual QuickJS experiment measured 14.14% less process wall time
on one AMD session. The NUC experiment was inconclusive. Those experiments
include function and linked-code layout changes and establish neither isolated
CMP latency nor the profitability of every application of this compiler rule.
This implementation deliberately keeps ordinary memory and debug semantics
bounded; it does not introduce CPU-specific profitability tuning.

`x86_memory_cmp` tests all ten conditions, branch arguments, materialized
booleans, address offsets, both allocation tiers, and forced spills. It also
checks fallback boundaries and direct versus text-assembled objects. Its Linux
native test compares independently compiled Clang arithmetic oracles and checks
a valid final eight-byte access plus a protected-page crossing fault.
