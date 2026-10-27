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

The generated legacy/VEX and EVEX catalogues share repeated operand lists
within each table. Imported operand-size rules use an exhaustive Rust enum;
unknown rules stop generation. The `isa_table_layout` Cargo example reports
record sizes and distinct operand-slice storage.
See the [Rust refactor review](../../docs/rust-refactor-review.md) for measured
source-size changes and remaining representation work.

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
The legacy/VEX selection remains unchanged. A separate `evex.json` selection
adds AVX-512F/VL and its VEX mask operations, from the same upstream pin. Other
EVEX families, MVEX, XOP and 3DNow remain outside these imports.

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
67 fixed encodings and opcode feature queries. Public fixed/v2/v3 enums, their
`ALL` lists, parse aliases and mnemonic mappings are also generated from the
explicit identity policy. Existing enum order and spellings are preserved.

Fixed adapters also generate their source mnemonic, operand-size selection and
leading-FWAIT normalization. The decoder uses generated width-specific names
for canonical formatting. [SYSEXIT/RSM support](../../docs/x86-system-exits.md)
adds public adapters without another instruction-specific encoder or compiler
terminal API.
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
GNU `db0` through `db7` aliases parse as the corresponding debug registers;
canonical formatting remains `dr0` through `dr7`.

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

The `extensions` list in `keel-adapters.json` exposes 26 additional mnemonics
through generated `ExtensionOpcode` identities: ADCX/ADOX, the seven SHA
instructions, VAESENC/VAESENCLAST/VAESDEC/VAESDECLAST, VAESIMC,
VAESKEYGENASSIST and VPCLMULQDQ. These use the existing shared matcher and
encoder. No compiler intrinsics or selection rules are added.

The kernel adapters also expose RDTSCP, RDFSBASE/RDGSBASE/WRFSBASE/WRGSBASE,
RDRAND, PREFETCHW and INVPCID. They require the independent `rdtscp`,
`fsgsbase`, `rdrand`, `prefetchw` and `invpcid` capabilities. None is implied
by the v1/v2/v3 profiles. Runtime privilege and enabled-state requirements remain
the caller's responsibility. Upstream's `PREFETCH_NOP` label for PREFETCHW is
overridden only in the generated feature query, preserving the source catalogue.
AT&T `fxrstorq` and `fxsaveq` aliases select existing 64-bit state forms.

RDSEED and TPAUSE require the separate `rdseed` and `waitpkg` features; neither
changes CPU-profile membership. `sysretl` aliases the existing 32-bit SYSRET
return form in long mode, distinct from `sysretq`. These are assembler adapters,
not new compiler intrinsics or execution qualification. TPAUSE takes a
fixed-width 32-bit GPR control operand. GNU as requires unsuffixed RDRAND/RDSEED/TPAUSE
mnemonics, so suffix inference now excludes those operand families. Canonical
RDRAND and RDSEED still admit 16-, 32- and 64-bit register views. Clang's
additional random-instruction suffix aliases are outside this GNU syntax subset.

Enable `adx` or `sha` independently. XMM VAES forms require `avx,aes`;
YMM VAES forms require `avx,vaes`. VAESIMC and VAESKEYGENASSIST have XMM forms
only. XMM VPCLMULQDQ requires `avx,pclmulqdq`; YMM requires
`avx,vpclmulqdq`. ADX/SHA/VAES/VPCLMULQDQ are not implied by psABI v3.
The source labels XMM VPCLMULQDQ simply `AVX`; the generated adapter uses an
explicit `AVX_PCLMULQDQ` policy label to retain its independent PCLMULQDQ
requirement. The shared source catalogue retains the upstream ISA label.
The XMM requirement is documented in Intel's
[instruction reference, volume 2B](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf).

New extension identities, scalar AT&T suffix eligibility, operand-specific
VEX form adapters and feature queries are generated from that exposure list
and the source records. Adding a feature field requires a schema entry; deciding
the CPU profile policy remains an explicit profile-schema change. Other
EVEX/AVX-512 families remain outside this increment.

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

## Intrinsic registry and family declarations

`intrinsic-registry.json` owns all 819 intrinsic variants, explicit numeric IDs,
`ALL` positions and metadata ownership. It includes target-neutral intrinsics,
not only x86 intrinsics. `intrinsic_registry.py` generates the public enum and
inventory in `crates/keel-ir/src/generated/intrinsic_registry.rs`. The public
`Intrinsic` paths, derives and inventory type remain unchanged.

