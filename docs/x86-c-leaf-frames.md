# Native x86-64 C leaf frames

The SysV backend can omit the frame for `cc(c)` SIMD leaves and `cc(fast)`
leaves. Scalar C leaves retain their frames: a controlled LZ4 compression
comparison found that removing those frames regressed the application after
moving unrelated code. The C extension stays limited to SIMD functions.
An eligible function needs no stack storage, incoming stack arguments,
callee-saved registers, special-purpose parameters, TLS addresses, debug files,
or exception personality. Calls, including helper calls for wide division and
software FMA, retain frames. Intrinsics, inline assembly, tail calls and exception
instructions also retain frames. This keeps dynamic stack allocation and frame
observers on the existing frame path.

A frameless leaf leaves `rsp` and `rbp` unchanged. Its unwind rules define the
CFA as `rsp + 8`, with the return address at `CFA - 8`. The assembler still emits
an `.eh_frame` FDE. Wide-vector stack alignment and vector spills retain frames.

Parallel copies use reserved `r11` for integer copies and `xmm14`/`ymm14` when
SIMD locations participate. The SIMD temporary preserves every lane through a
copy cycle. Memory-to-memory transfers use the separate `xmm15`/`ymm15` scratch
register. A function does not reserve a stack copy temporary merely because it
contains ordinary 128-bit vectors. Wide-vector, F80 and intrinsic recipes
keep their existing staging slot conservatively. This does not change which
registers the allocator may use.

`cargo test -p keel-codegen --test x86_c_leaf --test fast_cc` checks frame
eligibility, typed/text object equality, exact leaf CFI and emitted CIE/FDE rules,
and forced register and spill copy cycles at 128 and 256 bits. On Linux x86-64,
the cycle test also links C callers and checks all lanes. The 256-bit runtime
case requires AVX2. These tests do not measure performance or exercise
asynchronous unwinding through a running leaf.
