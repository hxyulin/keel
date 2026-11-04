# keel-asm

Native assemblers: AArch64 Mach-O and AArch64 ELF from `lib.rs`, and x86-64 ELF
from `x86`. Both read GNU-style source or a structured `keel_asm_ir::Program`
and write object files without external tools. `ObjectAssembler::for_target`
picks the assembler for a `Target`; `x86::Request` sets the x86 syntax,
features and binary inputs. The contracts are in the
[AArch64](../../docs/targets/aarch64/assembler.md) and
[x86-64](../../docs/targets/x86_64/assembler.md) assembler references.

## Module map

| Module | Owns |
| --- | --- |
| `lib.rs` | Public AArch64 API, `Assembly`, `Error`, `ObjectAssembler` |
| `parser.rs` | AArch64 layout passes and their per-pass arena |
| `parser/scanning.rs`, `storage.rs`, `text.rs` | Line and comment scanning |
| `parser/preprocess.rs` | Bounded macro and conditional expansion |
| `parser/arithmetic.rs`, `memory.rs`, `atomic.rs`, `float.rs`, `control.rs`, `system.rs` | A64 instruction families |
| `parser/vector/`, `scalable/` | Advanced SIMD, and opt-in SVE and SME forms |
| `parser/data.rs`, `relocate.rs`, `atoms.rs` | Data sections, symbol expressions, Darwin relocations and linker atoms |
| `parser/debug.rs`, `compact.rs`, `dwarf_line.rs` | DWARF line programs, CFI and compact unwind |
| `parser/elf.rs` | GNU AArch64 syntax for the shared ELF path |
| `parser/program.rs` | Structured `Program` input |
| `macho.rs` | Mach-O object writer |
| `x86/mod.rs` | x86-64 ELF assembler, `Request`, layout loop |
| `x86/source.rs` | GNU statement scanning and numeric local labels |
| `x86/encode.rs`, `encode/tables.rs`, `encode/evex.rs` | Form matching and prefix, ModRM and EVEX emission |
| `x86/relax.rs`, `symbolic.rs` | Short-branch relaxation and symbolic instructions |
| `x86/sections.rs`, `metadata.rs`, `reloc.rs` | ELF sections, symbols and relocation kinds |
| `x86/dwarf.rs` | ELF line tables |
| `x86/arm.rs` | A64 instructions written through the shared ELF writer |
| `x86/inputs.rs`, `metrics.rs` | `.incbin` inputs and `asm-metrics` encoding reports |

## Generated files

| File | Generator | Input |
| --- | --- | --- |
| `src/generated.rs` | `crates/keel-asm/tools/generate.py` | `isa/a64.json` and the `isa/*.txt` selections |
| `src/x86/encode/mnemonics.rs` | `tools/x86-isa/generate.py` | the shared x86 tables |

Regenerate with `python3 crates/keel-asm/tools/generate.py` or
`python3 tools/x86-isa/generate.py`. `python3 tools/test.py generators` checks
that both are current. Every other file is hand-written.
