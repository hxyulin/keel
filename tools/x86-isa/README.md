# x86 ISA generation pilot

This pilot imports a pinned subset of Intel XED into opcode dispatch tables.
It proves that Keel can generate native Rust tables from ISA data without
linking a C decoder. It does not decode instruction streams or add x86 support
to `keel disasm`.

Run the offline import report and tests:

```sh
python3 tools/x86-isa/probe.py
python3 -m unittest discover -s tools/x86-isa -p 'test_*.py'
python3 tools/x86-isa/probe.py --emit-rust > /tmp/keel-x86-forms.rs
rustc --crate-type lib /tmp/keel-x86-forms.rs -o /tmp/libkeel_x86_forms.rlib
```

`system-xed.txt` contains 24 unmodified records selected from Intel XED commit
[`0bcb6237345c5066726dcc08b3d87928df3b5b26`](https://github.com/intelxed/xed/tree/0bcb6237345c5066726dcc08b3d87928df3b5b26).
This is also the pin cited by Keel's existing x86 v3 encoding inventory.
The records retain all 34 `PATTERN`/`OPERANDS` pairs. The pilot imports 22 forms,
excludes eight forms explicitly marked `not64`, and reports four unsupported
`MOV_CR`/`MOV_DR` forms. Unknown predicates remain visible in the report;
they never become unconditional matches. The selected forms are a system
instruction sample, not the whole x86-64 baseline or psABI v1 profile.

`source.json` records the selection, pinned URL, full-file SHA-256 and subset
SHA-256. Ordinary runs require no network access. To audit the subset against
an independently downloaded copy of that exact upstream file:

```sh
python3 tools/x86-isa/probe.py --check-upstream /path/to/xed-isa.txt
```

The importer preserves repeated `PATTERN`/`OPERANDS` pairs within a record.
It generates opcode bytes, ModRM domains, group selectors, fixed RM values,
`FORCE64` and the no-REX2 constraint. Original operand expressions and metadata
remain in the report. The generated Rust carries the operand expressions as
strings; typed operand extraction is still future work. `candidates()` only
tests opcode/group selection, including memory-only forms and SWAPGS's fixed
ModRM. It returns all candidates and does not consume bytes.

## Source choice

[Intel XED](https://intelxed.github.io/) provides instruction patterns,
operand expressions, mode conditions and ISA feature layers. Its Apache-2.0
data is already cited in this repository. The grammar includes nonterminals,
state, register-class rules and overrides across files. A complete importer
must resolve those rules; parsing only `xed-isa.txt` does not cover the ISA.
The pilot deliberately rejects the control/debug register predicates instead
of treating them as ordinary memory-addressing forms.

[Zydis's database](https://github.com/zyantific/zydis-db/tree/eaf92a02dca616b90c12b2ebc338d87ed943c84f)
offers explicit JSON filters and operands under MIT. It is another viable
input, but does not remove the need to implement prefix interpretation,
register classes and operand/address-size rules. Its sampled LGDT entry lacks
the CPL0 metadata present in XED. Imported metadata needs a separate semantics
audit whichever source is used.

[iced's definitions](https://github.com/icedland/iced/blob/8cfc5f57c08fd2e80a4cec9d3d9d36f642b2de31/src/csharp/Intel/Generator/Tables/InstructionDefs.txt)
provide MIT instruction descriptions and formatter information. Reusing those
definitions would also require interpreting their generator conventions and
decoder handlers. They remain useful as an independent decoding oracle.

## Next implementation boundary

Keep the byte decoder independent of assembler opcode enums. Add a handwritten,
bounded prefix/REX and ModRM/SIB reader, with generated dispatch and operand
recipes. Enforce the 15-byte instruction limit. Handle operand/address-size
selection, signed displacements and immediates, RIP-relative targets, invalid
prefix combinations and truncation explicitly. VEX/EVEX require their own
dispatch and validation rules; this pilot does not cover them.

Before expanding imports, classify every pattern as supported, excluded by
scope, or unsupported, and audit overlapping candidates. Preserve original
bytes even when formatting chooses an alias. An unsupported x86 instruction
does not have a known length: use an explicit stop or byte-fallback policy,
without claiming the following bytes are synchronized instructions.

The parallel privileged-instruction session owns assembler enums, encoders,
typed IR/codegen and disposable CPL0 tests. This work owns source import,
generation and decoding. No shared enum migration is required for this pilot.
LGDT/LIDT witnesses are `0F 01 /2` and `/3`, memory-only in long mode, with
`FORCE64()` and a ten-byte memory operand. Data generation proves neither
privilege behavior nor compiler semantics. Four control/debug register forms
remain explicit gaps until their grammar and register restrictions are handled.

Intel-derived records and generated tables use Apache-2.0; see `LICENSE-XED`.