IDs and `ALL` iteration order are separate contracts. The existing inventory
has 226 positions that differ from numeric order. Both are explicit in the
manifest and checked against the independent frozen fixture from `a80be8ca`.
There is no handwritten enum prefix, fixed prefix length or family-owned tail.

Each row names its metadata owner: `handwritten`, `x86_system` or `x86_query`.
The generator checks both directions: every generated-family declaration needs
an identical registry ID/variant/owner, and every registry member of that family
needs a declaration. It rejects duplicate identities, duplicate family names,
missing members and unknown owners. Global IDs remain contiguous from zero;
family IDs need only be ordered and unique, so families can interleave.

`system-intrinsics.json` owns signatures, names, docs, effects, target policy and
reviewed lowering recipes for 45 existing system intrinsics at IDs 774..818.
`system_intrinsics.py` generates their metadata and classification patterns;
it no longer declares variants or computes global inventory length.

`query-bindings.json` owns equivalent metadata and helper mappings for 42
existing descriptor/status, selector and scalar port-I/O APIs.
`query_bindings.py` generates their tables and const-checks their IDs.
Shared emitters remain handwritten. The four wide LAR/LSL compilation gaps
found during migration are fixed; see [the query binding notes](../../docs/x86-query-bindings.md).

The other 732 intrinsics retain handwritten metadata and lowering. Their
existing enum documentation lives in the registry; generated-family contract
documentation stays in its family schema. Canonical name lookup still uses
`ALL` and `info()`. This cleanup does not migrate signatures or semantics.

Effects and target availability are explicit reviewed policy. They are not
inferred from the encoding catalogue or mnemonic. All current rows are pinned
compiler memory barriers and x86-64-only; a compiler barrier does not promise
hardware fencing. The family schema validates ordered explicit IDs, unique names
and variants, supported scalar types, effects and target domains. Pure entries
cannot claim memory effects. Generic instruction-memory classification and
AArch64 rejection/exhaustive classification consume the generated policy.

To add an intrinsic:

1. Append a row to `intrinsic-registry.json` with the next global ID and the
   next `ALL` position, a unique variant and its metadata owner. Do not reorder
   existing IDs or inventory positions.
2. Add the declaration to the owner's schema with the same ID and variant.
   For `handwritten`, implement the metadata/classification matches in Rust.
   Family membership does not require neighbouring IDs or a generator edit.
3. Select an existing lowering recipe or implement the new behavior. Review
   permission, state, clobber and continuation contracts explicitly.
4. Run `python3 tools/x86-isa/generate.py`. The enum, global inventory and family
   outputs follow their schemas. Update the independent frozen fixture for the
   intentional append, then run generator tests, freshness and affected Rust
   suites. Add independent byte/runtime witnesses as needed.

This migration introduces no instruction, intrinsic ID, runtime permission,
feature requirement or kernel entry/return contract. Existing lowering and
privileged qualification fixtures remain separate.

The [regular system-lowering contract](system-lowering.md) describes generated
bindings for 43 existing intrinsics through four shared recipe families and two
explicit MSR hooks. It records validation, allocation, adjacency and terminal
transfer boundaries.

## Generated structural coverage ledger

`coverage_ledger.py` emits [the x86 coverage ledger](../../docs/x86-coverage/README.md)
and its per-source-form JSON through the same generator and freshness check.
It joins import/exclusion facts, decoder registration, declarative assembler
candidates, the 45 tracked compiler recipes and feature/profile policy.
[Definitions and limits](coverage-ledger.md) explain why candidates, unknown
policy, unknown privilege and untracked validation are separate states.

## AVX-512F/VL and EVEX

`evex.json` adds 1,884 EVEX AVX-512F records and 15 VEX-encoded foundation mask
records. The latter are also present in the original legacy/VEX snapshot.
`evex-source.json` pins the upstream commit, source hash and selection hash.
`generate.py --check-upstream <instructions.json>` checks both selections.
The imported data retains the Zydis MIT licence in `LICENSE-ZYDIS`.

