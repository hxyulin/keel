# keel-asm-ir

The structured assembly that codegen hands to the assemblers, and the shared
x86 instruction tables that the assembler and the disassembler both read. It
also defines `Target`, the one list of compilation targets. It parses no
AArch64 source and writes no objects.

## Module map

| Module | Owns |
| --- | --- |
| `lib.rs` | AArch64 `Instruction`, `Item`, `Program` and `X86Source` |
| `target.rs` | `Target`: triples, aliases and `Target::host()` |
| `directive.rs` | Directives, CFI and symbol expressions |
| `symbolic.rs` | Symbol-bearing instructions resolved after layout |
| `generated.rs` | AArch64 `Opcode` and `FieldId` identities |
| `x86.rs` | x86 `Instruction`, `Operand`, `Memory` and `Address` |
| `x86/att.rs`, `format.rs`, `expression.rs` | AT&T parsing, Intel printing and GNU integer expressions |
| `x86/features.rs` | CPU levels and feature policy |
| `x86/evex.rs` | AVX-512 forms and EVEX decorators |
| `x86/core.rs`, `v2.rs`, `v3.rs`, `x87_extra.rs`, `fixed.rs`, `extensions.rs` | Hand-written opcode families and their stable identities |
| `x86/isa.rs` | Access to the generated form catalogue |
| `x86/isa/*.rs` | Generated forms, opcodes, registers, features and `op` constants |

## Generated files

`src/generated.rs` comes from `crates/keel-asm/tools/generate.py`. Every file
under `src/x86/isa/` comes from `tools/x86-isa/generate.py`; see the
[shared x86 tables](../../tools/x86-isa/README.md) for its inputs. Run
`python3 tools/test.py generators` after changing either generator or its
inputs.
