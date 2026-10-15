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

## Boundary audit and design

The existing `keel-asm-ir::Instruction`, generated `Opcode`/`FieldId`, symbolic
instructions, and `Item::Branch` describe exact AArch64 encodings. They remain
unchanged. `x86::Opcode`, `Register`, `Memory`, `Address`, `Operand`, and
`Instruction` provide separate x86 identities through `Item::X86Instruction`.
Each assembler explicitly rejects the other architecture's instruction items.
`Program` remains the common transport and counts both instruction families.

The x86 SysV generator already emits Intel source, unlike AArch64's structured
sink. Its object API currently parses that source into typed instruction items;
ELF metadata directives remain explicit text. This is a transitional boundary:
there is no subprocess or temporary assembly file, but there is still formatting
and parsing overhead. A future direct x86 sink can remove it without changing
the encoder or ELF writer. Shared byte, integer, expression, section, file,
location, CFI and zero-fill directives also have native consumers.

The encoder implements REX, legacy prefixes, ModRM, SIB, immediate and displacement
sizing. The `object` crate serializes ELF64 section headers, string tables,
ordered symbol tables and RELA records; Keel owns assembly layout, symbol
expressions, relocation selection and instruction encoding.

## Backend coverage

The source audit includes all mnemonic families emitted by `x86_64.rs`: integer
arithmetic/logic, comparisons, condition codes, sign/zero extension, shifts and
rotates (including double shifts), bit scans/counts, multiply/divide, stack and
control flow, scalar SSE arithmetic/conversions/rounding, XMM moves and logic,
locked atomics, fences and traps. Addressing includes high registers, signed
displacements, scaled indices, RIP-relative symbols, FS/GS and absolute disp32.

ELF handling includes PROGBITS/NOBITS, writable/executable/allocated/TLS flags,
local/global/weak/hidden symbols, types/sizes, common definitions, section-relative
aliases and same-section symbol differences. Relocations include `64`, `PC32`,
`PLT32`, `GOTPCREL`, `32`, `32S`, `16`, `8`, `TPOFF32`, `GOTTPOFF` and `DTPOFF64`.
PC-relative instruction addends account for trailing immediate bytes.

Data directives, strings, space, alignment, constant LEB128, `.set`, `.file`,
`.loc`, and the backend's CFI directives are handled. DWARF data emitted by the
compiler retains its relocations; the assembler generates DWARF 4 line tables
and `.eh_frame` CIE/FDE records. CFI register numbers in shared directives use
x86 DWARF numbering. Source CFI accepts x86 register names too.

## Limits and performance boundaries

All direct branches use rel32, including nearby conditional branches. There is
no short-branch relaxation, so code can be larger than Clang/GAS output. This
policy gives stable layout, handles jump-table differences without iteration,
and diagnoses resolved displacements outside signed 32 bits. Calls outside that
range require linker support; Keel does not synthesize large-code-model thunks.

This is a backend-oriented assembler, not a replacement for all GNU assembler
syntax. AVX/AVX-512, x87, arbitrary vector instructions, AT&T syntax, macros,
preprocessing, include files, numeric directional labels, section groups/COMDAT,
mergeable section entry sizes, arbitrary expression arithmetic, symbolic LEB128,
exception personality/LSDA directives and general-dynamic TLS are not implemented.
Unsupported directives and operands return errors; they are not silently dropped.

Layout and fixups require memory proportional to the input/object. Symbol lookup
uses ordered maps; local temporary symbols are retained in the ELF symbol table.
Executable alignment uses single-byte NOPs unless a zero fill is explicit;
data alignment uses zeros. Tuned multi-byte NOP sequences are not generated. No
compile-time speedup is claimed without a separate controlled measurement.
Native suite configurations are opt-in (`keel-native`, `keel-O-native`,
`keel-g-native`, `keel-O-select-native`); existing suite baselines are unchanged.

## Validation

`cargo test -p keel-asm --test x86` contains byte-level Clang/GNU as comparisons, local
branch/difference checks, ELF symbol/section/relocation assertions, mixed-ISA
rejection and metadata checks. Existing AArch64 assembler tests exercise the
unchanged target path. Native NUC validation uses an isolated checkout/build
location and reads the parent's prepared suite inputs without modifying them.

The [NUC validation and timing report](../suite/reports/native-x86-assembler-linux-20261015.json)
records successful execution of 53 codegen fixtures in eight configurations (424
runs, plus focused spill regressions), 20 C fixture assembly checks in four modes,
and Lua/QuickJS/SQLite correctness with baseline, optimized, debug and optimized
selection configurations. Lua runs its portable suite, QuickJS all nine listed
suite tests, and SQLite output matches a Clang build. All 192 assembler tests on
MacHost pass, including existing AArch64 tests. A test with an empty tool PATH
confirms preprocessed Linux C can emit an object without external executables.

Clang and GNU as match the instruction byte matrix and normalized ELF relocation
semantics. GNU readelf accepts the symbols, relocations, CFI and debug tables;
GDB stops at the requested C source line, reads `answer = 42` and unwinds to
`main`. libgcc unwinds through a Keel-generated frame (eight frames observed).

Reproduce the Linux execution and metadata checks in an isolated directory:

```sh
KEEL_TEST_ASSEMBLER=native cargo test -p keel-codegen --test run
python3 suite/validate_native_x86.py --compiler target/release/keel \
  --prepared /path/to/prepared/target/suite --work-dir /path/to/isolated/validation
python3 suite/validate_native_x86_metadata.py --compiler target/release/keel \
  --work-dir /path/to/isolated/metadata
```

Three alternating paired 44-file batches after warmup on the NUC measured median
native object emission at **2.61 s versus 5.69 s** for the same Keel followed by
Clang assembly; with `-O`, **3.15 s versus 5.83 s**. These are serial compilation
and assembly times, excluding linking. The measured binary/input hashes and all
samples are in the report. `suite/measure_native_x86.py` reproduces this comparison.
This establishes an object-generation improvement on these inputs and this host;
it does not measure application runtime or remove the text-to-IR conversion cost.
