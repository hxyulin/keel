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
Existing expression limits still apply: parenthesized symbol terms such as
`(1b)` and deferred `.skip` expressions remain separate gaps found in the
configured Linux corpus.

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
The existing restricted macro grammar still requires the declared argument
count and comma-separated positional calls. Defaults, keyword and variadic
arguments and symbol-dependent conditionals remain unsupported.

`crates/keel-asm/tests/x86_gnu_language.rs` compares section bytes, sizes, flags,
and normalized ELF relocation types, targets and addends against both Clang
and GNU as. It covers nested section switches, branch relaxation across `.org`,
exception-table section deltas, macro invocations, NOBITS fill and negative
language witnesses. Shared preprocessing tests also compare the new macro
operators against Clang on AArch64. These checks do not establish kernel-build
coverage.
