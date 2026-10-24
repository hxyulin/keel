# Adjacent x86 integer memory operands

Tree selection can encode the right operand of an integer comparison directly
as memory instead of materializing an adjacent load:

```text
v2 = load.i64 v0
v3 = icmp.ult v1, v2
```

This becomes `cmp register, qword ptr [address]`.

## Comparisons

Ordinary i8, i16, i32 and i64 loads can fold, with a single resolved use and
immediately adjacent instructions in the same block. It preserves the original subtraction
operand order and therefore every signed/unsigned comparison condition. The
load still reads exactly its original byte width at its original position in
the IR effect order; neither alignment nor `notrap` is assumed. No intervening instruction,
including a pure instruction or another read, is crossed.

Volatile and atomic operations, shared results, left-hand loads, 128-bit loads,
aliased result definitions, and debug-located functions keep the
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

## Arithmetic with a memory RHS

The same adjacency and effect guards permit an integer arithmetic RHS to use
memory: `iadd`, `isub`, `and`, `or`, and `xor` at i8/i16/i32/i64 widths, and
`imul` at i16/i32/i64 widths. The two-operand IMUL encoding has no byte form,
so i8 multiplication stays materialized. No operand commutation is needed;
subtraction retains `lhs - loaded_rhs`. Narrow arithmetic accesses its exact
byte width and explicitly zero-extends the modular result before moving or
spilling it. Signed extensions and signed comparisons continue to interpret
the low IR-width bits.

The selected ALU publishes its lhs and every selected address leaf before
allocation, preserving duplicate occurrences when lhs also supplies the index.
When result and lhs coalesce to the same register, that register is the ALU
accumulator. Memory address reloads touch only reserved RCX/RDX; the ALU reads
all address inputs before writing its result, so a coalesced lhs may also supply
an address leaf. Otherwise the accumulator uses reserved RAX and copies to the
allocated destination only after the memory operation. Thus a distinct destination
overlapping an address leaf cannot destroy the address before the read. Narrow
normalization uses the actual accumulator. This holds for Fast/Full allocation,
forced spills, and experimental rematerialization. Existing larger tree covers
retain precedence. Trees disabled, unsupported widths, debug functions, or any
intervening instruction retain the existing load and arithmetic lowering.
A shared i32 load used by a comparison and a shift store remains one materialized
read; selection never duplicates it to create a memory operand.

## Indexed addresses

Indexed scalar memory selection also composes with this rule. A single-use,
same-block `ptradd` with an i64 offset can become `[base + index * scale]`
at the original load or store. A single-use i64 shift can supply scales 1, 2,
4, or 8 after masking its count modulo 64. Scalar integer, pointer, and float
accesses keep their exact 1/2/4/8-byte widths; atomic and 128-bit/vector accesses
remain excluded. Shared or cross-block addresses stay materialized, and narrow
shifts/sign extensions retain their own arithmetic. Address producers respect
the existing file/line source boundaries. Volatile accesses retain their
original effects and ordering, while volatile loads remain excluded from CMP
folding.

Both address leaves are allocation operands at the memory consumer, including
when that consumer becomes a comparison or fused branch. Indexed address reloads
use reserved RCX and RDX separately; integer store data, ALU accumulators, and
compare left operands remain in RAX or allocated registers. The backend validates that allocation pools
exclude these scratch registers, and rematerialization writes only its requested
scratch. A load destination can overlap either allocated address leaf because
the memory instruction reads its address before writing the result.

## Testing

```sh
cargo test -p keel-codegen \
  --test x86_memory_cmp --test x86_memory_rhs --test x86_indexed_memory
```

- `x86_memory_cmp` tests all ten conditions, branch arguments, materialized
  booleans, address offsets, both allocation tiers and forced spills, fallback
  boundaries and direct versus text-assembled objects. Its Linux native test
  compares against independently compiled Clang oracles and checks a valid final
  eight-byte access plus a protected-page crossing fault.
- `x86_memory_rhs` checks arithmetic/compare widths, subtraction order, lhs/index
  liveness, source/effect barriers, shared and resolved aliases, allocator
  pressure, disabled options and LLVM assembler parity. Its C oracle runs across
  seven selector/allocator settings, including calls, exact-width page
  boundaries, fault ordering, signed narrow normalization, materialized and
  branched comparisons, and stack addresses.
- `x86_indexed_memory` checks all scalar widths, legal scales, masked counts,
  selection options, both allocators, forced spills, overlap, source boundaries,
  fallbacks, optimizer interaction and LLVM assembly encoding. Its C oracle
  checks modular address wrap, calls across address formation, byte-exact
  stores, floats and protected-page faults and ordering.

The C oracles run on Linux x86-64 or macOS arm64. On arm64 they validate only the
oracle and the unchanged ARM lowering; native x86 correctness requires the Linux
run. Deliberate-fault children set `PR_SET_DUMPABLE=0` so no crash reporter is
involved.

## Performance

A manual QuickJS experiment that motivated the CMP fold measured 14.14% less
process wall time in one session on an AMD x86-64 machine; a second machine was
inconclusive. Those runs include function and linked-code layout changes and do
not establish isolated CMP latency or the profitability of every application of
the rule. No CPU-specific profitability tuning is applied, and no performance
improvement is claimed for the arithmetic and indexed-address simplifications.
