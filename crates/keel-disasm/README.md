# Native disassembly

`cargo run -p keel -- disasm input.o` prints executable sections from a
little-endian AArch64 or x86-64 Mach-O or ELF file. Each line includes the section address,
original bytes and assembly. Defined text symbols appear as labels. Use
`-o output.txt` to save the listing. No external disassembler is invoked.

The AArch64 `decode_word([u8; 4], address)` API returns
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

## Object handling

Addresses in relocatable objects are section-relative. Relocations are not
applied or symbolized, so an unresolved call prints its encoded target, not the
linker's eventual destination. Literal pools and ELF mapping symbols are not
interpreted. AArch64 text is decoded in four-byte units, including embedded
data. Mach-O universal binaries, archives and big-endian input are unsupported.

## x86-64

`x86::decode(bytes, address)` uses the shared generated legacy/VEX forms in
`keel-asm-ir`. The assembler uses the same forms. Returned instructions include
original bytes, source form metadata and structured registers, immediates,
branch targets and memory operands. Decode reads at most 15 bytes. Unknown,
invalid and truncated input returns `None` without claiming a length.

The CLI stops decoding that section at the first unknown instruction and prints
its remaining bytes as `.byte` data. It does not assume that byte-by-byte
recovery finds instruction boundaries. EVEX, MVEX, XOP, 3DNow and REX2 are outside
the current import. Recognition is an inventory match, not full architectural
validity, CPU feature detection or a runtime privilege check. Original bytes
preserve exact encodings even when assembly text chooses an alias or omits an
ignored prefix.

See [shared x86 tables](../../tools/x86-isa/README.md) for provenance,
regeneration, feature policy and the boundary with compiler semantics.
