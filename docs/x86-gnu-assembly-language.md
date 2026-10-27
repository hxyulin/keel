# GNU x86-64 assembly language

The native x86 ELF object path now accepts the six 64-bit language witnesses
from the Linux readiness audit: `.code64`, numeric labels, required macro
arguments, `\@` macro invocation numbers, `.org` and `.pushsection`/`.popsection`.
`.code16`, `.code32` and `.code16gcc` remain explicit errors. This does not add
C inline assembly, asm-goto, instruction families or kernel compiler flags.

The old x86 `Items::parse` called the instruction source parser directly. It
never entered the shared assembly preprocessor used by the AArch64 parser.
The x86 path now feeds GNU statements to that same bounded macro/repetition/
conditional processor. Its lexer handles `#`, `//` and block comments, quoted
strings and semicolon statement separators. Macro definitions and invocation
numbers persist across text fragments within one object. Existing public APIs
and generated instruction data are unchanged.

Numeric labels can repeat and cross section switches. `Nb` resolves to the
latest preceding definition and `Nf` to the next definition in source order.
Undefined references are errors, including references in data directives.
Internal local names do not become external undefined symbols. Numeric tokens
inside strings, integer literals and ordinary symbol names keep their meaning.
Parenthesized linear terms, including `(1b) - .`, preserve the existing
relocation model. A bounded GNU expression parser supplies constant arithmetic,
shifts, bitwise operators, comparisons and logical operators. It follows GNU
precedence and returns -1 for true comparisons. Integer literals and checked
intermediate values stay within the supported 64-bit magnitude; arithmetic
wraparound is not implemented. Expressions allow at most 64 levels of nesting,
512 parser nodes and 64 KiB of source.

The section stack stores section identities, including unique sections.
`.previous` swaps the current and previous section. Temporary section switches
for `.ident` and local common storage preserve that state. GNU supplies the
local-common oracle because Clang changes the previous section in that case.
Subsection-number
syntax is not added.

`.org expression[, fill]` advances the current section's offset. The expression
must resolve at the directive to an absolute value or a value in the current
section; section-name origins and `.` work. Cross-section placement, undefined
values and backward movement are errors. Fill defaults to zero and uses the
low byte of an explicit integer. NOBITS sections grow without file bytes and
ignore fill, matching GNU and Clang. This deliberately rejects GNU's warned
fallback of assuming zero for an undefined `.org` symbol. LLVM accepts some
cross-section origins which GNU rejects; Keel follows GNU's section rule.

Macro parameters accept `:req`; empty or missing required values fail.
`\@` is a zero-based invocation number, shared by nested macro calls.
GNU x86 macros accept omitted optional parameters, defaults, keyword arguments
and comma or whitespace separators. Positional arguments must precede keyword
arguments. `.irp` and ASCII `.irpc` reuse the bounded expansion engine;
`.ifc`/`.ifnc` compare strings and `.purgem` removes a macro definition.
GNU's empty `.irp` runs once with an empty value; LLVM omits that iteration.
Unquoted empty `.irpc` strings are rejected.
GNU macro bodies retain `$number` instruction immediates. Expanded argument
text is scanned again for statements, labels and nested macro calls, including
semicolon-separated alternatives. The existing integer-data fields also accept
GNU `.value`, `.2byte`, `.4byte` and `.8byte` spellings emitted by these macros.

Assembly conditionals and repetition counts can use previously defined absolute
`.set`/`.equ` values, including mutable counters and constant expressions.
Register equality and inequality support the unwind macros' `%rsp == %rsp`
form. Undefined symbols, forward absolute aliases and layout-dependent tests
remain errors at expansion. Register arithmetic, variadic macros, `.altmacro`,
`.ifdef`/`.ifndef`, `.ifb`/`.ifnb` and textual `.include` remain unsupported.
Absolute assignments can be repeated. Redefining a symbolic alias remains an
error. Textual expansion tracks assignments from text fragments; it does not
read preceding structured `Directive::Set` items.
Rebinding an alias still referenced by a deferred relocation, alias or symbol
size is rejected; it requires an explicit value snapshot rather than changing
an earlier use. Grouped constants and unary chains work in instruction operands.
Grouped register arithmetic that the address model cannot represent is
diagnosed instead of becoming a relocation against a register's name.

