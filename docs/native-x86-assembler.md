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
syntax. AVX/AVX-512, x87, arbitrary vector instructions, AT&T syntax, macros,
preprocessing, include files, numeric directional labels, section groups/COMDAT,
mergeable section entry sizes, arbitrary expression arithmetic, symbolic LEB128,
exception personality/LSDA directives and general-dynamic TLS are not implemented.
Unsupported directives and operands return errors; they are not silently dropped.

Layout and fixups require memory proportional to the input/object. Symbol lookup
uses a hash map; ELF emission sorts names to preserve deterministic output.
Unneeded local `.L` symbols are omitted from ELF; ordinary
relocations use section symbols plus the target's section offset. Explicit
global/weak/hidden symbols, undefined symbols, TLS symbols, GOT/TLS references
and absolute symbols requiring PC-relative linker relocations retain their
identity. Absolute constants used only in resolved data are omitted too.
Executable alignment uses single-byte NOPs unless a fill byte is explicit;
data alignment uses zeros. Explicit fills preserve their low byte. Tuned
multi-byte NOP sequences are not generated. Parsed instructions and raw encodings
are reused across layout passes;
layout-dependent absolute operands are reevaluated and encoded again. Layout
iteration adds assembly cost, measured separately from compilation below.
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

## Layout follow-up

`rel8_boundaries_and_binding_sensitive_targets`,
`cascading_widening_rechecks_alignment_and_all_offsets`, and the Clang/GNU as
branch matrix cover both signed rel8 boundaries, cascades, aliases, explicit
alignment fill and max-skip changes. Temporary-symbol tests check section
relocation addends and preserved GOT/TLS/global/weak/absolute identities.
`layout_dependent_absolute_operands_are_reencoded` checks an immediate whose
width changes after a branch widens. The metadata validator also injects a
forward alias branch and alignment into a compiler-generated debug function,
then checks source breakpoints, backtraces and libgcc unwinding for native and
Clang objects.

The fixed-source measurement script uses two prebuilt compilers, generates each
`.s` once, and links before/after/Clang objects with the same system linker:

```sh
python3 suite/measure_x86_relaxation.py --before /path/to/keel-before \
  --after /path/to/keel-after --prepared /path/to/prepared/target/suite \
  --out /path/to/isolated/benchmark --cpu 2 \
  --before-revision BEFORE_COMMIT --after-revision AFTER_COMMIT
```

Its output records binary/input hashes, every timing sample, object/text sizes,
linked and stripped sizes, temporary-symbol counts, and alternating paired
runtime measurements. Run it on an otherwise idle Linux host; it pins itself
and its children to the requested CPU.

The [layout follow-up report](../suite/reports/native-x86-layout-20261015.json)
compares `426d6fc` with `e3a4bc6` on the i7-1360P NUC. Each mode uses the same
44 fixed Keel-generated assembly files, prebuilt assemblers and the system linker.
Baseline/optimized aggregate object text shrinks **6.88%/6.21%**, leaving it only
**0.08%/0.13%** larger than Clang's. No `.L` symbols remain in the three native
executables. SQLite's linked file shrinks from **5,632,640 to 1,994,048 bytes**
(baseline) and **4,865,824 to 1,924,416 bytes** (optimized), matching Clang's file
sizes. SQLite's stripped sizes also match Clang; exact text, section and symbol counts
are retained in the report.

Five alternating warm assembly batches measure **1.070 to 1.587 seconds**
(baseline) and **0.982 to 1.394 seconds** (optimized): layout iteration costs
**48%/42%** assembly time compared with the original single-pass assembler.
Native assembly remains **2.50x/2.57x faster** than Clang's driver on these files.
The cache retains only symbol-free encodings in a contiguous byte buffer;
symbolic instructions and layout-dependent absolute operands are reencoded.
These timings exclude C compilation and linking.

Eleven alternating paired runtime runs on CPU 2 give baseline/optimized geomeans
**0.65%/0.41% faster** than the original, and **0.08%/0.39% faster** than Clang
assembly of the same source. Individual changes versus the original range from
**1.97% faster to 1.50% slower** (baseline) and **2.69% faster to 0.94% slower**
(optimized). These small, warm measurements include startup and are descriptive;
they do not establish a general runtime speedup. All samples and per-workload
bootstrap intervals are retained.

Validation for this follow-up passes 201 Mac assembler checks (including the
AArch64 suites), 15 codegen library checks, 44 selected Linux assembler checks,
all 424 native runtime fixture variants plus spill regressions, and all 12
application builds (176 objects, 44 test invocations). GNU readers, GDB source
lines/locals/backtraces, libgcc unwinding, and native/Clang GOT/TLS alias execution
pass. The relocation differential also checks temporary section targets with
trailing instruction immediates. Modern AArch64 differential checks run on Mac,
not Linux Clang 18. Strict assembler library/x86-test Clippy passes with the
existing AArch64/parser lint categories allowed; the report names those allowances.


### Integration review

