# Native x86-64 Linux objects

`keel compile --target x86_64-unknown-linux-gnu --emit-object input.keel -o output.o`
and `keel cc --emit-object input.i -o output.o` assemble in process. Linking still
uses the system linker. Raw C uses Keel's native preprocessor and target-owned compiler headers, plus
the selected target's system headers. Preprocessed `.i` input isolates compilation
and assembly from preprocessing.

Standalone Intel syntax:

```sh
keel asm --target x86_64-unknown-linux-gnu input.s -o output.o
keel asm --target x86_64-unknown-linux-gnu a.s b.s --out-dir objects
```

Use destination-first operands, unprefixed registers, `[base + index*scale +
displacement]`, and explicit `byte/word/dword/qword/xmmword ptr` when a memory
width cannot be inferred. `#` and `//` introduce comments; semicolons separate
statements. Quote symbols that collide with register names or contain punctuation.
AArch64 remains the standalone command's default for compatibility.

Optional ISA features are explicit: pass `--target-features avx,bmi1,bmi2` to
`compile`, `cc`, or `asm`. The default stays baseline x86-64. API callers use
`x86::Features` with the `_with_features` assembler APIs and wrap their codegen
selector in `keel_codegen::x86_features::FeatureSelector`. BMI gates are independent
of AVX even where their instructions use VEX prefixes. See the [AVX subset](x86-avx-subset.md)
and [BMI coverage](x86-bmi.md) for instruction forms, lowering choices and limits.

## Design

The existing `keel-asm-ir::Instruction`, generated `Opcode`/`FieldId`, symbolic
instructions, and `Item::Branch` describe exact AArch64 encodings. They remain
unchanged. `x86::Opcode`, `Register`, `Memory`, `Address`, `Operand`, and
`Instruction` provide separate x86 identities through `Item::X86Instruction`.
Each assembler explicitly rejects the other architecture's instruction items.
`Program` remains the common transport and counts both instruction families.

The x86 SysV generator constructs typed instructions, numeric registers and
memory operands directly. Both `compile_program[_with_pipeline]` and the CLI's
`--emit-object` path pass this program to the assembler without formatting or
parsing assembly source. Text output serializes the same typed instructions as
Intel syntax. ELF sections, symbol types/sizes, data, source locations, CFI and
DWARF (including TLS offsets) also use typed directives. Compiler-produced x86
programs contain zero text bytes; standalone assembly and explicit API text
fragments retain the source parser. The x86 compiler still rejects inline
assembly. The assembler borrows structured items across layout passes instead
of cloning the complete program.

The encoder implements REX, legacy and VEX prefixes, ModRM, SIB, immediate and displacement
sizing. The `object` crate serializes ELF64 section headers, string tables,
ordered symbol tables and RELA records; Keel owns assembly layout, symbol
expressions, relocation selection and instruction encoding.

## Backend coverage

The assembler covers every mnemonic family emitted by `x86_64.rs`: integer
arithmetic/logic, comparisons, condition codes, sign/zero extension, shifts and
rotates (including double shifts), bit scans/counts, multiply/divide, stack and
control flow, scalar SSE arithmetic/conversions/rounding, XMM moves and logic,
locked atomics, fences and traps. Packed SSE/SSE2 vector arithmetic, square root,
integer add/subtract and i16 multiply support the [SIMD IR lowering](simd-ir.md). Addressing includes high registers, signed
displacements, scaled indices, RIP-relative symbols, FS/GS and absolute disp32.

ELF handling includes PROGBITS/NOBITS, writable/executable/allocated/TLS flags,
mergeable/string entry sizes, COMDAT section groups (including relocation
members), local/global/weak/hidden symbols, types/sizes, common definitions, section-relative
aliases and same-section symbol differences. Relocations include `64`, `PC32`,
`PLT32`, `GOTPCREL`, `32`, `32S`, `16`, `8`, `TPOFF32`, `GOTTPOFF` and `DTPOFF64`.
PC-relative instruction addends account for trailing immediate bytes.

