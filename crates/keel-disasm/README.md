# AArch64 disassembly

`cargo run -p keel -- disasm input.o` prints executable sections from a
little-endian AArch64 Mach-O or ELF file. Each line includes the section address,
original bytes and assembly. Defined text symbols appear as labels. Use
`-o output.txt` to save the listing. No external disassembler is invoked.

`keel-disasm` has no runtime dependencies. `decode_word([u8; 4], address)` returns
an `Instruction` with the original bytes and `Recognition::{Unknown, Unique,
Ambiguous}`. Unique matches expose the Arm inventory name and named bit fields.
`Instruction::assembly()` formats supported instructions and otherwise returns a
lossless `.inst 0x12345678` directive. Callers handle alignment and partial words;
the CLI prints trailing bytes as `.byte`.

The generated table contains all 1,670 records in `keel-asm/isa/a64.json`.
Regenerate it with `python3 crates/keel-asm/tools/generate.py`; use `--check` to
check freshness. Arm-derived data retains its BSD license in `LICENSE-ARM`.
All stored Boolean field predicates are evaluated. Feature predicates assume
all inventory features are available. Matches do not establish CPU support or
full architectural validity: the inventory omits constraints from architectural
decode pseudocode. The overlap test enumerates every relevant predicate-bit
assignment in each of the 81 intersecting mask pairs. Table order never selects
a winner; any unresolved matches remain `Ambiguous` and print as `.inst`.

Formatting covers scalar integer add/subtract, logic, move-wide, carry, divide,
variable shifts, selected unary operations, multiply, conditional select,
bitfield/extract, direct/conditional/register branches, ADR/ADRP, ordinary scalar
and FP/SIMD-register loads/stores and pairs, scalar FP arithmetic, NOP, BRK and
SVC. Familiar aliases include MOV, CMP, CMN, TST and MUL. Other instructions,
including vector arithmetic, SVE, SME, atomics and most system instructions,
print as `.inst` even when recognized. This is a bounded formatter, not a
complete ISA disassembler.

Addresses in relocatable objects are section-relative. Relocations are not
applied or symbolized, so an unresolved call prints its encoded target, not the
linker's eventual destination. Literal pools and ELF mapping symbols are not
interpreted. Text sections are decoded in four-byte units, including embedded
data. Mach-O universal binaries, archives and big-endian input are unsupported.
