# x86 source dialects

The x86 assembler accepts Intel and AT&T source. Existing assembler APIs and the
CLI keep **Intel as the initial default**. Instructions are never inspected to
guess a dialect. For compiler output that defaults to AT&T without a directive:

```sh
keel asm --target x86_64-unknown-linux-gnu --x86-syntax att input.s -o output.o
```

`--x86-syntax intel` selects the existing default explicitly. This option requires
the x86 assembler target, applies independently to each batch input, and does not
change code generation or typed `Program` instructions.

`.att_syntax [prefix|noprefix]` and `.intel_syntax [prefix|noprefix]` change the
source parser's lexical state, including within a line or between `Program` text
fragments. Without an argument, AT&T uses register prefixes and Intel omits them.
Registers are parsed as tokens; symbol names and strings are preserved.

The library entry point is
`keel_asm::x86::assemble_object_with_syntax_and_features(source, Syntax::Att, features)`.
`keel_asm_ir::x86::Instruction::parse_att` parses a single instruction;
`SourceParser::new(Syntax::Att).parse(source)` parses a program fragment and retains
syntax state for subsequent fragments. `parse_intel` and `parse_program` retain
their Intel defaults. The typed instruction model and its Intel serialization
are unchanged.

AT&T operands use source-before-destination order, `%register`, `$immediate`,
`displacement(base,index,scale)`, `%fs:`/`%gs:`, RIP-relative and existing ELF
relocation modifiers. Indirect `call`/`jmp` require `*`. Scalar suffixes select
widths; extension forms such as `movsbl`, `movzwq`, and `movslq` specify both widths.
SIMD, AVX, BMI and condition operands map into the existing instruction model.
Feature checks still apply. Omitted scalar shift counts mean one; two-operand
SHLD/SHRD aliases use CL. x87 is rejected because the model has no x87 registers
or opcodes.

Unsupported instructions and metadata remain errors. In particular, this change
adds no DWARF, CFI, `.prefalign`, TLS relocation families, or instruction encodings.
Symbolic `movq $symbol,%reg` currently rejects: AT&T requires a signed 32-bit
relocation, while the existing model selects a 64-bit symbolic immediate.
`movabsq` explicitly selects the 64-bit relocation and has different link-time
semantics. Consecutive expression signs also reject because the shared expression
helper cannot yet represent their unary semantics safely. Suffixed NOP and
immediate PUSHW remain explicit model gaps. Addresses currently require 64-bit
base/index registers.

## Testing

`cargo test -p keel-asm --test x86_att` checks Clang bytes,
nonrelaxable ELF relocations, compiler-generated instruction bodies, syntax
switches, diagnostics, malformed forms and Intel/typed equivalence. Clang's
relocation oracle uses `-Wa,-mrelax-relocations=no`, matching the existing encoder's
GOTPCREL relocation family; default Clang GOTPCRELX/REX_GOTPCRELX relaxation is a
separate model gap. Clang 21 rejects AT&T noprefix and Intel prefix, so these two
lexical modes are checked against equivalent Intel/typed instruction streams.
The test executes six ABI/runtime kernels against Clang and C when a local
x86-64 runner is available. On Darwin/Rosetta it embeds unchanged,
relocation-free ELF text in Mach-O wrappers; that path checks instruction and
ABI behavior, not ELF loading or relocation processing.