Independent review found a temporary absolute-symbol case missed by the initial
identity filter: `.set .Labs,42; .long .Labs-.` needs a PC32 relocation after
converting the data difference. The assembler now retains the required symbol
instead of panicking. Defined-before-use, forward-definition and forward-alias
cases have explicit relocation tests and Clang/GNU comparisons; resolved
absolute-only data still omits unnecessary temporary symbols.

The second review found no remaining blockers after 80 additional Clang
absolute-difference comparisons and 21 unsupported-input diagnostic checks.
Integrated checks with the macOS linker changes pass 43 selected Mac assembler
checks, 40 linker/driver tests, 45 Linux assembler checks, all 424 native runtime
fixture variants plus spills, and the GNU reader/GDB/libgcc metadata checks.
The report's `review_followup` records these results. Original performance samples
remain tied to the measured `e3a4bc6` implementation; the final symbol-retention
correctness fix was not separately timed.

## Assembly speed follow-up

The layout work made assembly slower because it added repeated layout/emission
passes, but stage profiling also found a larger cost in the existing parser.
For the optimized SQLite input on MacHost, parsing took about 190 ms out of 336 ms;
three layout passes took another 93 ms. QuickJS likewise spent about 182 ms out
of 278 ms parsing. These are warm in-process diagnostic timings excluding file
reads, rather than the CLI batch measurements below.

The follow-up keeps relaxation and its correctness checks. Register names use a
direct match, already-lowercase tokens stay borrowed, and short delimiter/address
lists use inline storage with a heap fallback for larger lists. Symbolic address
terms no longer construct and discard integer-parse error strings. Assembly uses
hash-based symbol lookup and sorts symbols once before ELF emission. Tests cover
mixed case, quoted delimiters, spilled lists and deterministic output across hash
seeds. Accumulator immediate forms omit the unnecessary ModRM byte while retaining
shorter sign-extended imm8 forms. Instruction and relocation tests compare these
encodings with Clang.

A prototype that skipped byte emission on intermediate layout passes did not
improve the large-input timings and was removed. The current implementation still
rebuilds per-pass state; neither a globally minimal branch layout nor a direct
structured x86 compiler sink is implemented by this change.

To inspect the remaining stages without instrumenting normal assembly:

```sh
cargo build --release -p keel-asm --example profile_x86
./target/release/examples/profile_x86 input.s 5
```

Each JSON line records parse time, per-pass layout/branch-check/discard times,
widening counts, ELF finalization and total in-process time. On the same MacHost
SQLite input, warm median parsing falls to about 85 ms, layout to 68 ms, and total
to 188 ms. QuickJS parsing falls to about 71 ms and total to 133 ms. These diagnostic
samples identify bottlenecks; the paired CLI measurements establish the overall
speed change.

The [speed follow-up report](../suite/reports/native-x86-speed-20261015.json)
compares the integrated pre-optimization implementation (`0738fc4`, code-identical
to the session's `a958895`) with `23e141e`. Seven alternating warm paired batches
on NUC CPU 2 assemble the same 44 files per mode:

| Input mode | Keel before | Keel after | Clang driver | Paired time reduction |
| --- | ---: | ---: | ---: | ---: |
| Baseline | 1.561 s | 1.139 s | 3.959 s | 27.2% |
| Optimized | 1.367 s | 0.967 s | 3.618 s | 29.3% |

These timings include process startup, input reads and object writes, and exclude
C compilation and linking. Keel's CLI is 3.48x/3.74x faster than Clang's driver
for these inputs; this is not an isolated LLVM assembler-library comparison.
The baseline/optimized sums of object `.text` shrink a further **225/593 bytes**,
leaving them **0.071%/0.107%** larger than Clang assembly of the same source.
This is a small encoding improvement, not a new compiler optimization.

Eleven alternating paired runtime runs give aggregate changes of **-0.02%/+0.12%**
(baseline/optimized, negative means less time). Individual changes range from
**-1.34% to +1.38%** and **-1.47% to +1.30%**. These measurements do not establish
a runtime improvement or regression. Raw samples and paired bootstrap intervals
are retained in the report.

A separate seven-pair MacHost check assembles just the two large optimized x86
QuickJS/SQLite files into Linux ELF objects: combined medians are **662.6 ms
before, 354.6 ms after, and 468.8 ms for Clang**, a **46.5%** paired reduction.
This is an x86 assembler comparison running on the Mac, not an AArch64 assembler
measurement. Both hosts use prebuilt binaries and the same source within each
comparison. The report includes source/binary hashes, Mac timing samples and raw
stage profiles.

Validation passes all 204 Mac assembler checks, 15 codegen library checks, 47
selected Linux assembler checks, 424 native runtime fixture variants plus spill
regressions, and all 12 application configurations (176 objects, 44 test
invocations). GNU readers, GDB source lines/locals/backtraces and libgcc unwinding
pass. A seeded layout check verifies 3,200 branch targets across 100 graphs with
aliases, padding and max-skip alignment. Formatting and strict Clippy pass with
the existing lint allowances recorded in the report. The measurement helper now
accepts explicit binary revisions instead of embedding the original experiment's
base revision.
