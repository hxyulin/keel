# Raw packed BCD

`x86_x87_fbld(ptr) -> (f80, i32)` reads exactly ten bytes with FBLD.
`x86_x87_fbstp(f80, ptr) -> i32` writes exactly ten bytes with FBSTP.
Pointers need no alignment. The caller supplies accessible ten-byte storage;
no sixteen-byte read/write, padding access, allocation or speculative copy is
part of either contract. Memory faults are those of the principal architectural
operation; a fault does not return a status/result. Overlapping external F80 and
BCD storage is supported by loading the complete SSA F80 before the store.

Both are pinned x86 target intrinsics with Barrier memory effects, including
unused results. All exceptions must be masked with no pending unmasked exception,
and incoming x87 tags must be empty at any physical TOP. CW (including PC/RC)
is inherited and unchanged. FBLD and FBSTP provide hardware signed zero, rounding,
invalid/indefinite and noncanonical digit behavior; there is no decimal validation
or portable saturation. Nondecimal input nibbles have no portable numerical
meaning: the API exposes the current processor's hardware behavior.

Returned i32 is the zero-extended full raw SW captured by FNSTSW immediately
after the principal instruction. FBLD snapshot TOP is entry TOP minus one modulo
eight; FBSTP snapshot TOP is entry TOP. Cleanup follows the snapshot and restores
entry TOP with empty tags, even when every result is unused. Exception sticky
bits include operand preparation and incoming state; undefined condition bits
are exposed unchanged, not normalized. Unmasked operation and occupied incoming
tags are outside this bounded contract.

Native acceptance requires the independent exported Linux x86_64 packet and
fresh review; local ARM compilation does not establish numerical acceptance.
