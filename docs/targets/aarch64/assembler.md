# AArch64 assembler

`keel-asm` assembles A64 for Apple arm64 macOS and AArch64 Linux. It separates
instruction encoding, assembly syntax and the object container. The library
returns encoded text and symbols or a complete relocatable object, and
`keel asm input.s -o output.o` writes the object. Link it with the
[native Mach-O linker](../../linking/macho.md), the
[AArch64 Linux path](README.md#assembly-archives-and-native-linking) or a
system linker. The x86-64 assembler has its [own reference](../x86_64/assembler.md).

## Interfaces

`keel-codegen` and `keel-asm` both depend on `keel-asm-ir`; neither depends on
the other. `keel-asm-ir` owns `Program`, `Item`, `Instruction` and the
generated AArch64 `Opcode`/`FieldId` identities, plus separate x86 identities.
AArch64 opcode indices and the assembler's encoding records are generated
together from the pinned Arm archive. The indices are not a stable serialized
format.

`keel_codegen::compile_program[_with_pipeline]` produces a `Program`.
`keel_asm::assemble_program` returns an owned `Assembly`, and
`assemble_program_object` writes Mach-O bytes in memory. `encode_instruction`
encodes one instruction. Field count, range, reserved-bit and CPU feature
checks apply, but the native path indexes the encoding table directly and does
not parse operands. The APIs have no global state.

Labels and symbolic instructions share the layout, fixup, relocation, atom and
unwind machinery of source assembly. `SymbolicInstruction` carries conditional
and compare branches, local ADR references and page, GOT and TLVP references.
`Directive` carries sections, alignment, symbols, bytes, integer expressions,
LEB128, zero-fill, source locations and CFI. The compiler emits every
instruction and metadata directive through these typed paths; no instruction
text is formatted or reparsed. Functions of at least 1 MiB estimated code
expand conditional branches into an inverse-condition skip over a B.

Only user inline assembly creates text items, and statements do not continue
across item boundaries. Inline assembly that needs macro expansion uses a
validated source fallback. `compile` and `cc --emit-object -o output.o` run
these stages without intermediate files; `--emit-ir` cannot be combined with
`--emit-object`.

`keel asm --cpu apple-m5 one.s two.s --out-dir objects` assembles each input to
its own object in one process. Output names use the input stem and `.o`;
collisions and outputs that alias inputs are rejected before writing.
Processing is serial and stops at the first error, keeping completed objects.
Input files must not change while they are parsed.

## Source syntax

Source accepts LF, CRLF and bare CR line endings, Darwin `;` and `//` comments
and `/* ... */` comments. A block comment acts as whitespace, even inside an
operand across lines; diagnostics keep physical line numbers. Mnemonics,
registers and shift names are case-insensitive; labels are case-sensitive.
Integers may be decimal, hexadecimal (`0x`), binary (`0b`) or leading-zero
octal. `x31` and `w31` are rejected: SP and ZR must be named and are checked
against their operand roles. `fp` and `lr` name x29 and x30. Numeric local
labels (`1:`, `1f`, `1b`) are not supported.

Directives include `.text`, `.section __TEXT,__text` (optionally with
`,regular,pure_instructions`), `.globl`/`.global` and `.p2align N` for
0 <= N <= 16, with optional fill and maximum operands. `.globl` declares
external symbols, including undefined ones. `.extern` validates its operands
without changing binding, as Clang does. `.private_extern` sets Mach-O
`N_PEXT`: other objects can resolve the symbol, but the native linker leaves
private definitions and private common storage out of the export trie.
`Directive::PrivateExtern` is the typed form; ELF rejects it.

Named `.macro` definitions end at `.endm` or `.endmacro`. Parameters are plain
unique names and arguments are comma-separated positional values. `\name`
substitutes a parameter and `\()` separates it from following text. Quote
arguments that contain spaces or commas. `.rept`/`.endr` nests with literal
counts. `.if`, `.ifeq`, `.ifne`, `.ifgt`, `.ifge`, `.iflt`, `.ifle`, `.elseif`,
`.else` and `.endif` test integer literals, including literals substituted by
macros. Each invocation or expansion directive must be its own statement.
Keyword, default, required and variadic parameters, Darwin `$0` parameters,
`\@`, symbolic conditions, `.ifdef` and `.include` are unsupported. Expansion
starts only when the parser reaches an expansion directive. Nesting is limited
to 64, expansion work to 100,000 steps and expanded output to 8 MiB.
Diagnostics name both the definition and invocation lines.

## Generated encoding tables

`crates/keel-asm/isa/a64.json` holds selected instruction names, conditions
and encoding records copied from Arm's BSD-licensed AARCHMRS December 2025
release (schema 2.7.4), with the source URL, archive SHA-256, metadata and
license notice. The subset has 1647 instruction and alias forms. Generated
requirements keep Arm's `FEAT_FP`, `FEAT_AdvSIMD`, `FEAT_LSE`, `FEAT_SVE` and
`FEAT_SME` checks and the numeric feature predicates below; the default Apple
arm64 target enables FP, AdvSIMD and LSE only. Encoding conditions are separate
from feature requirements. The generator handles conjunctions, disjunctions,
negation, equality and masked set membership, and rejects unknown features,
fields or expression forms. `SMSTART`/`SMSTOP` keep the upstream alias
conditions with the parent `MSR_SI_pstate` encoding.

The generator extracts fixed bits, masks and variable field layouts; the
assembler duplicates no opcode constants. Operand interpretation (register
roles, immediate ranges, shifts, aliases and PC-relative displacements) is
handwritten. Encoding-name dispatch is a generated `match`.

Normal Cargo builds use the checked-in Rust tables and need no network, Python
or ISA download. To regenerate, with Python 3 and rustfmt:

```sh
python3 crates/keel-asm/tools/generate.py
python3 crates/keel-asm/tools/generate.py --check
```

To reproduce the subset from the original archive:

```sh
curl -L --fail -o /tmp/aarchmrs.tar.gz \
  https://developer.arm.com/-/cdn-downloads/permalink/Exploration-Tools-OS-Machine-Readable-Data/AARCHMRS_BSD/AARCHMRS_OPENSOURCE_A_profile_FAT-2025-12.tar.gz
python3 crates/keel-asm/tools/generate.py --archive /tmp/aarchmrs.tar.gz --check
```

The importer checks SHA-256
`4dc5da62a5c856d7b1086b895075f54807f821ea21a333049cb0f40f9479cecc`
before reading the archive. Arm-derived data and tables carry BSD-3-Clause
terms; keep `crates/keel-asm/isa/LICENSE-ARM` with distributions.

## Integer, floating-point and control-flow forms

Integer forms cover move-wide (`movz`, `movk`, `movn`, `lsl` in 16-bit steps),
add/sub with immediates (`lsl #0` or `#12`), shifted registers and extended
registers including SP; logical operations with shifted registers and A64
bitmask immediates; `adc`/`sbc` families; multiply, high multiply, widening
multiply-add and division; shifts and rotates; `clz`, `cls`, `rbit`, `rev*`;
bitfield moves and their aliases; and `neg`, `negs`, `mvn`, `tst`, `ngc`,
`ngcs`. `mov #imm` chooses one `movz`, `movn` or `orr`; longer constants must
be written as `movz`/`movk` sequences.

Conditional forms cover `csel`, `csinc`, `csinv`, `csneg` and their aliases,
and `ccmp`/`ccmn` with a register or 5-bit immediate. Inverting aliases reject
`al` and `nv`. `b.cond`, `cbz`/`cbnz` (±1 MiB) and `tbz`/`tbnz` (±32 KiB) check
their ranges and are not relaxed; W names in test branches need bit indices
below 32. `br`/`blr` need X registers, and `brk`/`svc` take a 16-bit immediate.

Scalar S and D floating-point forms cover moves, absolute value, negation,
square root, all rounding modes, arithmetic, min/max, fused multiply-add,
`fcmp`/`fcmpe` against a register or `#0.0`, `fcsel`, `fcvt` between S and D,
and integer conversions with W/X. `fmov` covers S/W and D/X bit transfers and
all 256 FP immediates. `movi` covers all six modified-immediate forms.

## NEON

`crates/keel-asm/isa/neon.txt` lists all 519 baseline Advanced SIMD records in
the pinned archive (`asimd*` and `asisd*` classes whose predicates need only
AdvSIMD), and the importer checks it against the archive. The parser
implements every record, including legal arrangements and high-half variants:
integer and floating arithmetic, comparisons, logical operations, saturating
and rounding forms, widening and narrowing, reductions, shifts, lane-indexed
multiply, lane moves, permutations, `ext`, `tbl`/`tbx`, conversions, modified
immediates, and structured loads and stores with 1 to 4 registers, lane and
replicating forms and post-index addressing. Operand validation rejects
reserved sizes, invalid lane indices and nonconsecutive lists. 64-bit
polynomial multiplication needs FEAT_PMULL.

`tests/neon-cases.txt` names one case per record. Regenerate it with:

```sh
python3 crates/keel-asm/tools/neon_cases.py
python3 crates/keel-asm/tools/neon_cases.py --check
```

## Optional features

Defaults stay at the fixed Apple baseline. Library callers select optional
encodings with `assemble_with_features` and `assemble_object_with_features`;
the CLI uses `--cpu apple-m5` or `--cpu baseline`, and unknown names are
errors. Feature flags permit encodings only. They do not probe the host,
insert state transitions, change Mach-O CPU subtypes or promise execution
support, and they do not change compiler code generation.

### M5 numeric extensions

`TargetFeatures` has independent builders `with_fp16`, `with_fhm`, `with_bf16`,
`with_dotprod`, `with_i8mm`, `with_rdm`, `with_fcma`, `with_jscvt` and
`with_frintts`. `crates/keel-asm/isa/m5-numeric.txt` lists the 217
non-scalable records these features select, and all are implemented: dot
products, matrix multiply, BF16 operations, RDM, complex arithmetic, widening
FP16 multiply-add, FJCVTZS, FRINT32/64, and scalar and vector FP16 arithmetic
and conversions. Indexed forms check group sizes, lane bounds, multiplier
registers and rotations. H-element complex arithmetic also needs FP16. H↔S/D
conversions use their baseline FP requirement.

`TargetFeatures::apple_m5()` selects the baseline, these nine features,
SME/SME2, the SME FP64, I16-to-I64, non-widening FP16 and non-widening BF16
types, LRCPC and LRCPC2, and the compatibility features below, following
LLVM's [M5 definition](https://raw.githubusercontent.com/llvm/llvm-project/main/llvm/lib/Target/AArch64/AArch64Processors.td).
It does not enable ordinary SVE.

```sh
python3 crates/keel-asm/tools/numeric_cases.py
python3 crates/keel-asm/tools/numeric_cases.py --check
```

### SVE and SME

NEON uses fixed-width V registers, SVE uses scalable Z and P registers, and SME
adds streaming mode and ZA matrix state. LLVM's Apple definitions list SME and
SME2 for M4 and M5 and note that M4 lacks ordinary SVE.
[XNU's SME documentation](https://github.com/apple-oss-distributions/xnu/blob/main/doc/arm/sme.md)
says Apple implementations expose SVE registers and instructions only in
streaming mode. Mode transitions clobber vector and predicate registers, and
many NEON instructions are illegal while streaming. Check CPU and OS support at
runtime, for example with `sysctl hw.optional.arm.FEAT_SME`.

`TargetFeatures::default().with_sme()` and `.with_sve()` enable each family
independently; Arm's `SVE || SME` requirements accept either. `with_sme2()`
implies SME. `with_sme_f64f64()` and `with_sme_i16i64()` enable SME and their
matrix type; `with_sme_f16f16()` and `with_sme_b16b16()` also enable SME2. SME2
alone permits none of these four types. No builder enables ordinary SVE.
Callers own streaming mode, the calling convention and ZA lifetime.

Implemented scalable forms:

- Unpredicated Z `add`/`sub` on B/H/S/D, `and`/`orr`/`eor` on D and
  `fadd`/`fsub`/`fmul` on H/S/D; `ptrue`, `pfalse`, `dup`, `lastb`, `rdvl` and
  `cnt[bhwd]`.
- `smstart`/`smstop`, `zero {za}` and partial tile lists, and the 123 matrix
  records in `crates/keel-asm/isa/sme.txt`: FP32, FP64, FP16, BF16 and integer
  outer products with add and subtract forms, `addha`/`addva`, ZA slice moves
  and loads/stores, whole-ZA `ldr`/`str`, `rdsvl`/`addsvl`, SME2 multi-vector
  moves, ZA-array `add`/`sub`/`fmla`/`fmls`, and ZT0 `ldr`/`str`/`zero`.
- 46 streaming-compatible forms: `whilelt`/`whilele`/`whilelo`/`whilels`,
  contiguous `ld1*` and `st1*` with `mul vl` or scaled register offsets,
  `addvl` and `dup` from a GPR. They keep their `SVE || SME` requirement and
  the Rm=31 exclusion.

Predicated Z arithmetic, gather/scatter and first-fault memory, vector shifts
and conversions, SME2 grouped arithmetic and predicate-counter memory, ZA dot
products and lookup operations, SME2.1 and ordinary SVE2 are not implemented.

```sh
python3 crates/keel-asm/tools/sme_cases.py --check
python3 crates/keel-asm/tools/streaming_cases.py --check
cargo test -p keel-asm --test sme
```

`tests/fixtures/sme-kernels/{fp32,bf16,int8}.s` are correctness fixtures for
packed matrix multiplication that need only SME. They are ordinary PCS leaves
with inactive ZA on entry, balanced mode transitions and preserved D8–D15.
Their private fixture ABI takes pointers plus M, N and the number of K blocks.

### Compatibility features

Optional builders, all selected by `apple_m5()`, add PAC/AUT/XPAC, BTI, AES,
SHA-1, SHA-256, CRC32/CRC32C, 64-bit PMULL, CASP, prefetch forms, selected
MRS/MSR registers and DC/IC operations, numeric SYS syntax, CSSC, RCpc
acquire loads, LDPSW, scalar FP rounding conversions, conditional FP compare
and fixed-point conversions. `with_lrcpc2()` enables the 13 LRCPC2 unscaled
acquire-load and release-store forms (`ldapur*`, `stlur*`) with signed byte
offsets in -256..255 and implies `with_lrcpc()`. `crates/keel-asm/isa/m5-compat.txt`
lists the records and `crates/keel-asm/isa/system.json` the system-register
names and accessors. Acceptance of a system or cache instruction is permission
to encode, not a promise of EL0 execution. SHA-512, SHA-3, MTE, generic
system-register syntax and newer PAC extensions are not supported.

## Loads and stores

Single transfers cover `ldr`/`str` for W, X, B, H, S, D and Q; `ldrb`, `strb`,
`ldrh`, `strh`; `ldrsb`, `ldrsh` and `ldrsw`; and the unscaled `ldur*` and
`stur*` forms. Bases are X registers or SP. Addresses:

- `[base]` or `[base, #offset]`: unsigned 12-bit offsets scaled by the access
  size. As in Clang, the ordinary mnemonic selects the unscaled encoding for
  small negative or unaligned offsets. Unscaled mnemonics use a signed 9-bit
  byte offset.
- `[base, #offset]!` and `[base], #offset`: pre- and post-index writeback with
  signed 9-bit byte offsets. Unscaled mnemonics have no writeback.
- `[base, Xm]`, `[base, Xm, lsl #shift]`, `[base, Xm, sxtx {#shift}]` and
  `[base, Wm, uxtw|sxtw {#shift}]`: register offsets with shift zero or
  log2(access size). An explicit byte-transfer `#0` keeps Arm's distinct
  S-bit encoding.

`ldp`/`stp` transfer W, X, S, D or Q pairs with offset, pre-index or post-index
addresses and signed 7-bit scaled offsets. Repeated load-pair destinations and
integer writeback-base overlaps are rejected as constrained unpredictable.

`ldar`/`stlr` and their byte and halfword forms accept `[base]` or
`[base, #0]` only. LSE atomics cover LDADD, LDCLR, LDSET, LDEOR, LDSMIN,
LDSMAX, LDUMIN, LDUMAX, SWP and CAS, each with relaxed, `a`, `l` and `al`
orderings and B/H/W/X widths, plus the ST* store aliases (relaxed or release).
Exclusive accesses cover `ldxr`, `ldaxr`, `stxr`, `stlxr`, their pair and
byte/halfword forms, and `clrex`. Store status must not overlap a source or
non-SP base. DMB and DSB accept the named domain options and a raw 4-bit
value; ISB defaults to SY.

Literal `ldr` (W, X, S, D, Q) and `ldrsw` accept a numeric displacement or a
symbol in the same text atom plus an addend, 4-byte aligned in
-1048576..1048572. External, cross-section and cross-atom literals are
rejected because Mach-O has no literal-load relocation. There is no
`ldr =constant` pool.

## Symbols, relocations and data

`adr` takes a defined text label plus an optional addend, within a signed
21-bit displacement. Numeric `adrp` takes a 4 KiB-aligned displacement.
Symbolic `adrp` uses `symbol@PAGE`, `@GOTPAGE` or `@TLVPPAGE`. `add` and scaled
single loads and stores accept `symbol@PAGEOFF`; GOT and TLVP page offsets
require `ldr Xt, [base, symbol@GOTPAGEOFF]` or `@TLVPPAGEOFF`. Symbolic memory
operands have no writeback or unscaled form.

Expressions take one symbol and integer addends (`symbol+8`, `symbol@PAGE+8`,
`symbol+8@PAGE`), which must fit signed 24 bits. Nonzero PAGE, PAGEOFF and
external branch addends use `ARM64_RELOC_ADDEND` before the matching
relocation. GOT and TLVP addends are rejected, and branch addends must be
4-byte aligned. Undefined `b`/`bl` targets produce `ARM64_RELOC_BRANCH26`.
Page references produce PAGE21/PAGEOFF12, GOT_LOAD_* or TLVP_LOAD_* entries
against symbol-table indices. Names starting with `L` are local and need a
definition unless declared external. `adr` and conditional or test branches
need targets in the same file and atom.

`.byte`, `.short`, `.long` and `.quad` emit little-endian integers.
`.space`/`.zero` reserve bytes with an optional fill. `.ascii`/`.asciz` support
the standard escapes. Floating-point data directives are unsupported; emit
integer bit patterns instead. `.data`, `.const`, `.const_data` and `.bss` select
the usual sections; `.section` also accepts the TLS sections
`__thread_data`, `__thread_bss` and `__thread_vars`. `.zerofill`, `.tbss` and
`.comm` allocate named zero-fill storage without switching sections;
initializers and relocations in zero-fill sections are errors.

`.quad symbol [+/- integer]` emits an absolute pointer relocation. `.long` and
`.quad A-B [+/- integer]` resolve directly within one section or atom and use
`ARM64_RELOC_SUBTRACTOR` plus `ARM64_RELOC_UNSIGNED` otherwise, with the addend
in the data word. Repeated identical symbols cancel. Constant assignments
(`name = expression`, `.set`) accept constants and same-section differences.
Shifted expressions follow LLVM's logical 64-bit right shift. General
algebra and 8- or 16-bit symbolic relocations are unsupported.

## Objects

The writer emits a little-endian arm64 `MH_OBJECT` with one unnamed segment.
File-backed sections precede zero-fill sections. Relocations are ordered by
descending offset within each section, keeping ADDEND and SUBTRACTOR pairs
together. Symbols are ordered as local definitions, external definitions, then
undefined symbols. TLS descriptor sections set `MH_HAS_TLV_DESCRIPTORS`.
Objects carry an LC_BUILD_VERSION for macOS 11.0, or the `.build_version`
deployment and SDK values. `.loh` emits all eight LC_LINKER_OPTIMIZATION_HINT
kinds; the labels must be aligned text within one atom. The segment virtual
size is `max(vmsize, filesize)`.

`.subsections_via_symbols` sets `MH_SUBSECTIONS_VIA_SYMBOLS`, letting the
linker reorder or discard atoms. Non-temporary and global temporary labels
start atoms; aliases at one address share one. Temporary labels keep
source-order ownership, so a function's end label belongs to it. Local
references are rebased onto atom symbols with explicit addends. B/BL across
atoms use branch relocations; ADR and conditional branches must stay inside
their atom. Data differences across atoms keep SUBTRACTOR/UNSIGNED pairs.
Symbolic LEB128 differences must stay within one atom, except in debug
sections, where object-space differences are allowed for Clang's DWARF 5
range and location lists.

Darwin temporary `L…` labels stay out of nlist unless referenced. Absolute
pointers to temporary labels use section relocations. Other required temporary
text symbols are marked `N_ALT_ENTRY`.

## Unwind information

`.cfi_startproc`/`.cfi_endproc` collect CFA and saved-register rules:
`.cfi_def_cfa`, `.cfi_def_cfa_register`, `.cfi_def_cfa_offset`, `.cfi_offset`,
`.cfi_restore`, `.cfi_same_value`, `.cfi_undefined`, balanced
`.cfi_remember_state`/`.cfi_restore_state` and `.cfi_negate_ra_state`, which
forces DWARF unwind. X registers are DWARF 0–30, SP is 31 and D/V registers are
64–95. The assembler writes CIE/FDE records in `__TEXT,__eh_frame` and one
`__LD,__compact_unwind` entry per function.

Compact frame mode needs CFA = X29 + 16, X29 at CFA - 16 and X30 at CFA - 8.
Optional X19/X20 through X27/X28 and D8/D9 through D14/D15 pairs must sit
directly below, in increasing pair order, integer pairs first, with the
lower-numbered register at the higher address. Frameless mode covers functions
with no saved registers and one `.cfi_def_cfa_offset` of 0 to 65520 bytes,
16-byte aligned. Other layouts keep DWARF. Functions with compact encodings omit
their FDEs. Compact personalities are limited to `___gxx_personality_v0` and
`___objc_personality_v0`; other personalities keep a CIE/FDE.

For DWARF unwind, `.cfi_personality 155, symbol` adds a GOT-indirect,
PC-relative personality reference, and `.cfi_lsda 16, symbol` adds a
PC-relative LSDA pointer. LSDA data lives in `__TEXT,__gcc_except_tab`, and
type-table entries `.long typeinfo@GOT-label` emit
`ARM64_RELOC_POINTER_TO_GOT`.

## Debug line tables

`.file number "path"`, `.file number "directory" "name"` and
`.loc file [line [column]] [basic_block] [prologue_end] [epilogue_begin]
[is_stmt 0|1] [isa N] [discriminator N]` generate a DWARF 4 line program in
`__DWARF,__debug_line`. As in LLVM, `is_stmt` persists across `.loc`
directives and the other options apply to one row. File numbers are contiguous
from 1, and unnumbered `.file` is an error. Explicit debug sections
`__debug_info`, `__debug_abbrev`, `__debug_line`, `__debug_ranges`,
`__debug_str`, `__debug_loc`, the Apple accelerator tables and the DWARF 5
sections are copied through with `S_ATTR_DEBUG`. `.uleb128`/`.sleb128` encode
constants and same-section differences; layout repeats until positions
settle, and a symbolic field never shrinks.

The compiler supplies types, variables and scopes; the assembler supplies line
and unwind tables. On macOS the linker records a debug map and `dsymutil`
builds a dSYM.

### DWARF 5

`.file 0 "compilation-dir" "name"` switches the line table to version 5, as
in LLVM's `AsmParser::parseDirectiveFile`; `clang -gdwarf-5 -S` emits it first.
Without `.file 0` the DWARF 4 output is unchanged, and `md5` and `source`
operands are errors. There is no `--dwarf-version` flag. Keel's own code
generator emits DWARF 4. There is no split DWARF, type unit or DWARF64
support.

```
.file 0 "compilation-dir" "name" [md5 <integer>] [source "text"]
.file N ["dir"] "name"           [md5 <integer>] [source "text"]
```

- `md5` takes an integer of at most 128 bits, stored big-endian as
  `DW_FORM_data16`. `source` takes a quoted string. `.loc 0` names the root.
- The root directory is include directory 0 and its name is unsplit.
- For numbered files, a directory equal to the compilation directory is
  dropped, an empty name becomes `<stdin>`, a name without a directory is
  split at its last `/`, and directories are deduplicated from index 1.
- A numbered entry with the root's name and checksum folds into file 0, and a
  later `.loc` to its number is an error, as in Clang.
- The header uses LLVM's standard parameters. File entries always have
  `DW_LNCT_path` and `DW_LNCT_directory_index`, `DW_LNCT_MD5` when every file
  has a checksum, and `DW_LNCT_LLVM_source` (0x2001) when any file has source.
- `__debug_line_str` deduplicates strings in first-use order. Explicit
  contents come first and generated offsets account for them. Mach-O needs no
  relocations for these offsets.
- An empty source is written as `"\n"`, following upstream LLVM; Apple
  clang-2100 writes `""`.
- A version 5 table is emitted whenever `.file 0` exists, even with no rows.
- The line program uses standard opcodes rather than LLVM's special opcodes,
  so decoded rows match while `__debug_line` is larger.

Keel rejects what LLVM would warn about or guess: `.file 0` without a nonempty
directory, a missing root name, a duplicate `.file 0` or one after a numbered
`.file` or `.loc`, a folded entry whose source differs from the root's,
duplicate or malformed operands, NUL in paths or source, paths ending in `/`
or starting with `//`, and gaps in file numbers.

Clang's verbose assembly can put non-UTF-8 bytes in comments. When strict
decoding fails, `keel asm` accepts input whose invalid bytes all lie in `;` or
`//` comments outside quotes.

## Tests

Tests compare every supported encoding form and operand boundary, relocations,
section data, unwind records and line tables with Clang's arm64 Mach-O output,
and link and run programs on Apple Silicon.
`suite/asm/audit_clang_assembler.py` assembles Clang `-S -O2` output of the
repository's C fixtures and Lua sources with both assemblers and compares code
and data bytes, normalized relocations, section attributes, build versions and
linker hints. `crates/keel-asm/tests/dwarf5.rs` covers DWARF 5 tables:

```sh
cargo test -p keel-asm --test dwarf5
```

See the [suite runners](../../../suite/README.md) for application comparisons.
