# Raw x87 binary register arithmetic

Twelve APIs take `(a:F80,b:F80)` and return `(F80,i32 rawSW)`.
Operands always describe pre-instruction ST(0)=a and ST(1)=b: FLD b, FLD a.
No existing raw API covers these principal opcodes. Ordinary generic F80
arithmetic remains unchanged and is not an alias for these pinned barriers.

| Intrinsic suffix (`x86_x87_`) | Intel principal operands | Bytes | Result |
|---|---|---|---|
| fadd | fadd st(0),st(1) | D8 C1 | a+b |
| fmul | fmul st(0),st(1) | D8 C9 | a*b |
| fsub | fsub st(0),st(1) | D8 E1 | a-b |
| fsubr | fsubr st(0),st(1) | D8 E9 | b-a |
| fdiv | fdiv st(0),st(1) | D8 F1 | a/b |
| fdivr | fdivr st(0),st(1) | D8 F9 | b/a |
| faddp | faddp st(1),st(0) | DE C1 | b+a |
| fmulp | fmulp st(1),st(0) | DE C9 | b*a |
| fsubp | fsubp st(1),st(0) | DE E9 | b-a |
| fsubrp | fsubrp st(1),st(0) | DE E1 | a-b |
| fdivp | fdivp st(1),st(0) | DE F9 | b/a |
| fdivrp | fdivrp st(1),st(0) | DE F1 | a/b |

Result means the destination register after the single opcode; algebraic
expressions do not license operand exchange, NaN substitution or reassociation.
The returned i32 is FNSTSW AX immediately after that opcode, zero-extended.
CW (all exceptions masked, PC/RC inherited) is preserved. Entry tags must be
empty, TOP may be arbitrary, and no unmasked exception may be pending.
Snapshot TOP is `(entry_TOP-2)&7` for D8 and `(entry_TOP-1)&7` for DE.
Result storage and cleanup follow the snapshot and restore entry TOP/empty tags.
Both inputs are resident before any output or shared home is written. Memory
accesses are exactly ten bytes. Every instruction executes even with no live
outputs. No libm, completion loop, domain repair or portable arithmetic change.

All six sticky exception bits and structural state are observable. C0/C2/C3
are hardware-undefined. C1 reports rounding direction on valid numerical
outcomes; exceptional/undefined C1 is not canonicalized. Masked invalid results,
NaN propagation, division by zero, overflow and underflow follow hardware.

Primary source: [Intel SDM vol.2A](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf),
FADD/FADDP, FMUL/FMULP, FSUB/FSUBP, FSUBR/FSUBRP,
FDIV/FDIVP and FDIVR/FDIVRP entries. DC destination-ST(1) forms are specified
in [dc.md](dc.md), comparisons in [compare.md](compare.md), BCD in
[bcd.md](bcd.md), and explicit stack leaves in [owned-stack.md](owned-stack.md).
Arbitrary stack-register indices and memory-operand arithmetic are not
supported.
