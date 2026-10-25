# Shared x86 instruction tables

Keel's x86 assembler and disassembler use one generated form catalogue.
The instruction-specific encoders have been replaced by operand matching and
common prefix, ModRM/SIB, immediate and relocation emission. Compiler lowering,
intrinsic semantics and the CPU profiles remain separate.

## Regeneration

```sh
python3 tools/x86-isa/generate.py
python3 tools/x86-isa/generate.py --check
python3 -m unittest discover -s tools/x86-isa -p 'test_*.py'
```

Ordinary Cargo builds use checked-in Rust. They need no network access,
Python, C decoder, or upstream generator.

The input is a subset of the MIT-licensed
[Zydis instruction database](https://github.com/zyantific/zydis-db/tree/eaf92a02dca616b90c12b2ebc338d87ed943c84f),
pinned to `eaf92a02dca616b90c12b2ebc338d87ed943c84f`. The snapshot retains
legacy and VEX records except explicit non-long-mode records, removing only
the upstream comment field. `zydis-source.json` records full-file and subset
SHA-256 values. Audit a separately downloaded upstream file with:

```sh
python3 tools/x86-isa/generate.py --check-upstream /path/to/instructions.json
```

The generator rejects unknown filter names, unsupported filter values and
unclassified feature sets used by Keel adapters. It excludes REX2 and the
alternate AMD, KNC and MPX interpretations. The default decoder uses the
ModRM-bearing UD0 form; Keel's explicit `ud0.short` encoding remains available.
EVEX, MVEX, XOP and 3DNow records are outside this import.

## Inputs and outputs

- `zydis.json` supplies opcodes, prefixes, operand roles, size rules, ModRM
  domains, VEX fields, ISA sets and explicit privilege metadata.
- `keel-adapters.json` binds stable public enums and dialect aliases to source
  forms. Encoding selectors identify exact alternatives. Memory-width and
  family expectations guard compatibility; they do not supply emitted bytes.
- `feature-fields.json` lists capability fields in public struct and `names()`
  order, with field docs, CLI aliases, stable bit IDs and prerequisites.
  `feature_fields.py` generates the struct, `NONE`, all-known set, containment,
  names, fingerprint, enabling closure and strict validation.
- `cpu-profiles.json` defines cumulative level membership and the v1 default.
  `cpu_profiles.py` emits constructors, level lookup and the membership table.
- `registers.json` defines register class IDs, symbolic tags, names, number/width
  domains, parser spelling rules and formatter compatibility. `registers.py`
  generates the public register types, parsing, structural validation, formatting,
  ISA-kind mappings and decoder register names. Class IDs and symbolic tags are
  independent fields; changing either requires a compatibility review.
- `api-identities.json` preserves baseline enum order, payload types, static
  spellings and x87 identities. `api_identities.py` emits their Rust definitions
  and mappings without consulting instruction encodings. Condition codes and
  Core identities have separate schemas; wrapper dispatch and parameterized
  Core grammar remain handwritten. `Pclmulqdq` retains its
  position in the handwritten parse fallback.
- `feature-policy.json` maps upstream ISA sets onto Keel feature fields.
  Empty lists mean baseline capabilities; null entries exclude unsupported
  interpretation families. The generator rejects undeclared feature fields.
  Profile membership is generated from `cpu-profiles.json`; dependency closure
  and strict validation come from prerequisites in `feature-fields.json`.
  Target-option provenance stays in the CLI rather than the capability set.
- `keel-extensions.json` preserves the existing intentional long-mode D6 trap
  spelling, `udb`, which is absent from this upstream snapshot.

Generated files under `crates/keel-asm-ir/src/x86/isa/` provide the shared
catalogue, decoding index, 93 v2 metadata entries, 1,226 VEX form adapters,
64 fixed encodings and opcode feature queries. Public fixed/v2/v3 enums, their
`ALL` lists, parse aliases and mnemonic mappings are also generated from the
explicit identity policy. Existing enum order and spellings are preserved.
The baseline formatting macro expands into an exhaustive match with handwritten
dynamic arms. Compatibility tests pin the original API schema and exercise an
independent spelling/format fixture captured from `db6129d3`.
The generator also reconciles
encoding fields in `docs/v2-coverage/inventory.json`, retaining its operand and
validation prose. AES entries are optional and are not implied by psABI v2.

The assembler keeps stable public opcode identities and syntax adapters. Handwritten
code handles branch relaxation, exact alias selection, operand conventions,
canonical choices, addressing and object relocations. It contains no fallback
to the removed instruction-specific encoders. Adding a public instruction
still requires its syntax/API identity and any compiler semantics; importing
an ISA form does not add an intrinsic or make it available for selection.

## Register mappings

The register schema preserves the existing GPR, XMM, MMX, YMM, x87, segment,
instruction-pointer, control and debug register APIs. `Register` keeps its public
`class`, `number`, `width` and `high_byte` fields. Parser case folding, aliases
and numeric spelling rules stay unchanged. CR/DR names accept exactly one
numeric digit; CR indices are 0/2/3/4/8 and DR indices are 0..7. DR4/5 retain
their documented conditional alias preconditions.

`Register::is_valid()` checks the structural register domain. Instruction
operands, addressing, high-byte/REX compatibility and the current four-bit
legacy/VEX register-number limit remain encoder rules. The assembler uses the
generated class validation and ISA-kind mapping; symbolic serialization uses
the generated tag mapping. The decoder uses the same canonical GPR names and
generated names for its other register fields.

Formatting compatibility is separate from structural validity. Existing
permissive formatting of malformed XMM/YMM, segment, instruction-pointer and
high-byte GPR values is preserved; control/debug formatting remains strict.
Decoded fields also retain their existing recognition limits, including raw
reserved CR/DR numbers. `decoder_only` name rules for mask, bounds and tile
register fields do not add assembler register classes or instruction support.

Schema generation rejects duplicate IDs/tags/spellings, inconsistent number
domains, ambiguous duplicate patterns and unknown schema constraints. A frozen
canonical register fixture captured from `5b04a1aa` protects existing public
names and field values independently of the generator.

## Core identities and condition codes

`core-identities.json` owns the ordered 70-variant `CoreOpcode` API, including
its five payload shapes, static parse aliases and static mnemonic names.
`core_mappings.py` emits `core_opcodes.rs` and `core_mnemonics.rs`. The latter
is an exhaustive match macro whose caller supplies the five dynamic cases.
String/loop/far-return parsing, prefix handling, width checks and instruction
semantics remain in `core.rs`. Wrapper dispatch precedence stays unchanged.

`condition-codes.json` owns the sixteen numeric condition codes, their accepted
aliases, public formatting names and imported ISA names. For example, public
`ae` maps to ISA `nb`, and public `e` maps to ISA `z`. The generated
`condition_codes.rs` serves parsing, formatting and encoder canonicalization.
Public `Jcc(u8)`, `Setcc(u8)` and `Cmovcc(u8)` payloads stay unchanged; invalid
condition numbers have no generated name. The decoder retains the mnemonic
recorded in the shared ISA catalogue.

Compatibility fixtures and generator pins come from `3109c651`, before this
move. Literal opcode-family tests cover every accepted condition alias through
the generic encoder. These schemas describe names and API shape, not compiler
effects, architectural permission or runtime behavior.

## AT&T dialect mappings

`att-rules.json` owns regular suffix widths, scalar suffix eligibility,
far-memory widths, sign/zero-extension spellings, baseline and additional x87
memory spellings, simple aliases, and the integer/packed conversion mappings.
`att_rules.py` checks opcode references against the existing identity schemas
and generates `att_rules.rs`. Optional extension suffix eligibility continues
to come from the extension adapters rather than a second list.

The parser consumes these lookups in its original precedence order. Address
and branch grammar, operand reversal, suffix/register checks, implicit shift
counts, x87 subtraction/division operand semantics, prefix rules and relocation
restrictions remain handwritten. CR/DR and segment MOV exceptions remain
explicit. This move preserves accepted source, structured operands and errors;
it does not add AT&T instruction forms.

The frozen AT&T fixture comes from `14bed00d`, before this move, and contains
448 accepted or rejected source cases. The existing assembler suites retain
independent Clang/GNU encoding and relocation witnesses.

CPU profiles use the same generation pipeline. See the
[generated CPU-profile contract](cpu-profiles.md) for current memberships,
dependency closure, v1 defaults and ordered CLI modifier resolution.

## Optional legacy/VEX extensions

The `extensions` list in `keel-adapters.json` exposes 16 additional mnemonics
through generated `ExtensionOpcode` identities (46 imported forms): ADCX/ADOX, the seven SHA
instructions, VAESENC/VAESENCLAST/VAESDEC/VAESDECLAST, VAESIMC,
VAESKEYGENASSIST and VPCLMULQDQ. These use the existing shared matcher and
encoder. No compiler intrinsics or selection rules are added.

Enable `adx` or `sha` independently. XMM VAES forms require `avx,aes`;
YMM VAES forms require `avx,vaes`. VAESIMC and VAESKEYGENASSIST have XMM forms
only. XMM VPCLMULQDQ requires `avx,pclmulqdq`; YMM requires
`avx,vpclmulqdq`. None of the four new feature bits is implied by psABI v3.
The source labels XMM VPCLMULQDQ simply `AVX`; the generated adapter uses an
explicit `AVX_PCLMULQDQ` policy label to retain its independent PCLMULQDQ
requirement. The shared source catalogue retains the upstream ISA label.
The XMM requirement is documented in Intel's
[instruction reference, volume 2B](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf).

New extension identities, scalar AT&T suffix eligibility, operand-specific
VEX form adapters and feature queries are generated from that exposure list
and the source records. Adding a feature field requires a schema entry; deciding
the CPU profile policy remains an explicit profile-schema change. EVEX/AVX-512 vector forms remain outside
this increment.

## Decoding limits

`keel-disasm::x86::decode(bytes, address)` returns original bytes, the matching
form and structured operands. Decoding uses an opcode index, reads at most
15 bytes and returns no length for unsupported, invalid or truncated input.
The CLI prints the rest of that section as data after an unknown instruction.
It does not guess subsequent boundaries.

The catalogue is an encoding description, not exhaustive architectural decode
pseudocode. Recognition does not certify CPU support, register restrictions or
runtime privilege. Upstream privilege metadata is optional; absent CPL is
unknown. The sampled LGDT record lacks the CPL0 metadata present in XED.
Compiler APIs retain their explicit preconditions and separate CPL0 tests.
Aliases, ignored prefixes and alternative encodings can produce assembly text
that reassembles differently. Original bytes remain authoritative.

## Independent witnesses

The original XED pilot remains in `probe.py`, `system-xed.txt` and `source.json`.
It preserves repeated PATTERN/OPERANDS pairs and imports 22 system forms,
excludes eight non-long-mode forms and reports four control/debug register
forms as unsupported by that pilot's grammar. Those pilot gaps are unchanged;
the production catalogue separately describes control/debug register forms.
No compiler access APIs are inferred from them.

```sh
python3 tools/x86-isa/probe.py
python3 tools/x86-isa/probe.py --emit-rust > /tmp/keel-x86-forms.rs
rustc --crate-type lib /tmp/keel-x86-forms.rs -o /tmp/libkeel_x86_forms.rlib
```

Existing Clang/GNU assembler comparisons remain independent byte and relocation
oracles. Decoder tests include literal legacy, VEX and privileged encodings,
truncation, invalid prefixes, the 15-byte limit and every one/two-byte start.
Neither table generation nor an encoding comparison proves execution semantics.

See `LICENSE-ZYDIS` for production ISA data and `LICENSE-XED` for the pilot.

## System intrinsic declarations

`system-intrinsics.json` owns the 45 existing x86 system intrinsic declarations
at IDs 774 through 818. `system_intrinsics.py` generates their enum variants,
`ALL` inventory entries and length, names, parameter/result types, Rust docs,
purity, compiler memory effects and x86-only classification patterns. Each row
also requires a validated regular lowering recipe or an explicit MSR hook. Name lookup
continues to use `ALL` and `info()`, so it consumes the same generated names.
The earlier intrinsic declarations remain handwritten.

The generated enum and inventory macros retain the handwritten prefix. Numeric
IDs and `ALL` iteration order are separate contracts: the earlier prefix does
not iterate in numeric order. Frozen fixtures captured from `a80be8ca` check
all 819 numeric IDs and inventory entries independently, plus every migrated
signature, name, purity, memory effect and target classification.

Effects and target availability are explicit reviewed policy. They are not
inferred from the encoding catalogue or mnemonic. All current rows are pinned
compiler memory barriers and x86-64-only; a compiler barrier does not promise
hardware fencing. The schema validates contiguous explicit IDs, unique names
and variants, supported scalar types, effects and target domains. Pure entries
cannot claim memory effects. Generic instruction-memory classification and
AArch64 rejection/exhaustive classification consume the generated policy.

To add another system intrinsic in this append-only range:

1. Append its declaration with the next stable ID, signature, effects, target
   and contract documentation to `system-intrinsics.json`.
2. Run `python3 tools/x86-isa/generate.py`. Declarations, lookup inputs, effects
   and target classifications follow the new row.
3. Select a supported lowering recipe and review privilege, state and continuation
   contracts. Existing recipe bindings and dispatch are generated; shared emitters,
   new recipe semantics, MSR sequences and terminal transfers remain handwritten.
   Add independent encoding/runtime witnesses as needed.
4. Extend the compatibility fixtures for the intentional new ID and signature,
   then run the generator tests, freshness checks and affected Rust suites.

This migration introduces no instruction, intrinsic ID, runtime permission,
feature requirement or kernel entry/return contract. Existing lowering and
privileged qualification fixtures remain separate.

The [regular system-lowering contract](system-lowering.md) describes generated
bindings for 43 existing intrinsics through four shared recipe families and two
explicit MSR hooks. It records validation, allocation, adjacency and terminal
transfer boundaries.
