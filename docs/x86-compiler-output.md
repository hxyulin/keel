# Bounded x86 compiler-output instruction support

These assembler additions let Keel assemble the Clang-generated Lua and SQLite
assembly used by the application suite. They extend the assembler and shared
assembly model only; instruction selection is unchanged, and this is not full
compiler-output compatibility.

`cmpeqsd`, `cmplesd`, `cmpneqsd`, and `cmpordsd` select legacy `cmpsd`
predicates 0, 2, 4, and 7. They take an XMM destination and an XMM or
64-bit memory source, without an explicit immediate. Existing `cmpsd` and
less-than aliases retain their behavior, including RIP relocation addends
that account for the trailing predicate byte.

The x87 register class represents `st`/`st(0)` through `st(7)` at 80 bits.
It cannot serve as a general register, address base, or index. The supported
instruction forms are:

- `fld` and `fstp`: one stack register or explicit 32/64/80-bit floating memory.
- `fxch`: one stack register, or no operands for `st(1)`.
- `fucomi` and `fucomip` (`fucompi` is an alias): explicit `st(0), st(i)`,
  one `st(i)` with implicit first operand `st(0)`, or no operands for
  `st(0), st(1)`.
- `fadd` and `fmul`: explicit register pairs containing `st(0)`, one `st(i)`
  with implicit destination `st(0)`, or explicit 32/64-bit floating memory.
- `fmulp`, `fsubp`, and `fsubrp`: explicit `st(i), st(0)`, one destination
  `st(i)`, or no operands for `st(1), st(0)`.
- `fcmovnb`: explicit destination `st(0)` and source `st(i)`.
- `fild` and `fistp`: explicit 16/32/64-bit integer memory.
- `fldcw` and `fnstcw`: 16-bit control-word memory. `fldz` has no operands.

The shared model and formatter use Intel destination/source semantics.
AT&T reverses explicit operand order, uses `s/l/t` floating-memory suffixes
and `s/l/ll` integer-memory suffixes, and infers the fixed 16-bit width of
control-word operands. GAS/LLVM AT&T `fsubp` maps to Intel `fsubrp`, and
AT&T `fsubrp` maps to Intel `fsubp`, including one-operand/default forms.
Register prefix modes also apply to x87 registers.

Floating/integer memory widths remain mandatory; ambiguous unsuffixed AT&T
memory forms are rejected. The encoder validates explicit register positions
instead of discarding operands to match permissive assembler aliases.
Other x87 instructions and unsupported directives remain errors. Existing
memory encoding supplies FS/GS, address REX, SIB, displacements and fixups;
x87 data widths never introduce REX.W or a 16-bit operand-size prefix.

## Testing

`cargo test -p keel-asm --test x86_compiler_output` compares `.text` and relocation offsets, ELF types,
addends and target names against installed Clang in both dialects. GNU as is
also checked when an existing x86 assembler is found (`KEEL_GNU_AS` overrides
discovery). The tests cover every stack index, both register destinations,
subtraction direction, explicit/default operands, all supported memory widths,
extended addressing, typed/text equivalence, formatting round trips, malformed
typed registers, and invalid operands. These are encoding checks; they do not
execute the x87 code natively.
