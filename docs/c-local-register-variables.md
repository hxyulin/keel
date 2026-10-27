# Local register variables

On x86-64 Linux, an automatic declaration such as
`register unsigned long value asm("rdx")` binds a direct extended-asm operand
to that hardware register. The variable otherwise uses ordinary C value and
storage lowering. The declaration does not reserve a register or sample its
contents outside an asm operand. This implements the supported
[GNU local register use](https://gcc.gnu.org/onlinedocs/gcc-14.1.0/gcc/Local-Register-Variables.html).

Bindings accept the existing GPR names and aliases, with an optional `%` prefix.
Aliases identify the canonical register; the C type and template modifiers
select the access width. RSP, RBP and high-byte aliases reject. The backend can
also reject a register needed as the active frame base. Types are integers up
to 64 bits or pointers. Floating, vector and aggregate bindings are outside this
increment. Global register declarations retain their separate read-only RSP
contract.

Direct input, output and read/write operands retain their binding through
`r`, `q`, supported register/immediate or register/memory choices and `g`.
A fixed-letter constraint must name the same register. Numeric and named
matching inputs propagate their binding to a generic register output, or check
that an existing output binding agrees. Matching operands still require the
same IR type and width. Early-clobber markers remain intact.

Taking a bound local's address rejects, including explicit memory asm operands.
Pure immediate constraints and incompatible register classes reject. A cast,
arithmetic expression or other expression that is not the local itself does
not carry the binding. The compiler continues to check clobber conflicts,
register overlap, frame reservations and existing stack-call preconditions.

The frontend stores the binding on the local and emits existing fixed-register
IR constraints at the asm statement. Existing operand staging, allocation,
restoration and output stores handle the transfer. No IR schema, backend
operand class, ISA table or public intrinsic ID changes are needed.

`x86_local_register.rs` checks inputs, outputs and read/write operands for all
14 nonreserved GPRs. It checks early-clobbers, named/numeric ties, integer aliases,
pointers, shadowing, volatile accesses and a kernel-shaped RAX/RDX call with an
explicit stack dependency. Hardcoded physical-register reads and writes in the
fixture make incorrect operand placement observable. IR roundtrips and
text/typed/chunked objects agree across 24 selector, allocator, pool and
optimization variants.

All 24 objects pass native GCC and Clang callers. Independent compiler
references cover ordinary register binding; they omit Keel's balanced-stack
call extension. The Clang reference additionally binds the tied output
explicitly. The tested Clang otherwise updates R9 for an input-bound generic
tie but returns RAX without transferring the result. GCC and Keel pass the
input-bound case without that adaptation. Six broken output, read/write and
hidden-call controls return their expected nonzero exits.
