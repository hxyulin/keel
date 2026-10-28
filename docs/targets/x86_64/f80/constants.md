# Raw architectural x87 constants

`x86_x87_fldpi()`, `x86_x87_fldl2e()`, `x86_x87_fldl2t()`,
`x86_x87_fldlg2()` and `x86_x87_fldln2()` each return `(f80, i32)`.
They execute the named hardware constant instruction under inherited RC and PC.
In particular they round the internal hardware constant according to RC, not
according to PC, without raising precision or reporting rounding in C1. They
are not equivalent to pre-rounded F80 literals. Output payload is the current
processor's architectural constant, with exactly ten bytes of F80 transport
in a sixteen-byte Layout/home.

All are pinned x86 target intrinsics with Barrier effects. They require empty
incoming tags at any TOP, all exceptions masked, and no pending unmasked
exception. FNSTSW immediately follows the constant instruction; the returned
zero-extended raw SW has TOP equal to entry TOP minus one modulo eight. Cleanup
stores or drops the result, restores entry TOP and empty tags, and leaves CW
unchanged. Undefined condition flags remain raw. Incoming sticky exceptions
remain sticky. Unused results do not remove the instruction.

Exact positive one and positive zero already have exact F80 literal recipes:
`fconst.f80 0x3fff,0x8000000000000000` and
`fconst.f80 0x0000,0x0000000000000000`. Their numerical payloads match FLD1 and FLDZ under every RC/PC.
Ordinary literals do not promise the raw SW or pinned environment observation
of a principal FLD1/FLDZ instruction. No new raw intrinsic is added for them.

Native numerical validation on x86-64 Linux has not been completed.
