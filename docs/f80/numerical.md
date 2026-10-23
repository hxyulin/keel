# Single-step x87 numerical operations

`x86_x87_fsin`, `x86_x87_fcos`, and `x86_x87_f2xm1` take one F80.
`x86_x87_fpatan`, `x86_x87_fyl2x`, `x86_x87_fyl2xp1`,
`x86_x87_fscale`, `x86_x87_fprem`, and `x86_x87_fprem1` take two F80s,
ordered as the principal instruction's ST(0), ST(1). Thus FPATAN computes
atan2(second, first), FYL2X computes second * log2(first), and FYL2XP1
computes second * log2(first + 1).

Every operation returns `(F80, i32)`: the hardware result and the raw 16-bit
status word zero-extended to i32. They are pinned barriers even when both
results are unused. Only these intrinsics admit F80 architectural numerical
operations; foreign backends reject them.

The caller must provide an empty x87 data stack, all six exception masks set
in CW, and no pending unmasked exception. The current CW (including PC/RC)
is inherited and preserved. The operation does not clear sticky exceptions
or initialize condition codes. Status bits undefined by the hardware opcode
remain undefined; this API does not canonicalize them.

Both operands are loaded before either output is written. Status is captured
with FNSTSW AX immediately after the principal instruction, before any result
store or stack cleanup. The snapshot includes the temporary physical TOP:
`(entry_TOP - 1) & 7` for unary and popping binary instructions, and
`(entry_TOP - 2) & 7` for nonpopping binary ones. Empty tags do not require
TOP zero: any entry TOP is supported. Cleanup empties the stack and restores
entry TOP. The FSTP of the result can subsequently affect the
architectural status; those effects are excluded from the returned snapshot.
Input FLD effects are included. F80 memory accesses use exactly ten bytes.

FSIN/FCOS range failures expose C2 and the unchanged operand directly.
FPREM/FPREM1 execute once, preserving partial remainder and C2; there is no
completion loop. No libm substitution, software range reduction, or domain
repair is performed. Ordinary F80 arithmetic retains its separate contract.

## Multi-result instructions

`x86_x87_fsincos`, `x86_x87_fptan`, and `x86_x87_fxtract` take one
F80 and return `(F80, F80, i32)`, in immediate post-instruction ST(0),
ST(1), raw-status order. Thus FSINCOS returns cosine then sine, FPTAN
returns one then tangent, and FXTRACT returns significand then exponent.
All existing masked-CW, arbitrary-empty-TOP, pinned-barrier and exact-ten-byte
memory rules above apply. The instruction executes exactly once.

FSINCOS/FPTAN range failure does not push: the first result is the unchanged
operand; the absent second result is unspecified and must not be interpreted
as a numerical result. The implementation duplicates the remaining operand
only after the status snapshot to materialize that unspecified payload.
No numerical equivalence to libm is promised. Hardware-undefined condition
codes and exceptional numerical outputs remain undefined. Cleanup uses the
snapshot's TOP relative to entry TOP, rather than interpreting undefined C2
on exceptional inputs. Two physical results have snapshot TOP `(entry_TOP-2)&7`;
a single physical result has `(entry_TOP-1)&7`. Both paths restore entry TOP
and empty tags, even with no live results. Both result payloads reside on the
x87 stack before writing any home, allowing dying-input/home aliases.

## Exact signed integer-memory arithmetic

`x86_x87_fiadd16/32`, `x86_x87_fimul16/32`, `x86_x87_fisub16/32`,
`x86_x87_fisubr16/32`, `x86_x87_fidiv16/32`, and `x86_x87_fidivr16/32`
are twelve distinct contracts. Each takes F80 first, then exact I16 or I32,
and returns `(F80, i32 raw SW)`. The integer bit pattern is interpreted as
signed by the hardware. FISUB and FIDIV compute F80 minus/divided by integer;
FISUBR and FIDIVR compute integer minus/divided by F80. These baseline v1
instructions inherit the masked PC/RC environment described above.

The compiler captures the integer in an independent reserved stack slot and
loads the F80 before writing either result. It emits exactly one corresponding
FI memory instruction with the declared integer width, followed immediately by
FNSTSW. It then stores or discards the F80 and restores the incoming empty TOP.
Snapshot TOP is `(entry_TOP - 1) & 7`. Integer staging uses exactly two or four
bytes and F80 accesses exactly ten. Both outputs may share dying input homes;
live inputs remain available after the operation. Unused outputs retain the
principal operation and raw architectural effects.

This raw instruction contract differs from FILD followed by portable arithmetic:
operand order, precision-control rounding and exception/status behavior belong
to the exact FI instruction. Ordinary portable Convert retains its existing
semantics. The independent assembly packet in
`crates/keel-codegen/tests/x86_x87_fi_arithmetic` checks same-CPU raw results,
defined status separately from undefined condition-bit diagnostics, CW and stack
balance. Native acceptance and fresh review remain pending until root records
them; the packet does not establish full x87 completion.