`evex.py` generates a separate form/decode table and 351 `EvexMnemonic` bindings.
It preserves mask policy, encoded length constraints, rounding/SAE, broadcast,
element sizes and displacement tuples. Unknown encoding conditions fail
validation. Feature requirements come from the existing policy and prerequisite
graph. A shared encoder/decoder handles these forms without per-instruction
encoding functions. Registers and both symbolic tag directions are generated
from `registers.json`; existing IDs/tags are unchanged, with ZMM and mask classes
appended. XMM/YMM names now extend through register 31 for EVEX.

Enable `avx512f` explicitly, or `avx512vl` for 128/256-bit VL forms. Positive
feature edits include prerequisite closure. The v1/v2/v3 profiles remain
unchanged; no v4 profile is added. Runtime execution still requires OS-enabled
opmask and ZMM state in XCR0. Target flags do not inspect the host or enable
that state. Existing compiler xstate APIs do not acquire a wider state contract.

Intel and AT&T input accept merge/zero masks, embedded broadcast, rounding and
SAE. For example:

```asm
vaddps zmm31{k7}{z}, zmm20, dword ptr [r13+508]{1to16}
{evex} vaddps xmm0, xmm1, xmm2
```

The first line needs `avx512f`; the second also needs `avx512vl`. Ordinary low
XMM/YMM operations retain their VEX spelling/encoding. `{evex}` explicitly
selects the EVEX adapter, while ZMM, high vector registers, mask destinations
and 512-bit memory select it automatically. The assembler applies the same
promotion to typed programs. Typed callers can also use
`Opcode::Evex(EvexOpcode { kind, options })` directly. Decorators stay in this
copyable opcode payload, so the public `Instruction`/`Register` fields and the
symbolic cache's operand format do not need a second representation.

AT&T memory-width aliases are generated from ambiguous operand shapes. This
covers x/y conversion suffixes and l/q integer-input widths, while preserving
existing VEX parsing. Ambiguous unsized memory is rejected; Intel input needs
an explicit width and AT&T input needs its width suffix. Broadcast counts
disambiguate their own vector length.
`addr32` supports base-free VSIB as well as ordinary 32-bit address registers.
Symbolic displacements retain full relocation fields; only divisible numeric
displacements in range use disp8*N. RIP-relative fixups include any trailing
immediate when computing the relocation addend.

This increment does not add AVX-512 compiler intrinsics, selection, allocation
or runtime qualification. It does not cover BW/DQ/CD, FP16, APX, additional
EVEX opcode maps or a complete v4 profile. Those need their own import/feature
review and witnesses; new operand or tuple kinds require shared-engine work.

## Generated assembly smoke tests

`assembly_smoke.py` generates source witnesses, not expected bytes:

- 2,423 AVX-512F operand shapes, including broadcast alternatives, in both
  dialects. They exercise all imported records through their public bindings.
  Several source records can share a canonical assembly spelling; these are
  representative binding checks, not exhaustive architectural input coverage.
- One compiler IR function for each of the 87 reviewed system/query bindings.
  Original inputs stay live after each operation, results are stored, and
  immediate ports use an explicit constant. The test checks all 36 CPU-level,
  selector, allocator and pool configurations with text/typed/chunked parity.

The AVX-512 test compares both dialects with Clang/GNU and checks decode and
canonical reassembly. VMOVQ's equivalent memory encodings are compared through
complete decoded operand descriptions. Separate literal bytes remain independent
of generation. Further tests cover invalid decorators/reserved prefix bits,
feature gates, width/disp8 boundaries, relocations, address32 and symbolic folding.
No generated privileged or AVX-512 payload executes on the host. The coverage
ledger keeps structural registration separate from test qualification.

Validation at this increment: the full workspace passed 2,727 tests with zero
failures and 13 ignored across 502 groups. Workspace compilation, all 98 ISA
generator tests, both generator freshness checks and comparison with the pinned
full upstream source passed. Logs are `/tmp/keel-evex-workspace.log`,
`/tmp/keel-evex-check.log`, `/tmp/keel-evex-python.log` and
`/tmp/keel-evex-upstream.log`. This is encoding/transport evidence for EVEX;
no native AVX-512 execution qualification is claimed.

GNU AT&T aliases can select fixed identities as well as scalar identities.
`pushf` and `popf` select the existing 64-bit `Pushfq`/`Popfq` forms in long
mode. Explicit word spellings retain their operand-size prefixes; no 32-bit
flags-stack form is enabled in long mode. The mappings live in `att-rules.json`.
