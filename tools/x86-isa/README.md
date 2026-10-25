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
- `feature-policy.json` maps upstream ISA sets onto Keel feature fields.
  Empty lists mean baseline capabilities; null entries exclude unsupported
  interpretation families. The profile definitions remain handwritten.
- `keel-extensions.json` preserves the existing intentional long-mode D6 trap
  spelling, `udb`, which is absent from this upstream snapshot.

Generated files under `crates/keel-asm-ir/src/x86/isa/` provide the shared
catalogue, decoding index, 93 v2 metadata entries, 1,202 VEX form adapters,
64 fixed encodings and opcode feature queries. The generator also reconciles
encoding fields in `docs/v2-coverage/inventory.json`, retaining its operand and
validation prose. AES entries are optional and are not implied by psABI v2.

The assembler keeps public opcode identities and syntax adapters. Handwritten
code handles branch relaxation, exact alias selection, operand conventions,
canonical choices, addressing and object relocations. It contains no fallback
to the removed instruction-specific encoders. Adding a public instruction
still requires its syntax/API identity and any compiler semantics; importing
an ISA form does not add an intrinsic or make it available for selection.

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
