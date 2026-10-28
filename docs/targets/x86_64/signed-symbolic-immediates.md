# Signed symbolic x86 immediates

AT&T `movq $symbol+addend,%r64` now selects the existing
`CoreOpcode::MovImm` adapter. Its generated form selects the ModRM immediate
and emits `R_X86_64_32S`. No opcode identity, instruction-specific encoder or
ISA table was added. The old AT&T parser rejection was stale after the
generated encoder migration.

The exact adapter preserves the field constraint through Intel formatting
(`mov.imm r64, offset symbol`), typed programs and alias folding. `movabsq`
keeps its full-width `R_X86_64_64` relocation, and `movl` keeps unsigned
`R_X86_64_32`. A signed symbolic form does not silently widen when a resolved
value is outside its range; use the full-width spelling when needed.
Previously defined absolute names can instead reduce to numeric operands during
GNU preprocessing, allowing the ordinary numeric MOV selection. For example,
an absolute `0x80000000` selects the full-width encoding, matching GNU as.

Sign-extended fields recognize an unsigned 64-bit spelling of a negative
machine word, such as `0xffffffff80000000`. This normalization applies only
where a signed field can encode that word. It does not make a narrow unsigned
operand accept arbitrary 64-bit constants. Absolute ELF64 aliases also retain
negative machine-word values in their symbol-table representation.

Resolved symbol differences retain signed range checking. An external symbol
minus a local symbol in the current section can use the existing PC-relative
32-bit relocation transformation. Absolute aliases used by those relocations
keep their symbol identity. Differences that ELF cannot represent remain errors.

`crates/keel-asm/tests/x86_signed_immediates.rs` checks literal opcode bytes and
relocation offsets/types/addends, all 16 GPRs, addends, the existing TPOFF form,
typed/text parity and forward/backward absolute aliases. Independent GNU and
Clang objects must match. GNU ld tests all three producers at the signed-range
boundaries, including high-half kernel addresses and expected overflow failures.
These tests assemble and link objects; they do not execute kernel code.
