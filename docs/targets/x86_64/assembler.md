# Native x86-64 Linux objects

`keel compile --target x86_64-unknown-linux-gnu --emit-object input.keel -o output.o`
and `keel cc --emit-object input.i -o output.o` assemble in process. Link the
object with the [native ELF linker](../../linking/elf.md) or a system linker.
Raw C uses Keel's native preprocessor and target-owned compiler headers, plus
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
AArch64 remains the standalone command's default target. AT&T input is
described [below](#att-syntax).

Optional ISA features are explicit: pass `--target-features avx,bmi1,bmi2` to
`compile`, `cc`, or `asm`. The default stays baseline x86-64. API callers use
`x86::Features` with the `_with_features` assembler APIs and wrap their codegen
selector in `keel_codegen::x86_features::FeatureSelector`. BMI gates are independent
of AVX even where their instructions use VEX prefixes. See [BMI forms](#bmi-forms)
and the [code generation reference](codegen.md) for lowering choices.

The assembler also accepts ADX (`adx`), SHA (`sha`), XMM VAES
(`avx,aes`), YMM VAES (`avx,vaes`), XMM VPCLMULQDQ (`avx,pclmulqdq`)
and YMM VPCLMULQDQ (`avx,vpclmulqdq`). These optional forms have generated
public `ExtensionOpcode` adapters. They do not add compiler intrinsics or
instruction-selection rules. See the [extension coverage and feature rules](../../../tools/x86-isa/README.md#optional-legacyvex-extensions).

Compiler-owned raw fragments carry `keel_asm_ir::X86Source` metadata with
permissions and an owner for diagnostics. This metadata stays outside source
text. Instructions must satisfy both these local permissions and the assembler
API feature set. The assembler checks expanded instruction forms before encoding-cache
lookup and before retaining symbolic instructions for later layout. Ordinary
`Item::Text` uses the assembler's supplied feature set. `SourceValidator` shares
the same parser and preprocessing state across scoped fragments, including
macros, syntax, aliases and numeric local labels. A macro or conditional block
must close within its fragment; named forward symbols remain available for
final object assembly.

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
bounds, and architectural execution preconditions. The compiler has no tile
SSA values, tile allocation or C tile intrinsics.

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

`jmpabs` carries an absolute 64-bit address; symbolic operands use the existing
64-bit absolute relocation. `pushp` and `popp` select the APX push/pop hint
with REX2.W, while ordinary `push` and `pop` retain their existing semantics.
Intel and AT&T source and typed instruction operands share the same feature
and form checks. REX2 is emitted only where the source permits it, and is
required for the source's REX2-only records.

The pinned source has no APX EVEX new-destination/no-flags forms. Three-operand
APX arithmetic, NF/NDD decorators, EGPR use in VEX forms, and legacy maps 2/3
remain rejected. The compiler has no EGPR allocation, APX intrinsics or APX
calling convention.

The prefix and register rules follow the
[Intel APX architecture specification](https://cdrdv2-public.intel.com/861610/355828-007-intel-apx-spec.pdf).

## AT&T syntax

The assembler accepts Intel and AT&T source. The APIs and CLI default to
Intel; the assembler does not guess a dialect from instructions. For compiler
output that uses AT&T without a directive:

```sh
keel asm --target x86_64-unknown-linux-gnu --x86-syntax att input.s -o output.o
```

`--x86-syntax intel` selects the default explicitly. The option applies to
each batch input and does not change code generation or typed `Program`
instructions. `.att_syntax [prefix|noprefix]` and `.intel_syntax
[prefix|noprefix]` change the parser state, including within a line or between
`Program` text fragments. Without an argument, AT&T uses register prefixes and
Intel omits them.

The library entry point is
`keel_asm::x86::assemble_object_with_syntax_and_features(source, Syntax::Att, features)`.
`keel_asm_ir::x86::Instruction::parse_att` parses one instruction, and
`SourceParser::new(Syntax::Att).parse(source)` parses a fragment and keeps the
syntax state for later fragments. `parse_intel` and `parse_program` keep their
Intel defaults.

AT&T operands use source-before-destination order, `%register`, `$immediate`,
`displacement(base,index,scale)`, `%fs:`/`%gs:`, RIP-relative addresses and
ELF relocation modifiers. Indirect `call` and `jmp` require `*`. Scalar
suffixes select widths, and extension forms such as `movsbl`, `movzwq` and
`movslq` name both widths. Omitted scalar shift counts mean one, and
two-operand SHLD and SHRD use CL. Base and index registers may be 32-bit, which
adds the 0x67 address-size prefix. Feature checks apply in both dialects.

## Signed symbolic immediates

AT&T `movq $symbol+addend,%r64` selects `CoreOpcode::MovImm`, whose form uses
the ModRM immediate and emits `R_X86_64_32S`. Intel formatting writes it as
`mov.imm r64, offset symbol`. `movabsq` keeps the full-width `R_X86_64_64`
relocation, and `movl` keeps unsigned `R_X86_64_32`. A signed symbolic form
does not widen when the resolved value is out of range; use the full-width
spelling instead. An absolute name defined earlier can reduce to a numeric
operand during GNU preprocessing and select the ordinary numeric MOV, so an
absolute `0x80000000` selects the full-width encoding, as in GNU as.

Sign-extended fields accept an unsigned 64-bit spelling of a negative machine
word, such as `0xffffffff80000000`, only where the signed field can encode it.
Absolute ELF64 aliases keep negative values in the symbol table. Resolved
symbol differences keep signed range checks. An external symbol minus a local
symbol in the current section can use a PC-relative 32-bit relocation.
Differences that ELF cannot represent are errors.
`crates/keel-asm/tests/x86_att/x86_signed_immediates.rs` compares bytes and
relocations with GNU and Clang objects and links all three producers with GNU
ld at the signed-range boundaries.

## BMI forms

BMI1 (CPUID.7.0:EBX[3]) and BMI2 (EBX[8]) are independent features. These VEX
GPR instructions do not require AVX or OS AVX state. TZCNT is gated even though
older processors decode it as BSF, because the zero result differs.

All operands use matching 32- or 64-bit GPRs, and the destination is a
register. One source may be memory; its width follows the destination when
omitted. 8- and 16-bit forms are rejected, including the architecturally valid
16-bit TZCNT. There is no XOP or immediate-control BEXTR.

| Instructions | Intel operand order | ModRM.reg | VEX.vvvv | ModRM.r/m |
| --- | --- | --- | --- | --- |
| ANDN | dst, inverted-src, src | dst | inverted-src | src |
| BEXTR | dst, src, control | dst | control | src |
| BLSI, BLSMSK, BLSR | dst, src | /3, /2, /1 | dst | src |
| SHLX, SHRX, SARX | dst, src, count | dst | count | src |
| RORX | dst, src, imm8 | dst | 1111 (reserved) | src |
| BZHI | dst, src, index | dst | index | src |
| PEXT, PDEP | dst, src, mask | dst | src | mask |
| MULX | high-dst, low-dst, src | high-dst | low-dst | src |
| TZCNT | dst, src | dst | none (legacy F3 0F BC) | src |

RORX accepts a literal 0..255 only, with no relocation or negative value; the
hardware masks the count. MULX reads EDX or RDX implicitly and preserves flags.
The compiler does not select MULX, PEXT or PDEP.

## x87 and compiler-output forms

The x87 register class represents `st`/`st(0)` through `st(7)` at 80 bits. It
cannot be a general register, address base or index. The compiler's raw x87
intrinsics and Clang-generated application assembly use these forms:

- `fld` and `fstp`: one stack register or 32/64/80-bit floating memory.
- `fxch`: one stack register, or no operands for `st(1)`.
- `fucomi` and `fucomip` (alias `fucompi`): `st(0), st(i)`, one `st(i)`, or no
  operands for `st(0), st(1)`.
- `fadd` and `fmul`: a register pair containing `st(0)`, one `st(i)` with
  destination `st(0)`, or 32/64-bit floating memory.
- `fmulp`, `fsubp` and `fsubrp`: `st(i), st(0)`, one destination `st(i)`, or no
  operands for `st(1), st(0)`.
- `fcmovnb`: destination `st(0)` and source `st(i)`.
- `fild` and `fistp`: 16/32/64-bit integer memory.
- `fldcw` and `fnstcw`: 16-bit control-word memory. `fldz` has no operands.

The raw x87 intrinsics add further fixed forms; see the
[F80 reference](f80/README.md). AT&T reverses explicit operand order, uses
`s/l/t` floating-memory and `s/l/ll` integer-memory suffixes, and infers the
16-bit width of control-word operands. AT&T `fsubp` maps to Intel `fsubrp` and
AT&T `fsubrp` to Intel `fsubp`, as in GAS and LLVM. Floating and integer memory
widths are mandatory; unsuffixed AT&T memory forms are rejected. x87 data
widths never add REX.W or a 16-bit operand-size prefix.

`cmpeqsd`, `cmplesd`, `cmpneqsd` and `cmpordsd` select `cmpsd` predicates 0, 2,
4 and 7. They take an XMM destination and an XMM or 64-bit memory source.

## Design

`keel-asm-ir::Instruction`, generated `Opcode`/`FieldId`, symbolic
instructions and `Item::Branch` describe AArch64 encodings. `x86::Opcode`, `Register`, `Memory`, `Address`, `Operand`, and
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
fragments use the source parser. Compiler inline assembly uses the checked
[GPR/SIMD operand contract](c-inline-asm.md).

Instruction forms, opcode bytes, operand roles and ISA requirements come from
the [shared generated tables](../../../tools/x86-isa/README.md), which also drive the
x86 disassembler. Stable opcode enums are the assembly API. Common emission
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

`.file 0` selects a DWARF 5 `.debug_line` table with a `.debug_line_str`
section; otherwise the table is DWARF 4. The directive rules match the
[AArch64 assembler](../aarch64/assembler.md#dwarf-5). ELF `line_strp` offsets
carry `R_X86_64_32` relocations against `.debug_line_str`. `md5` and `source`
operands require a `.file 0` root, and a generated table or string section that
conflicts with explicit `.debug_line` or `.debug_line_str` contents is an
error.

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
rejected. Typed callers can use `Directive::ElfSectionMetadata`. A group-only signature may receive a synthetic
`.group` definition; a signature referenced by an ordinary relocation stays
undefined unless the input defines it. Local targets in mergeable sections retain
symbol identities so link-time merging applies PC-relative addends correctly.

## Limits

Resolved same-section local JMP/Jcc branches use rel8 when the final displacement
fits -128 through +127. Layout starts optimistically short and only widens;
each widening pass rechecks all short branches and recomputes alignments,
max-skip decisions, symbols, aliases, sizes/differences, CFI and line addresses.
This terminates even when alignment changes non-monotonically. It does not
promise a globally minimal layout. Calls and explicit PLT targets retain rel32.
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

Unneeded local `.L` symbols are omitted from ELF; ordinary
relocations use section symbols plus the target's section offset. Explicit
global/weak/hidden symbols, undefined symbols, TLS symbols, GOT/TLS references
and absolute symbols requiring PC-relative linker relocations retain their
identity. Referenced merge-section targets and COMDAT signatures retain identity
as well. Absolute constants used only in resolved data are omitted too.
Executable alignment uses single-byte NOPs unless a fill byte is explicit;
data alignment uses zeros. Explicit fills preserve their low byte. Tuned
multi-byte NOP sequences are not generated.
Native suite configurations are opt-in: `keel-native`, `keel-O-native`,
`keel-g-native` and `keel-O-select-native`.

## Compiler API

The native object route never builds register names, mnemonic strings or an
assembly file:

```rust
let program = keel_codegen::compile_program(
    &module,
    keel_codegen::Target::X86_64LinuxGnu,
)?;
let object = keel_asm::x86::assemble_program_object(&program)?;
```

`compile_program_with_pipeline` adds selection and allocation controls. Text output may use different but equivalent
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

`x86` compares instruction bytes with Clang and GNU as and checks branch
relaxation, symbol differences, ELF symbols, sections and relocations.
`x86_sections` compares merge, COMDAT and section declarations with Clang and,
on Linux, GNU as. Backend fixtures require structured emission and reassembled
text to produce identical objects. GNU as comparisons use `x86_64-elf-as` off
Linux; set `KEEL_GNU_AS` to override it.

To profile assembler stages (parse, per-pass layout, branch checks, ELF
finalization):

```sh
cargo build --release -p keel-asm --example profile_x86
./target/release/examples/profile_x86 input.s 5
```