Data directives, strings, space, alignment, constant LEB128, `.set`, `.file`,
`.loc`, and the backend's CFI directives are handled. DWARF data emitted by the
compiler retains its relocations; the assembler generates DWARF 4 line tables
and `.eh_frame` CIE/FDE records. CFI register numbers in shared directives use
x86 DWARF numbering. Source CFI accepts x86 register names too.

ELF section declarations accept `a`, `w`, `x`, `T`, `M`, `S`, and `G` flags.
For example, `.section .rodata.cst8,"aM",@progbits,8` sets `sh_entsize` to 8;
`.section .rodata.str1.1,"aMS",@progbits,1` describes byte strings.
`.section .text.foo,"axG",@progbits,foo,comdat` and
`.section .rodata.foo,"aMG",@progbits,8,foo,comdat` share a COMDAT group.
`%progbits`/`%nobits` spellings and quoted names/signatures are accepted too.
Repeated declarations must agree on attributes; distinct groups may use the same
section name. `M` requires a positive entry size, `S` requires `M`, and `G`
requires a signature and explicit `comdat` selection. Merge/string NOBITS
sections, other group selection kinds, and additional section arguments are
rejected. Typed callers can use `Directive::ElfSectionMetadata`; the existing
`ElfSection` API is unchanged. A group-only signature may receive a synthetic
`.group` definition; a signature referenced by an ordinary relocation stays
undefined unless the input defines it. Local targets in mergeable sections retain
symbol identities so link-time merging applies PC-relative addends correctly.

## Limits and performance boundaries

Resolved same-section local JMP/Jcc branches use rel8 when the final displacement
fits -128 through +127. Layout starts optimistically short and only widens;
each widening pass rechecks all short branches and recomputes alignments,
max-skip decisions, symbols, aliases, sizes/differences, CFI and line addresses.
This terminates even when alignment changes non-monotonically. It can retain a
near branch that a different layout strategy could shorten; it does not promise
a globally minimal layout. Calls and explicit PLT targets retain rel32.
Binding-sensitive, unresolved/external and cross-section branches retain the
necessary linker relocations.
Resolved displacements outside signed 32 bits are diagnosed. Keel does not
synthesize large-code-model thunks.

This is a backend-oriented assembler, not a replacement for all GNU assembler
syntax. AVX forms beyond the documented XMM subset, AVX2/AVX-512, x87,
arbitrary vector instructions, AT&T syntax, macros,
preprocessing, include files, numeric directional labels, non-COMDAT section groups,
arbitrary expression arithmetic, symbolic LEB128,
exception personality/LSDA directives and general-dynamic TLS are not implemented.
Unsupported directives and operands return errors; they are not silently dropped.

Layout and fixups require memory proportional to the input/object. Symbol lookup
uses a hash map; ELF emission sorts names once to keep output deterministic.
Unneeded local `.L` symbols are omitted from ELF; ordinary
relocations use section symbols plus the target's section offset. Explicit
global/weak/hidden symbols, undefined symbols, TLS symbols, GOT/TLS references
and absolute symbols requiring PC-relative linker relocations retain their
identity. Referenced merge-section targets and COMDAT signatures retain identity
as well. Absolute constants used only in resolved data are omitted too.
Executable alignment uses single-byte NOPs unless a fill byte is explicit;
data alignment uses zeros. Explicit fills preserve their low byte. Tuned
multi-byte NOP sequences are not generated. Symbol-free encodings are cached in a
contiguous buffer and reused across layout passes; symbolic instructions and
layout-dependent absolute operands are re-encoded each pass.
Native suite configurations are opt-in (`keel-native`, `keel-O-native`,
`keel-g-native`, `keel-O-select-native`); existing suite baselines are unchanged.

## Compiler API

The compiler emits typed x86 instructions and directives directly; only text
output formats Intel syntax. The native object route never builds register
names, mnemonic strings or an assembly file:

```rust
let program = keel_codegen::compile_program(
    &module,
    keel_codegen::Target::X86_64LinuxGnu,
)?;
let object = keel_asm::x86::assemble_program_object(&program)?;
```

`compile_program_with_pipeline` adds selection/allocation controls. The CLI's
`compile` and `cc --emit-object` use this route automatically; standalone
assembly still parses source. Text output may use different but equivalent
spellings (quoted symbols, numeric CFI register IDs, decimal immediates). The
formatter preserves widths, high-byte registers, lock prefixes, segments, scaled
indices and relocation modifiers, and writes symbolic non-branch immediates with
`offset` to distinguish them from memory operands. Structured emission and
reassembled text produce byte-identical objects.

