# Bounded scalar if-conversion

`if-convert` is an opt-in function pass. No default pipelines, allocator modes
or existing compatibility options change. It admits a branch whose two distinct,
single-predecessor arms end in jumps to the same join. Each arm may be empty.
Both arms together may have at most eight speculated instructions and the join
at most eight parameters. A conditional with two argument-carrying edges directly
to the same join is also supported. Entry/self-target arms and a join equal to
the decision block are rejected.

The pass substitutes incoming arm parameters simultaneously, moves the admitted
instructions before the decision, and selects each outgoing join argument using
the original nonzero condition. Identical arguments need no select. Signedness
and width remain in the original integer operations/comparisons; arithmetic still
wraps. Moved instructions retain source locations. Canonical integer/pointer
parameters and canonical instruction result definitions are required. Arm
values used outside their own arms are excluded, including unreachable users.

The explicit speculation allowlist contains integer constants, compares, selects,
wrapping add/subtract/multiply, integer logic, masked shifts/rotates, high multiply,
integer bit-count/negate/swap operations and integer extensions/truncations.
Loads, stores, atomics, calls (including returns-twice and indirect calls), division
and remainder, floating operations, frame operations, inline assembly, exception
operations and traps are excluded. New IR opcodes do not enter implicitly.

## Suggested schedule

For Full, the intended segment is:

```text
inline,small-memory,simplify-cfg,stack-promote,phi-simplify,dce,
if-convert,simplify-cfg,canonicalize,instcombine,phi-simplify,dce
```

Inlining and intrinsic initialization expansion expose private fields; phi
cleanup exposes small scalar diamonds; cleanup after conversion removes
redundant parameters and jumps. Keep the pass out of Fast until compile cost and
register pressure have been evaluated.

Select formation is sufficient for branchless scalar backend code, but general
multiple-accumulator vector reduction and select/mask vectorization are further
work. The real `k_stackobj` corpus kernel additionally needs the escaped-field
region/alias proof described in [stack-promote.md](stack-promote.md); it still
has seven loads, three stores and two conditional branches.

## Effect on the reduced fixture

On the combined reduced private fixture, `stack-promote` plus `if-convert`
removes six field loads and six field stores (including initialization),
leaving one input load, and reduces two conditional branches to the single
loop-exit condition. ARM assembly changes `b.lt` to two `csel`; x86 assembly
uses two `cmovne`. ARM's allocator adds one callee-save `stp`/`ldp` pair after
conversion, so this eliminates field traffic, not all stack traffic. No
performance measurements were made.

## Tests

```sh
cargo test -p keel-opt --test if_convert
cargo test -p keel-opt --test if_convert_nested
```

Coverage includes all four integer widths, wrapping, signed/unsigned compares,
nonzero condition polarity, simultaneous edge permutations, empty arms,
duplicate edges, pointers, alias-definition rejection, source locations,
bounded work, partial initialization, debugger homes, escapes, volatile/atomic
accesses, exception/returns-twice/frame barriers and untaken trapping division.

The native validator in `crates/keel-opt/tests/stack_promote/validate.py` (see
[stack-promote.md](stack-promote.md#tests)) accepts `--if-convert`. With it, the
validator emits ARM64 and x86-64 assembly and objects for five private-loop and
four diamond/guard variants, runs 10,401 unsigned-C private-loop cases per ARM
variant and 90,000 width/edge/guard cases per diamond variant, checks taken
native traps (division by zero, signed division overflow, null load) and
untaken ones, and matches all 795 whole-driver lines against Clang O2 for
Fast and Full with both passes. Only ARM64 macOS executes native code; x86 is
checked statically.
