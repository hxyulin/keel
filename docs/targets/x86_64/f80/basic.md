# Raw x87 basic unary and status operations

`x86_x87_fabs`, `x86_x87_fchs`, `x86_x87_fsqrt`, and
`x86_x87_frndint`: `(F80) -> (F80, i32)`.
`x86_x87_ftst` and `x86_x87_fxam`: `(F80) -> i32`.
The i32 is the immediate FNSTSW AX snapshot, zero-extended without masking.
All six are pinned barriers, including when every output is unused.

An empty x87 stack with arbitrary TOP, all exception masks set, and no pending
unmasked exception is required. CW including PC/RC is inherited and preserved.
FLD m80 occurs before exactly one named principal opcode; its effects are
included. Cleanup follows the snapshot, restores entry TOP and empty tags,
and may change physical status afterward. Snapshot TOP is `(entry_TOP-1)&7`.
Every F80 access is exactly ten bytes. Inputs are loaded before any home write.
Generic F80 arithmetic, optimization and contraction rules are unchanged.

FABS clears sign; FCHS toggles sign; their C1 is zero. FSQRT inherits PC/RC;
FRNDINT inherits RC and leaves infinity unchanged. Their C1 describes rounding;
C0/C2/C3 are undefined for these four unary opcodes. Masked exceptional results
and sticky exceptions follow hardware; no libm replacement is permitted.

FTST compares the loaded value with zero: C3/C2/C0 are 000 greater, 001 less,
100 equal (either zero sign), 111 unordered. NaNs cause masked invalid; C1=0.
FXAM reports the loaded register class in C3/C2/C0: unsupported 000, NaN 001,
normal 010, infinity 011, zero 100, empty 101, denormal 110; C1 is its sign.
This value API always loads an operand, so it cannot observe an empty ST(0).
It does not promise to classify original memory encodings before FLD
semantics. FLD m80 does not raise invalid for SNaNs or denormal exceptions for subnormals (Intel FLD entry, 3-353). FXAM itself raises no FP exceptions; FLD may do so.

Source: [Intel SDM volume 2A](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf),
FABS pp.3-311, FCHS 3-319, FRNDINT 3-373, FSQRT 3-385,
FTST 3-401, FXAM 3-406. Binary, compare, BCD and stack forms are specified
separately ([binary](binary.md), [compare](compare.md), [bcd](bcd.md),
[owned stack](owned-stack.md)).
