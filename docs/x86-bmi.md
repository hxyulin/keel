# x86 BMI implementation contract

The encoding authority is Intel's *Intel 64 and IA-32 Architectures Software
Developer's Manual*, Volume 2A (ANDN, BEXTR, BLSI, BLSMSK, BLSR, BZHI) and
Volume 2B (MULX, PDEP, PEXT, RORX, SARX/SHLX/SHRX, TZCNT), instruction operand
encoding tables and operation pseudocode:

- https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf
- https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf

Volume 1 section 5.1.16.1 defines independent BMI1 (CPUID.7.0:EBX[3]) and BMI2
(EBX[8]) detection. These VEX GPR instructions do not require AVX or OS AVX
state enablement. TZCNT is gated even though its encoding decodes as BSF on
older processors: the zero semantics differ.

## Assembler forms

All operands use matching 32- or 64-bit GPR widths. One source can be memory,
whose size is inferred from the destination if omitted. Destination is always
a register. Eight- and sixteen-bit forms are rejected, including the otherwise
architecturally valid 16-bit TZCNT form. Addressing uses the existing 64-bit
address, SIB, displacement, FS/GS, RIP-relative, and relocation machinery.
No address-size override or XOP/immediate-control BEXTR is introduced.

| Instructions | Intel operand order | ModRM.reg | VEX.vvvv (logical) | ModRM.r/m | Map / pp / opcode |
| --- | --- | --- | --- | --- | --- |
| ANDN | dst, inverted-src, src | dst | inverted-src | src | 0F38 / none / F2 |
| BEXTR | dst, src, control | dst | control | src | 0F38 / none / F7 |
| BLSI/BLSMSK/BLSR | dst, src | /3, /2, /1 | dst | src | 0F38 / none / F3 |
| SHLX/SHRX/SARX | dst, src, count | dst | count | src | 0F38 / 66,F2,F3 / F7 |
| RORX | dst, src, imm8 | dst | reserved (encoded 1111) | src | 0F3A / F2 / F0 |
| BZHI | dst, src, index | dst | index | src | 0F38 / none / F5 |
| PEXT/PDEP | dst, src, mask | dst | src | mask | 0F38 / F3,F2 / F5 |
| MULX | high-dst, low-dst, src | high-dst | low-dst | src | 0F38 / F2 / F6 |
| TZCNT | dst, src | dst | n/a | src | legacy F3, REX.W as needed, 0F BC |

VEX encodes inverted R/X/B and vvvv; W selects 64 bits, L is zero, and
BLS*'s unused R extension is encoded one. RORX accepts literal 0..255 only
(no symbolic relocation or negative immediate); hardware masks to 5/6 bits.
The focused helper shares memory emission with the legacy encoder. RIP-relative
relocation tests account for RORX's trailing immediate.

MULX reads EDX/RDX implicitly, writes the high and low destinations in Intel
order, and preserves flags. The assembler accepts aliases; codegen does not
select MULX or change multiplication clobbers. PEXT/PDEP support is an encoding
capability, without a performance claim or codegen selection.

## Codegen changes and assumptions

With BMI2 enabled, nonconstant SHL/USHR/SSHR use SHLX/SHRX/SARX. The count
remains in its allocated GPR, avoiding RCX moves for 32/64-bit register inputs.
Hardware masks counts modulo 32/64. For i8/i16 the count is copied into reserved
scratch RCX and masked modulo 8/16. Narrow right-shift inputs are zero/sign
extended into reserved scratch RAX. Three-operand instructions allow destination
and either source to alias. Scratch RAX/RCX are excluded from allocation.
Constant 32/64-bit ROTL/ROTR use RORX (left rotation negates the masked count).
Constant shifts and variable/narrow rotates retain their existing lowering.

BMI1 CTZ uses TZCNT. For i32/i64, hardware returns the width for zero. For
i8/i16, a sentinel bit above the narrow value ensures the narrow width is
returned for zero despite undefined high bits in the input. The i128 paths
are unchanged. IR has no user-visible implicit flags, and these selections
never replace a compare or consume arithmetic flags.

ANDN and isolate/reset-bit fusion are deferred: allocation currently follows
the unfused IR use graph, so looking through a producer can read an inner value
whose register has already been reused. Safe fusion needs selector/liveness
integration; this track does not change the generic optimizer. Baseline
selection remains unchanged when features are absent.

## Validation

`cargo test -p keel-asm --test x86_bmi --test x86_bmi_codegen --test x86_bmi_runtime`

- 2,280 encoding cases compare against both Clang's x86 Linux integrated
  assembler and GNU `as`, including high registers, aliases, inferred/explicit
  memory widths, SIB/index-only, FS, RIP-relative, and displacement boundaries.
  GNU cross assembler defaults to `x86_64-elf-as` off Linux; override with
  `KEEL_GNU_AS`. Both reference tools are required, not silently skipped.
- Known bytes and reserved fields, independent feature rejection, invalid
  operands, and symbolic RIP-relative relocation differential checks.
- Zero `Item::Text` and whole-object byte equality for structured/text assembly;
  codegen compares all three selection tiers and both allocation tiers,
  plus existing integer fixtures and disabled-feature baseline output.
- Linux/x86-64 runtime tests use actual `is_x86_feature_detected!` guards.
  Assembler tests independently guard BMI1 and BMI2, covering all families,
  32/64 bits, zero, boundaries, high GPRs, memory, and source/destination aliases.
  MULX tests check both halves and implicit RDX. Flag-preserving BMI2 operations
  are tested with both CF/ZF input states. Codegen tests cover all scalar
  widths, dirty upper bits, modulo shift counts, live sources, same-value inputs,
  constant rotates, and register pressure/spills across all pipeline tiers.
  Harnesses and assembler runtime objects are checked even on non-x86 hosts.

Execution of the guarded Linux tests is intentionally left to the parent on
its BMI-capable x86 Linux host. Local cross-target checks cannot establish
hardware execution semantics.
