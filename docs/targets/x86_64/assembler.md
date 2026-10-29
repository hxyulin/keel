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
of AVX even where their instructions use VEX prefixes. See the [AVX subset](codegen.md)
and [BMI coverage](bmi.md) for instruction forms, lowering choices and limits.

The assembler also accepts ADX (`adx`), SHA (`sha`), XMM VAES
(`avx,aes`), YMM VAES (`avx,vaes`), XMM VPCLMULQDQ (`avx,pclmulqdq`)
and YMM VPCLMULQDQ (`avx,vpclmulqdq`). These optional forms have generated
public `ExtensionOpcode` adapters. They do not add compiler intrinsics or
instruction-selection rules. See the [extension coverage and feature rules](../../../tools/x86-isa/README.md#optional-legacyvex-extensions).

## AMX assembly

The assembler and decoder expose the imported AMX-TILE, AMX-INT8, AMX-BF16
and AMX-FP16 forms. The feature names are `amx_tile`, `amx_int8`, `amx_bf16`
and `amx_fp16`. Arithmetic features require `amx_tile`; tile permission requires
XSAVE and OSXSAVE. Positive feature edits add these prerequisites. No v1/v2/v3
CPU profile enables AMX implicitly.

- Tile configuration and state: `ldtilecfg`, `sttilecfg`, `tilerelease`, `tilezero`.
- Tile memory: `tileloadd`, `tileloaddt1`, `tilestored`.
- Integer dot products: `tdpbssd`, `tdpbsud`, `tdpbusd`, `tdpbuud`.
- Floating dot products: `tdpbf16ps`, `tdpfp16ps`.

Intel and AT&T syntax accept `tmm0` through `tmm7`. Typed programs use
`RegisterClass::Tile`, whose 8192-bit width describes the maximum register
payload, not its configured shape. Tile configuration remains runtime state.
The shared VEX encoder emits the required SIB byte for tile memory operations,
including a base-only address with no stride register. Both 32-bit and 64-bit
addresses are supported. Tile loads/stores cannot use RIP-relative addressing;
tile configuration loads/stores can. Configuration memory is a 64-byte image;
data movement spans the rows and columns in the active tile configuration.

```asm
ldtilecfg [rdi]
tileloadd tmm1, [rsi+rdx]
tileloadd tmm2, [rcx+r8]
tdpbssd tmm0, tmm1, tmm2
tilestored [r9+r10], tmm0
tilerelease
```

Assembly permission does not enable tile XCR0/XFD state or arrange OS tile-state
permission. The caller owns tile configuration, shape compatibility, memory
bounds, and architectural execution preconditions. This interface adds no tile
SSA values, compiler tile allocation, C tile intrinsics, or native runtime
qualification. Encoding tests compare literal bytes and independent Clang/GNU
objects in both dialects, then decode and reassemble; they never execute AMX.

## APX REX2 assembly

The assembler and decoder support the REX2 part of Intel APX. Enable it with
`--target-features +apx_f` (`apx` and `apxf` are aliases). This adds XSAVE and
OSXSAVE prerequisites; it does not enable the required APX state in XCR0 or
qualify execution on the host. No v1/v2/v3 profile enables APX implicitly.

General registers `r16` through `r31` and their `b`, `w` and `d` views work in
eligible legacy opcode maps 0 and 1. The same registers can be memory bases
and indices, including 32-bit addressing. REX2 does not extend the number of
XMM registers in legacy SSE instructions. Such instructions can use extended
GPRs for their memory addresses. High-byte AH/CH/DH/BH operands cannot be
combined with REX2.

```asm
add r16, r17
mov qword ptr [r20+r28*4+8], r31
pushp r17
popp r30
jmpabs 0x123456789abcdef0
```

The imported source also exposes the APX `pushp`, `popp` and `jmpabs` forms.
`jmpabs` carries an absolute 64-bit address; symbolic operands use the existing
64-bit absolute relocation. `pushp` and `popp` select the APX push/pop hint
with REX2.W, while ordinary `push` and `pop` retain their existing semantics.
Intel and AT&T source and typed instruction operands share the same feature
and form checks. REX2 is emitted only where the source permits it, and is
required for the source's REX2-only records.

The pinned source has no APX EVEX new-destination/no-flags forms. Three-operand
APX arithmetic, NF/NDD decorators, EGPR use in VEX forms, and legacy maps 2/3
remain rejected. This adds no compiler EGPR allocation, compiler intrinsics,
or APX calling convention. Compiler fixed-register operands still require
the existing register bank. Tests compare literal bytes, Clang and GNU objects,
and decode/reassembly with feature-disabled and malformed-prefix controls;
they never execute APX.

The prefix and register rules follow the
[Intel APX architecture specification](https://cdrdv2-public.intel.com/861610/355828-007-intel-apx-spec.pdf).

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
fragments retain the source parser. Compiler inline assembly uses the checked
[GPR/SIMD operand contract](c-inline-asm.md). The assembler borrows structured items across layout passes instead
of cloning the complete program.

Instruction forms, opcode bytes, operand roles and ISA requirements come from
the [shared generated tables](../../../tools/x86-isa/README.md), which also drive the
x86 disassembler. Stable opcode enums remain the assembly API. Common emission
code handles legacy, REX, REX2, VEX and EVEX prefixes, ModRM, SIB, immediate
and displacement sizing. The `object` crate serializes ELF64 section headers, string tables,
ordered symbol tables and RELA records; Keel owns assembly layout, symbol
expressions, relocation selection and instruction encoding.

## Backend coverage

The assembler covers every mnemonic family emitted by `x86_64.rs`: integer
arithmetic/logic, comparisons, condition codes, sign/zero extension, shifts and
rotates (including double shifts), bit scans/counts, multiply/divide, stack and
control flow, scalar SSE arithmetic/conversions/rounding, XMM moves and logic,
locked atomics, fences and traps. Packed SSE/SSE2 vector arithmetic, square root,
integer add/subtract and i16 multiply support the [SIMD IR lowering](../../compiler/simd-ir.md). Addressing includes high registers, signed
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

Instruction support follows the generated form catalogue and the documented
operand subsets, including the AMX and APX boundaries above and the
[EVEX subset](../../../tools/x86-isa/README.md#avx-512fvl-and-evex).
Intel and AT&T dialects share the encoder. GNU preprocessing, macros, labels
and expressions follow the [GNU language contract](gnu-assembly-language.md).
Unsupported directives and operands return errors; successful assembly does not
establish a compiler operand ABI or runtime execution qualification.

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

Use the [suite measurement runners](../../../suite/README.md#measurements) for
controlled compiler comparisons.

To profile assembler stages (parse, per-pass layout, branch checks, ELF
finalization) without instrumenting normal assembly:

```sh
cargo build --release -p keel-asm --example profile_x86
./target/release/examples/profile_x86 input.s 5
```