## Validation

```sh
cargo test -p keel-asm --test x86
cargo test -p keel-asm --test x86_sections
KEEL_TEST_ASSEMBLER=native cargo test -p keel-codegen --test run
```

- `x86` compares instruction bytes with Clang and GNU as, and checks local
  branches and symbol differences, ELF symbols/sections/relocations, mixed-ISA
  rejection and metadata. It includes the rel8 boundary, cascading-widening and
  re-encoding tests (`rel8_boundaries_and_binding_sensitive_targets`,
  `cascading_widening_rechecks_alignment_and_all_offsets`,
  `layout_dependent_absolute_operands_are_reencoded`) and temporary-symbol
  relocation tests, including the `.set .Labs,42; .long .Labs-.` case that
  needs a retained symbol and a PC32 relocation.
- `x86_sections` checks merge entry sizes, COMDAT membership and signatures,
  shared and distinct section names, typed/text equivalence, deterministic output
  and malformed declarations against Clang and (on Linux) GNU as. On Linux it
  also links duplicate COMDAT definitions from Keel and reference objects and
  checks that code, data and relocations coalesce together.
- Backend fixtures require zero source-text bytes and byte-identical objects from
  structured emission and reassembled text.

On x86-64 Linux, two scripts run the codegen fixtures, the C fixtures and the
Lua, QuickJS and SQLite application builds, and check ELF/DWARF metadata with
GNU readelf, GDB (source breakpoints, locals, backtraces) and libgcc unwinding:

```sh
python3 suite/validate_native_x86.py --compiler target/release/keel \
  --prepared /path/to/prepared/target/suite --work-dir /path/to/validation
python3 suite/validate_native_x86_metadata.py --compiler target/release/keel \
  --work-dir /path/to/metadata
```

Native suite configurations are opt-in: `keel-native`, `keel-O-native`,
`keel-g-native` and `keel-O-select-native`.

## Performance

Measured on an Intel Core i7-1360P running Linux, over the same 44
Keel-generated assembly files used by the application suite. Timings include
process startup, reads and writes, and exclude linking.

| Measurement | Keel | Clang |
| --- | ---: | ---: |
| Standalone assembly, baseline | 1.139 s | 3.959 s |
| Standalone assembly, optimized | 0.967 s | 3.618 s |
| Preprocessed C to object, baseline | 1.996 s | 5.615 s (Keel text + Clang assembler) |
| Preprocessed C to object, optimized | 2.475 s | 5.830 s (Keel text + Clang assembler) |

Branch relaxation brings aggregate `.text` to within about 0.1% of Clang's
assembly of the same source. Runtime of the resulting executables is neutral
compared with Clang-assembled objects; no runtime speedup is claimed.

Measurement scripts (run on an otherwise idle host; where supported, `--cpu N`
pins the process and its children):

```sh
# Compile + assemble, Keel vs Keel text followed by Clang:
python3 suite/measure_native_x86.py --compiler target/release/keel \
  --prepared /path/to/prepared/target/suite --work-dir /path/to/benchmark
# Standalone assembler, two prebuilt compilers:
python3 suite/measure_x86_relaxation.py --before /path/to/keel-before \
  --after /path/to/keel-after --prepared /path/to/prepared/target/suite \
  --out /path/to/benchmark --cpu 2 \
  --before-revision BEFORE_COMMIT --after-revision AFTER_COMMIT
# Full preprocessed-C-to-object pipeline, including object identity checks:
python3 suite/measure_x86_structured.py --before /path/to/keel-before \
  --after /path/to/keel-after --prepared /path/to/prepared/target/suite \
  --out /path/to/benchmark --pairs 7 \
  --before-revision BEFORE_COMMIT --after-revision AFTER_COMMIT
```

To profile assembler stages (parse, per-pass layout, branch checks, ELF
finalization) without instrumenting normal assembly:

```sh
cargo build --release -p keel-asm --example profile_x86
./target/release/examples/profile_x86 input.s 5
```
