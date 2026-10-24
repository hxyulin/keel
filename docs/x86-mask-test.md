# Adjacent i32 mask/zero TEST selection

The x86 backend selects `TEST r32, imm32` for an i32 AND with a literal mask
whose only use is an Eq/Ne comparison against zero:

```text
v2 = iconst.i32 15
v3 = and v1, v2
v4 = iconst.i32 0
v5 = icmp ne v3, v4    ; -> test edi, 15 ; setne / jne / cmovne
```

This is an x86 backend selection change only: no frontend, optimizer, ISA-level,
ABI, FP, memory-width or default-target change. AArch64 selection is unchanged.
The [userspace ISA](userspace-isa-status.md), [C intrinsic](c-x86-intrinsics.md)
and [GNU vector/ABI](gnu-vector-mode.md) contracts still apply.

## Selection contract

`Selection::fold_x86_mask_tests` runs after the supplied selector and before the
existing compare/branch/select fusion. It recognizes:

- an exact i32 AND with a literal i32 mask, in either input order;
- whose result has exactly one resolved use;
- which is an Eq/Ne comparison against literal zero in the same block.

Only intervening constants are permitted. Nothing traverses a call, load, store,
fence, intrinsic, raw expansion, FLAGS region or block boundary.

Excluded:

- existing covers, machine matches, compare-condition covers and memory-RHS
  covers on the AND or compare;
- constraints on the AND, compare or intervening constants;
- debug functions, variables and instruction locations (these keep the
  materialized path);
- removed/aliased definitions and previously covered source producers;
- shared AND results, other predicates, nonzero comparisons, register masks and
  i8/i16/i64 widths.

## Lowering

The recipe stores the literal mask bits separately and publishes the resolved,
unmasked source as the compare's allocation operand. The encoded zero RHS is
recorded as already folded, so later folding cannot remove an equal-valued
source operand. Existing branch/select fusion transfers this leaf to its own
live boundary; the original source stays live for any other users.

`x86_64::compare` reloads the source normally and emits register
`TEST r32, imm32`, returning Eq/Ne's ZF condition to the existing SETcc/Jcc/CMOV
machinery. No new flag lifetime, memory form, narrowed TEST or alternate
predicate is introduced. User loads, extensions and stores stay at their
original positions and widths; spill completion, boolean canonicalization and
ABI transport are unchanged.

## Testing

```sh
cargo test -p keel-codegen \
  --test x86_mask_test --test x86_compare_select \
  --test x86_memory_cmp --test x86_memory_rhs
```

Repeat with `--release`. `crates/keel-codegen/tests/x86_mask_test.rs` covers
zero, all-one, sign-bit, alternating, small and arbitrary masks; Eq/Ne; both mask
input orders; resolved SSA source aliases; materialized booleans, branch and
select fusion, shared conditions, nonadjacent consumers and calls after boolean
capture; and composition with the [signed byte load](x86-signed-byte-load.md)
cover. All three selectors, both allocators, normal/one-register/empty pools and
v1/v2/v3 must produce identical text/typed/chunked objects, with no raw text
fallback and per-instruction feature legality. An ARM control retains AND.

On x86-64 Linux the suite also executes 100 functions in 54
pipeline/feature/pool configurations, linked with both GCC and Clang. Unsigned C
oracles check extrema and random inputs; calls overwrite all caller-saved GPRs;
byte/word/dword reads are checked at both protected-page edges, including
faulting crossings with marker stores proving the fault position.

## Application impact

Static counts from QuickJS, Lua and SQLite compiled at Full optimization (v1,
v2 and v3; optimized IR is unchanged):

| Translation unit | New register TESTs | TU `.text` change              |
|------------------|--------------------|--------------------------------|
| Lua `lvm.c`      | 13 (11 in `luaV_execute`) | unchanged (39,732 bytes) |
| QuickJS `quickjs.c` | 50              | -96 bytes (v1/v2), -112 (v3)   |
| SQLite `sqlite3.c`  | 66              | -96 bytes (v1/v2), -112 (v3)   |

Counts map each emitted TEST to its function's encoded bytes with an immediate
ZF consumer; raw linear disassembly is unreliable here because it decodes inline
jump tables as instructions. Byte changes include allocation and alignment
effects; an individual TEST does not necessarily save bytes (for example
`and edi,15; cmp edi,0` and `test edi,15` are both six bytes).

Rebuilt Lua (`all.lua`), QuickJS (closure/language/builtin/loop/bignum tests)
and SQLite (query corpus) pass with both GCC and Clang links. No runtime timing
was measured, and there is no speedup or general code-size claim.