`.skip`, `.space` and `.zero` accept deferred absolute expressions over label
differences. Linux alternatives can compare the lengths of code in separate
sections: each subtraction must first produce an absolute same-section
difference. Unknown symbols and cross-section subtraction fail. No unresolved
symbol receives a default value. A provisional zero *width* reserves the initial
layout slot; each complete layout evaluates its expression with all labels
available, then repeats layout when any width changes. The existing branch
widening, directive cycle detection and layout work bound include these slots.
Dot captures the directive's position on each pass. Each space directive is
limited to 64 MiB. Existing `.org` evaluation and backward-placement constraints
still apply on every layout pass.

`crates/keel-asm/tests/x86_gnu_language.rs` compares section bytes, sizes, flags,
and normalized ELF relocation types, targets and addends against both Clang
and GNU as. It covers nested section switches, branch relaxation across `.org`,
exception-table section deltas, macro invocations, NOBITS fill and negative
language witnesses. Shared preprocessing tests also compare the new macro
operators against Clang on AArch64. These checks do not establish kernel-build
coverage.

`crates/keel-asm/tests/x86_gnu_expressions.rs` adds GNU and Clang section-byte,
flag, size and normalized relocation comparisons for exception-table iteration,
unwind defaults, register conditionals, mutable counters, grouped symbol terms
and alternatives padding. The empty-iteration difference uses GNU as its oracle.
Negative witnesses cover undefined and cross-section expressions, alias and
layout cycles, expression bounds, missing arguments and rejected legacy modes.
These focused checks do not update the configured corpus acceptance counts.

## Fill and register symbols

`.fill repeat[, size[, value]]` supports defaults, GNU's size clamp to eight
bytes and its low-32-bit little-endian fill pattern. Negative repeats or sizes
emit nothing. Repeat expressions can depend on same-section label differences
and dot, using the existing layout cycle detection and 64 MiB limit. Size and
value must resolve after textual preprocessing. Mutable counters in `.rept`
bodies are captured before layout, including the kernel's fixed-size IDT entries.

Preceding `.set`/`.equ` definitions can bind register names, including chains.
These are lexical operand aliases, so `0(key)` uses the register bound to `key`
without creating an ELF data symbol. Aliases preserve Intel/AT&T prefix modes
and state across chunks. Quoted strings and directives are not operand rewrites.
Forward register aliases, ordinary-symbol-to-register rebinding and ELF symbol
attributes on register aliases remain explicit errors. GNU supplies the Intel
register-symbol oracle because Clang rejects that GNU spelling.

## Binary inputs

`.incbin "name"[, skip[, count]]` inserts binary bytes without alignment.
Library assembly remains free of filesystem access. Call
`x86::assemble_object_with_binary_inputs`,
`x86::assemble_program_object_with_binary_inputs` or
`ProgramAssembler::push_with_binary_inputs` with a resolver callback that returns
bytes for a name. Each name is resolved once and retained across layout passes
and chunks. Existing assembly APIs report that an explicit resolver is needed.
Inactive conditional blocks do not request input.

The x86 `keel asm` and C/IR object-emission CLI paths resolve names using the
process working directory, including absolute paths, as GNU as does. This does
not implement `-I` search paths or textual `.include`. The resolver owns lookup
policy for library users. Filenames must be nonempty UTF-8 without NUL bytes.
Skip/count expressions must be absolute after preprocessing; ranges outside the
input are errors. Output from one directive is limited to 64 MiB.

GNU as treats explicit count zero as the remainder of the file, despite emitting
a warning; Keel matches those bytes and still validates the filename and skip.
This follows the measured GNU behavior, alongside the
[documented binary-input directive](https://sourceware.org/binutils/docs/as/Incbin.html).
GNU comparisons cover section bytes, label offsets, skip/counts and layout;
source, structured, chunked and CLI object paths produce identical payloads.
Embedding externally built legacy-mode code is data packaging and does not add
16/32-bit instruction assembly.
