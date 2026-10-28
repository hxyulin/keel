# Explicit architectural integer division

`x86_div_u8/u16/u32/u64` and `x86_div_s8/s16/s32/s64` have the
uniform signature `(high: Iw, low: Iw, divisor: Iw) -> (quotient: Iw, remainder: Iw)`.
The dividend is the concatenation `high:low`, exactly 2w bits. Signed forms
interpret this concatenation as two's complement and the divisor as signed w bits;
they truncate the quotient toward zero and give the remainder the dividend's sign.
The high half need not be a sign extension of the low half.

These are baseline x86-64 target intrinsics (Features::NONE, v1, v2, v3).
Each lowers to exactly one principal DIV or IDIV, with a register divisor.
Zero divisor or a quotient outside the destination range raises architectural
#DE at that instruction. No preflight guard, normalized UD2, or helper call is
inserted. Both results are published only after the instruction succeeds.
They are impure pinned memory Barriers, including when both results are dead.
ARM, LLVM, and Cranelift reject them explicitly through their existing target
intrinsic rejection paths. Existing generic division and remainder are unchanged,
including generic signed remainder MIN % -1 = 0.

All three SSA inputs are captured before writing either output, using reserved
RDX, RAX, RCX. For w=8, high:low is packed into AX; quotient is AL and remainder
is extracted by copying AX and shifting, without encoding an AH operand. Scratch
register use is an implementation detail, not a public implicit-register ownership
or signal-resumption ABI. Output upper bits outside Iw are not part of the API.

CF, OF, SF, ZF, AF, and PF are undefined by DIV/IDIV. No flags value is returned
or asserted. Linux native validation records SIGFPE/FPE_INTDIV, saved RIP at the
principal opcode, and defined dividend/divisor input bits; this is a tested Linux
contract, not a promise of universal OS signal delivery or resumption.

Primary source: Intel official *64 and IA-32 Architectures Software Developer's
Manual*, Volume 2A, order 253666-060US, September 2016,
[DIV PDF pages 386–388 and IDIV PDF pages 541–543](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2a-manual.pdf).
The exception and value semantics above follow those instruction entries.

Registry IDs 0–678 are preserved. The eight new IDs are appended at 679–686;
total 687. No frontend or generic InstData division operation is added.
