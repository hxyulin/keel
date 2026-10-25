//! Intrinsics: operations with known meaning that behave like calls, written
//! `intrinsic memcpy(v0, v1, v2)`.
//!
//! Pure arithmetic belongs in the instruction set, where the optimizer sees
//! it directly. Intrinsics are for call-like operations (memory functions)
//! and target-specific ones. Everything a pass needs to know about one lives
//! in its [`IntrinsicInfo`]. Reviewed x86 system declarations come from
//! `tools/x86-isa/system-intrinsics.json`; their lowering stays in the backends.

use crate::{Type, VectorType};
use std::fmt;

#[macro_use]
#[path = "generated/x86_system_intrinsics.rs"]
mod generated_system;

#[macro_use]
#[path = "generated/x86_query_intrinsics.rs"]
mod generated_query;

define_intrinsics! {
    X86Sse41Blendvps128,
    X86Sse41Blendvpd128,
    X86Sse41Pblendvb128,
    X86AvxVblendvps128,
    X86AvxVblendvpd128,
    X86AvxVpblendvb128,
    X86AvxVblendvps256,
    X86AvxVblendvpd256,
    X86Avx2Vpblendvb256,

    X86Sse41Mpsadbw128,
    X86AvxVmpsadbw128,
    X86Avx2Vmpsadbw256,
    X86Sse41Phminposuw128,
    X86AvxVphminposuw128,
    X86Sse2Pmaddwd128,
    X86Ssse3Pmaddubsw128,
    X86Sse2Pmulhw128,
    X86Sse2Pmulhuw128,
    X86Ssse3Pmulhrsw128,
    X86Sse41Pmuldq128,
    X86Sse2Pmuludq128,
    X86Sse2Psadbw128,
    X86AvxVpmaddwd128,
    X86AvxVpmaddubsw128,
    X86AvxVpmulhw128,
    X86AvxVpmulhuw128,
    X86AvxVpmulhrsw128,
    X86AvxVpmuldq128,
    X86AvxVpmuludq128,
    X86AvxVpsadbw128,
    X86Avx2Vpmaddwd256,
    X86Avx2Vpmaddubsw256,
    X86Avx2Vpmulhw256,
    X86Avx2Vpmulhuw256,
    X86Avx2Vpmulhrsw256,
    X86Avx2Vpmuldq256,
    X86Avx2Vpmuludq256,
    X86Avx2Vpsadbw256,

    X86Sse2Pslldq128,
    X86Sse2Psrldq128,
    X86AvxVpslldq128,
    X86AvxVpsrldq128,
    X86Avx2Vpslldq256,
    X86Avx2Vpsrldq256,

    // Architectural counts are not reduced modulo lane width.
    X86Sse2Psllw128,
    X86Sse2Pslld128,
    X86Sse2Psllq128,
    X86Sse2Psrlw128,
    X86Sse2Psrld128,
    X86Sse2Psrlq128,
    X86Sse2Psraw128,
    X86Sse2Psrad128,
    X86AvxVpsllw128,
    X86AvxVpslld128,
    X86AvxVpsllq128,
    X86AvxVpsrlw128,
    X86AvxVpsrld128,
    X86AvxVpsrlq128,
    X86AvxVpsraw128,
    X86AvxVpsrad128,
    X86Avx2Vpsllw256,
    X86Avx2Vpslld256,
    X86Avx2Vpsllq256,
    X86Avx2Vpsrlw256,
    X86Avx2Vpsrld256,
    X86Avx2Vpsrlq256,
    X86Avx2Vpsraw256,
    X86Avx2Vpsrad256,
    X86Avx2Vpsllvd128,
    X86Avx2Vpsllvq128,
    X86Avx2Vpsrlvd128,
    X86Avx2Vpsrlvq128,
    X86Avx2Vpsravd128,
    X86Avx2Vpsllvd256,
    X86Avx2Vpsllvq256,
    X86Avx2Vpsrlvd256,
    X86Avx2Vpsrlvq256,
    X86Avx2Vpsravd256,

    // Runtime byte shuffle: signbit zeros, low4bits index within each128-bit half.
    X86Ssse3Pshufb128,
    X86AvxVpshufb128,
    X86Avx2Vpshufb256,

    /// Read XCR0; target CPU/OS must enable XSAVE and OSXSAVE. No state write.
    X86Xgetbv0,
    // Application cache hints and exact-width non-temporal memory operations.
    // Pinned barriers. No implicit hardware fence. See x86_64/cache_stream.rs.
    X86Prefetchnta,
    X86Prefetcht0,
    X86Prefetcht1,
    X86Prefetcht2,
    X86SseMovntps128,
    X86SseMovntpd128,
    X86SseMovntdq128,
    X86AvxVmovntps128,
    X86AvxVmovntps256,
    X86AvxVmovntpd128,
    X86AvxVmovntpd256,
    X86AvxVmovntdq128,
    X86AvxVmovntdq256,
    X86Movnti32,
    X86Movnti64,
    X86Sse41Movntdqa128,
    X86Avx2Vmovntdqa256,

    /// Save an owned, aligned standard xstate image at this compiler boundary.
    X86Xsave64,
    /// Observe SF/ZF/AF/PF/CF as the architectural AH byte at this IR boundary.
    X86Lahf,
    /// Capture old AH flags, then restore input SF/ZF/AF/PF/CF; preserve OF.
    X86SahfExchange,
    /// Owned 512-byte, 16-byte-aligned legacy image at this compiler boundary.
    X86Fxsave64,
    /// Same ownership; empty x87 FTW/TOP0/masked valid CW and valid MXCSR.
    /// Live SIMD SSA survives. See docs/x86-x87-state.md.
    X86Fxrstor64,
    // Explicit raw x87/MMX environment; see docs/x86-x87-state.md.
    X86X87ReadCwNowait,
    X86X87ReadSwNowait,
    X86X87ReadCwWait,
    X86X87ReadSwWait,
    X86X87ExchangeCw,
    X86X87ClexNowait,
    X86X87ClexWait,
    X86X87InitNowait,
    X86X87InitWait,
    X86X87Wait,
    /// Raw signed integer conversion, inherited masked CW/PC/RC, immediate SW.
    /// FIST(P) returns integer-indefinite on invalid; FISTTP requires SSE3.
    X86X87Fist16,
    X86X87Fist32,
    X86X87Fistp16,
    X86X87Fistp32,
    X86X87Fistp64,
    X86X87Fisttp16,
    X86X87Fisttp32,
    X86X87Fisttp64,
    X86X87Fild16,
    X86X87Fild32,
    X86X87Fild64,
    X86X87Fbld,
    X86X87Fbstp,
    X86X87Fldpi,
    X86X87Fldl2e,
    X86X87Fldl2t,
    X86X87Fldlg2,
    X86X87Fldln2,

    /// Exact integer-memory arithmetic: F80 then signed I16/I32; raw immediate SW.
    X86X87Fiadd16,
    X86X87Fiadd32,
    X86X87Fimul16,
    X86X87Fimul32,
    X86X87Fisub16,
    X86X87Fisub32,
    X86X87Fisubr16,
    X86X87Fisubr32,
    X86X87Fidiv16,
    X86X87Fidiv32,
    X86X87Fidivr16,
    X86X87Fidivr32,
    X86X87FaddSt1,
    X86X87FmulSt1,
    X86X87FsubSt1,
    X86X87FsubrSt1,
    X86X87FdivSt1,
    X86X87FdivrSt1,
    X86X87Fadd,
    X86X87Fmul,
    X86X87Fsub,
    X86X87Fsubr,
    X86X87Fdiv,
    X86X87Fdivr,
    X86X87Faddp,
    X86X87Fmulp,
    X86X87Fsubp,
    X86X87Fsubrp,
    X86X87Fdivp,
    X86X87Fdivrp,
    X86X87Fabs,
    X86X87Fchs,
    X86X87Fsqrt,
    X86X87Frndint,
    X86X87Ftst,
    X86X87Fxam,
    X86X87Fsincos,
    X86X87Fptan,
    X86X87Fxtract,
    X86X87Fsin,
    X86X87Fcos,
    X86X87F2xm1,
    X86X87Fpatan,
    X86X87Fyl2x,
    X86X87Fyl2xp1,
    X86X87Fscale,
    X86X87Fprem,
    X86X87Fprem1,

    X86MmxEmms,

    /// Restore compiler-compatible baseline xstate (mask subset of 7).
    /// Requires owned aligned standard image, permitted XCR0 components, and
    /// effective empty x87 stack/TOP=0 with valid masked environment if bit0
    /// is requested. Live SIMD SSA survives; physical state may later change.
    /// Full caller contract: docs/x86-xstate-20261022.json.
    X86Xrstor64,
    // Explicit nondeterministic CPU queries and hardware ordering/hint operations.
    X86Cpuid,
    X86Rdtsc,
    X86Lfence,
    X86Sfence,
    X86Mfence,
    X86Pause,

    /// Clear architectural SIMD state. Live SSA values remain valid; pinned barrier.
    X86AvxVzeroall,
    /// `memcpy(dst, src, len)`: the regions must not overlap.
    Memcpy,
    /// `memmove(dst, src, len)`: the regions may overlap.
    Memmove,
    /// `memset(dst, byte, len)`.
    Memset,
    /// Explicit standard builtin semantics, independent of symbol interposition.
    Memcmp,
    /// Stops in the debugger, which can resume past it. Without a debugger
    /// the process gets SIGTRAP.
    Debugtrap,
    /// `va_start(list)`: initializes the `va_list` at `list` to the first
    /// variadic argument of the current (variadic) function. A `va_list` is
    /// target-defined memory: 8 bytes, align 8, on Apple arm64. Read it with
    /// `va_arg`; copying its bytes copies the position.
    VaStart,
    /// `stack_alloc(size) -> ptr`: `size` bytes of stack, 16-byte aligned,
    /// live until the function returns or a `stack_restore` to a point
    /// before it. For `alloca` and variable-length arrays.
    StackAlloc,
    /// `stack_save() -> ptr`: marks the current extent of the stack, for
    /// `stack_restore`.
    StackSave,
    /// `stack_restore(mark)`: frees every `stack_alloc` since the
    /// `stack_save` that returned `mark`, as a VLA's scope ends.
    StackRestore,
    /// `frame_address() -> ptr`: the address of the function's frame
    /// record (its saved frame pointer and return address), as GNU C's
    /// `__builtin_frame_address(0)`; programs compare it to find how
    /// deep the stack is.
    FrameAddress,
    /// Four IEEE binary16 bit patterns in an i64, expanded to four f32 lanes.
    X86F16cPhToPs128,
    /// Eight binary16 bit patterns in i16x8, expanded to eight f32 lanes.
    X86F16cPhToPs256,
    /// Convert four f32 lanes to binary16 bits in i64. Rounding is constant0..7.
    X86F16cPsToPh128,
    /// Convert eight f32 lanes to binary16 bits in i16x8. Rounding is constant0..7.
    X86F16cPsToPh256,
    /// Explicit bit extraction/deposit; integer flags are not IR results.
    X86Bmi1Bextr32,
    X86Bmi1Bextr64,
    X86Bmi2Pdep32,
    X86Bmi2Pdep64,
    X86Bmi2Pext32,
    X86Bmi2Pext64,
    /// Pinned masked gather: (base, signed indices, passthrough, sign mask, constant scale).
    X86Avx2Vgatherdps128,
    X86Avx2Vgatherdps256,
    X86Avx2Vgatherdpd128,
    X86Avx2Vgatherdpd256,
    X86Avx2Vgatherqps128,
    X86Avx2Vgatherqps256,
    X86Avx2Vgatherqpd128,
    X86Avx2Vgatherqpd256,
    X86Avx2Vpgatherdd128,
    X86Avx2Vpgatherdd256,
    X86Avx2Vpgatherdq128,
    X86Avx2Vpgatherdq256,
    X86Avx2Vpgatherqd128,
    X86Avx2Vpgatherqd256,
    X86Avx2Vpgatherqq128,
    X86Avx2Vpgatherqq256,
    /// Sign-masked memory: inactive load lanes are zero; inactive stores do not access memory.
    X86AvxVmaskmovpsLoad128,
    X86AvxVmaskmovpsStore128,
    X86AvxVmaskmovpsLoad256,
    X86AvxVmaskmovpsStore256,
    X86AvxVmaskmovpdLoad128,
    X86AvxVmaskmovpdStore128,
    X86AvxVmaskmovpdLoad256,
    X86AvxVmaskmovpdStore256,
    X86Avx2VpmaskmovdLoad128,
    X86Avx2VpmaskmovdStore128,
    X86Avx2VpmaskmovdLoad256,
    X86Avx2VpmaskmovdStore256,
    X86Avx2VpmaskmovqLoad128,
    X86Avx2VpmaskmovqStore128,
    X86Avx2VpmaskmovqLoad256,
    X86Avx2VpmaskmovqStore256,
    /// Raw architectural conversions: MXCSR rounding/flags and integer-indefinite on invalid.
    X86AvxVcvtdq2pd128,
    X86AvxVcvtdq2pd256,
    X86AvxVcvtdq2ps128,
    X86AvxVcvtdq2ps256,
    X86AvxVcvtpd2dq128,
    X86AvxVcvtpd2dq256,
    X86AvxVcvtpd2ps128,
    X86AvxVcvtpd2ps256,
    X86AvxVcvtps2dq128,
    X86AvxVcvtps2dq256,
    X86AvxVcvtps2pd128,
    X86AvxVcvtps2pd256,
    X86AvxVcvttpd2dq128,
    X86AvxVcvttpd2dq256,
    X86AvxVcvttps2dq128,
    X86AvxVcvttps2dq256,
    X86AvxVcvtss2si32,
    X86AvxVcvttss2si32,
    X86AvxVcvtss2si64,
    X86AvxVcvttss2si64,
    X86AvxVcvtsd2si32,
    X86AvxVcvttsd2si32,
    X86AvxVcvtsd2si64,
    X86AvxVcvttsd2si64,
    X86AvxVcvtsi2ss32,
    X86AvxVcvtsi2ss64,
    X86AvxVcvtsi2sd32,
    X86AvxVcvtsi2sd64,
    X86AvxVcvtsd2ss,
    X86AvxVcvtss2sd,
    /// Raw32-predicate AVX compares, including quiet/signaling MXCSR effects.
    X86AvxVcmpps128,
    X86AvxVcmpps256,
    X86AvxVcmppd128,
    X86AvxVcmppd256,
    X86AvxVcmpss,
    X86AvxVcmpsd,
    /// Pure bit permutations/masks and tests; VTEST/VPTEST return CF bit0 and ZF bit6 only.
    X86AvxVpermilps128,
    X86AvxVpermilps256,
    X86AvxVpermilpd128,
    X86AvxVpermilpd256,
    X86Avx2Vpermd256,
    X86Avx2Vpermps256,
    X86AvxVmovmskps128,
    X86AvxVmovmskps256,
    X86AvxVmovmskpd128,
    X86AvxVmovmskpd256,
    X86AvxVpmovmskb128,
    X86Avx2Vpmovmskb256,
    X86AvxVtestps128,
    X86AvxVtestps256,
    X86AvxVtestpd128,
    X86AvxVtestpd256,
    X86AvxVptest128,
    X86AvxVptest256,
    /// Architectural AVX FP operations with MXCSR status effects, retained when unused.
    /// ROUND control is a constant 0..15: direction bits0..1, MXCSR bit2,
    /// precision-exception suppression bit3. RCP/RSQRT are hardware approximations
    /// with CPU-dependent result bits. MIN/MAX return the second source on an
    /// unordered comparison or equal zeros. COMI/UCOMI return CF bit0, PF bit2,
    /// ZF bit6 (other bits zero), including their distinct NaN exception behavior.
    /// Exception delivery into an unmasked signal context is outside this API's
    /// validated contract; masked MXCSR status and control behavior are preserved.
    X86AvxVroundps128,
    X86AvxVroundps256,
    X86AvxVroundpd128,
    X86AvxVroundpd256,
    X86AvxVroundss,
    X86AvxVroundsd,
    X86AvxVminps128,
    X86AvxVminps256,
    X86AvxVminpd128,
    X86AvxVminpd256,
    X86AvxVminss,
    X86AvxVminsd,
    X86AvxVmaxps128,
    X86AvxVmaxps256,
    X86AvxVmaxpd128,
    X86AvxVmaxpd256,
    X86AvxVmaxss,
    X86AvxVmaxsd,
    X86AvxVrcpps128,
    X86AvxVrcpps256,
    X86AvxVrcpss,
    X86AvxVrsqrtps128,
    X86AvxVrsqrtps256,
    X86AvxVrsqrtss,
    X86AvxVcomiss,
    X86AvxVucomiss,
    X86AvxVcomisd,
    X86AvxVucomisd,
    /// Non-temporal byte masked store followed by SFENCE. All 16 destination
    /// bytes must be valid writable memory even when mask sign bits are zero;
    /// unlike VMASKMOVPS/PD this API promises no fault suppression.
    X86AvxVmaskmovdqu,
    /// Baseline SSE/SSE2 raw conversions; available on strict x86-64-v1 without AVX.
    X86Ssecvtdq2pd128,
    X86Ssecvtdq2ps128,
    X86Ssecvtpd2dq128,
    X86Ssecvtpd2ps128,
    X86Ssecvtps2dq128,
    X86Ssecvtps2pd128,
    X86Ssecvttpd2dq128,
    X86Ssecvttps2dq128,
    X86Ssecvtss2si32,
    X86Ssecvttss2si32,
    X86Ssecvtss2si64,
    X86Ssecvttss2si64,
    X86Ssecvtsd2si32,
    X86Ssecvttsd2si32,
    X86Ssecvtsd2si64,
    X86Ssecvttsd2si64,
    X86Ssecvtsi2ss32,
    X86Ssecvtsi2ss64,
    X86Ssecvtsi2sd32,
    X86Ssecvtsi2sd64,
    X86Ssecvtsd2ss,
    X86Ssecvtss2sd,
    /// Strict-v1 legacy FP compare0..7, approximation, raw min/max and scalar flags.
    X86SseMinps128,
    X86SseMinpd128,
    X86SseMinss,
    X86SseMinsd,
    X86SseMaxps128,
    X86SseMaxpd128,
    X86SseMaxss,
    X86SseMaxsd,
    X86SseRcpps128,
    X86SseRcpss,
    X86SseRsqrtps128,
    X86SseRsqrtss,
    X86SseComiss,
    X86SseUcomiss,
    X86SseComisd,
    X86SseUcomisd,
    X86SseCmpps128,
    X86SseCmppd128,
    X86SseCmpss,
    X86SseCmpsd,
    /// Raw FP arithmetic retains MXCSR effects even when its value is unused.
    X86SseAddps128,
    X86SseAddpd128,
    X86SseAddss,
    X86SseAddsd,
    X86SseSubps128,
    X86SseSubpd128,
    X86SseSubss,
    X86SseSubsd,
    X86SseMulps128,
    X86SseMulpd128,
    X86SseMulss,
    X86SseMulsd,
    X86SseDivps128,
    X86SseDivpd128,
    X86SseDivss,
    X86SseDivsd,
    X86SseSqrtps128,
    X86SseSqrtpd128,
    X86SseSqrtss,
    X86SseSqrtsd,
    /// Strict-v1 scalar sign masks and ordered byte store (full16 writable).
    X86SseMovmskps128,
    X86SseMovmskpd128,
    X86SsePmovmskb128,
    X86SseMaskmovdqu,
    /// Non-AVX SSE3 horizontal/addsub and SSE4.1 rounding/dot controls with MXCSR effects.
    X86Sse3Haddps,
    X86Sse3Haddpd,
    X86Sse3Hsubps,
    X86Sse3Hsubpd,
    X86Sse3Addsubps,
    X86Sse3Addsubpd,
    X86Sse41Roundps128,
    X86Sse41Roundpd128,
    X86Sse41Roundss,
    X86Sse41Roundsd,
    X86Sse41Dpps,
    X86Sse41Dppd,
    /// SSE4.1 PTEST CF/ZF bits0/6; SSE4.2 reflected Castagnoli CRC32, no final xor.
    X86Sse41Ptest128,
    X86Sse42Crc32b,
    X86Sse42Crc32w,
    X86Sse42Crc32d,
    X86Sse42Crc32q,
    /// Pinned raw architectural MXCSR access; read returns all 32 bits.
    /// Swap returns the old raw bits and loads the supplied bits unchanged.
    /// Caller must supply a value valid for the executing CPU: unsupported or
    /// reserved bits fault as LDMXCSR does; no status/control bits are masked.
    /// Explicit raw FP intrinsics observe program order with these accesses.
    /// Mixing portable FP with the floating environment is unspecified, as for NEON.
    X86SseReadMxcsr,
    /// Swap raw MXCSR, returning the old bits; see the read contract above.
    X86SseSwapMxcsr,
    /// Immutable packed two-i32 I64 values; MXCSR effects are pinned.
    /// No MMX/x87 register or tag-state side effects. CVTPI2PS merges raw upper64.
    X86SseMmxvalueCvtpi2ps,
    X86SseMmxvalueCvtps2pi,
    X86SseMmxvalueCvttps2pi,
    X86SseMmxvalueCvtpi2pd,
    X86SseMmxvalueCvtpd2pi,
    X86SseMmxvalueCvttpd2pi,
    /// Packed strings: constant imm8 controls (bit7 ignored), full128-bit SSA
    /// inputs. Explicit lengths use signed32/64 absolute values clamped by the
    /// byte/word element count. Index, mask and flags are distinct pure results.
    /// Flags return CF/ZF/SF/OF bits0/6/7/11, all remaining result bits zero.
    X86Sse42PcmpistriIndex,
    X86Sse42Pcmpistri64Index,
    X86Sse42PcmpistrmMask,
    X86Sse42PcmpistriFlags,
    X86Sse42PcmpestriIndex,
    X86Sse42PcmpestrmMask,
    X86Sse42PcmpestriFlags,
    X86Sse42Pcmpestri64Index,
    X86Sse42Pcmpestrm64Mask,
    X86Sse42Pcmpestri64Flags,
    /// Raw FMA3, arguments in written dest/source2/source3 order. MXCSR effects
    /// are pinned; scalar results expose only the low scalar value.
    X86FmaVfmadd132pd128,
    X86FmaVfmadd132pd256,
    X86FmaVfmadd132ps128,
    X86FmaVfmadd132ps256,
    X86FmaVfmadd132sd128,
    X86FmaVfmadd132ss128,
    X86FmaVfmadd213pd128,
    X86FmaVfmadd213pd256,
    X86FmaVfmadd213ps128,
    X86FmaVfmadd213ps256,
    X86FmaVfmadd213sd128,
    X86FmaVfmadd213ss128,
    X86FmaVfmadd231pd128,
    X86FmaVfmadd231pd256,
    X86FmaVfmadd231ps128,
    X86FmaVfmadd231ps256,
    X86FmaVfmadd231sd128,
    X86FmaVfmadd231ss128,
    X86FmaVfmaddsub132pd128,
    X86FmaVfmaddsub132pd256,
    X86FmaVfmaddsub213pd128,
    X86FmaVfmaddsub213pd256,
    X86FmaVfmaddsub231pd128,
    X86FmaVfmaddsub231pd256,
    X86FmaVfmaddsub132ps128,
    X86FmaVfmaddsub132ps256,
    X86FmaVfmaddsub213ps128,
    X86FmaVfmaddsub213ps256,
    X86FmaVfmaddsub231ps128,
    X86FmaVfmaddsub231ps256,
    X86FmaVfmsubadd132pd128,
    X86FmaVfmsubadd132pd256,
    X86FmaVfmsubadd213pd128,
    X86FmaVfmsubadd213pd256,
    X86FmaVfmsubadd231pd128,
    X86FmaVfmsubadd231pd256,
    X86FmaVfmsubadd132ps128,
    X86FmaVfmsubadd132ps256,
    X86FmaVfmsubadd213ps128,
    X86FmaVfmsubadd213ps256,
    X86FmaVfmsubadd231ps128,
    X86FmaVfmsubadd231ps256,
    X86FmaVfmsub132pd128,
    X86FmaVfmsub132pd256,
    X86FmaVfmsub132ps128,
    X86FmaVfmsub132ps256,
    X86FmaVfmsub132sd128,
    X86FmaVfmsub132ss128,
    X86FmaVfmsub213pd128,
    X86FmaVfmsub213pd256,
    X86FmaVfmsub213ps128,
    X86FmaVfmsub213ps256,
    X86FmaVfmsub213sd128,
    X86FmaVfmsub213ss128,
    X86FmaVfmsub231pd128,
    X86FmaVfmsub231pd256,
    X86FmaVfmsub231ps128,
    X86FmaVfmsub231ps256,
    X86FmaVfmsub231sd128,
    X86FmaVfmsub231ss128,
    X86FmaVfnmadd132pd128,
    X86FmaVfnmadd132pd256,
    X86FmaVfnmadd132ps128,
    X86FmaVfnmadd132ps256,
    X86FmaVfnmadd132sd128,
    X86FmaVfnmadd132ss128,
    X86FmaVfnmadd213pd128,
    X86FmaVfnmadd213pd256,
    X86FmaVfnmadd213ps128,
    X86FmaVfnmadd213ps256,
    X86FmaVfnmadd213sd128,
    X86FmaVfnmadd213ss128,
    X86FmaVfnmadd231pd128,
    X86FmaVfnmadd231pd256,
    X86FmaVfnmadd231ps128,
    X86FmaVfnmadd231ps256,
    X86FmaVfnmadd231sd128,
    X86FmaVfnmadd231ss128,
    X86FmaVfnmsub132pd128,
    X86FmaVfnmsub132pd256,
    X86FmaVfnmsub132ps128,
    X86FmaVfnmsub132ps256,
    X86FmaVfnmsub132sd128,
    X86FmaVfnmsub132ss128,
    X86FmaVfnmsub213pd128,
    X86FmaVfnmsub213pd256,
    X86FmaVfnmsub213ps128,
    X86FmaVfnmsub213ps256,
    X86FmaVfnmsub213sd128,
    X86FmaVfnmsub213ss128,
    X86FmaVfnmsub231pd128,
    X86FmaVfnmsub231pd256,
    X86FmaVfnmsub231ps128,
    X86FmaVfnmsub231ps256,
    X86FmaVfnmsub231sd128,
    X86FmaVfnmsub231ss128,
    /// Packed strings: constant imm8 controls (bit7 ignored), full128-bit SSA
    /// inputs. Explicit lengths use signed32/64 absolute values clamped by the
    /// byte/word element count. Index, mask and flags are distinct pure results.
    /// Flags return CF/ZF/SF/OF bits0/6/7/11, all remaining result bits zero.
    X86AvxPcmpistriIndex,
    X86AvxPcmpistri64Index,
    X86AvxPcmpistrmMask,
    X86AvxPcmpistriFlags,
    X86AvxPcmpestriIndex,
    X86AvxPcmpestrmMask,
    X86AvxPcmpestriFlags,
    X86AvxPcmpestri64Index,
    X86AvxPcmpestrm64Mask,
    X86AvxPcmpestri64Flags,
    /// Raw FP arithmetic retains MXCSR effects even when its value is unused.
    X86AvxVaddps128,
    X86AvxVaddpd128,
    X86AvxVaddss,
    X86AvxVaddsd,
    X86AvxVaddps256,
    X86AvxVaddpd256,
    X86AvxVsubps128,
    X86AvxVsubpd128,
    X86AvxVsubss,
    X86AvxVsubsd,
    X86AvxVsubps256,
    X86AvxVsubpd256,
    X86AvxVmulps128,
    X86AvxVmulpd128,
    X86AvxVmulss,
    X86AvxVmulsd,
    X86AvxVmulps256,
    X86AvxVmulpd256,
    X86AvxVdivps128,
    X86AvxVdivpd128,
    X86AvxVdivss,
    X86AvxVdivsd,
    X86AvxVdivps256,
    X86AvxVdivpd256,
    X86AvxVsqrtps128,
    X86AvxVsqrtpd128,
    X86AvxVsqrtss,
    X86AvxVsqrtsd,
    X86AvxVsqrtps256,
    X86AvxVsqrtpd256,
    /// Locked seq-cst 16-byte RMW. Address must be 16-aligned and readable/writable
    /// for exactly 16 bytes. CPU/address faults follow hardware. Returns old bits;
    /// success is old == expected. No signal-register context ABI is promised.
    X86Cx16Cmpxchg16b,
    /// Raw AVX horizontal/addsub and dot control operations retain MXCSR effects.
    X86AvxVhaddps128,
    X86AvxVhaddps256,
    X86AvxVhaddpd128,
    X86AvxVhaddpd256,
    X86AvxVhsubps128,
    X86AvxVhsubps256,
    X86AvxVhsubpd128,
    X86AvxVhsubpd256,
    X86AvxVaddsubps128,
    X86AvxVaddsubps256,
    X86AvxVaddsubpd128,
    X86AvxVaddsubpd256,
    X86AvxVdpps128,
    X86AvxVdpps256,
    X86AvxVdppd128,
    X86Ssse3Phaddw128,
    X86Ssse3Phaddd128,
    X86Ssse3Phaddsw128,
    X86Ssse3Phsubw128,
    X86Ssse3Phsubd128,
    X86Ssse3Phsubsw128,
    X86Sse2Paddsb128,
    X86Sse2Paddsw128,
    X86Sse2Paddusb128,
    X86Sse2Paddusw128,
    X86Sse2Psubsb128,
    X86Sse2Psubsw128,
    X86Sse2Psubusb128,
    X86Sse2Psubusw128,
    X86Sse2Pavgb128,
    X86Sse2Pavgw128,
    X86Ssse3Psignb128,
    X86Ssse3Psignw128,
    X86Ssse3Psignd128,
    X86Ssse3Pabsb128,
    X86Ssse3Pabsw128,
    X86Ssse3Pabsd128,
    X86AvxVphaddw128,
    X86AvxVphaddd128,
    X86AvxVphaddsw128,
    X86AvxVphsubw128,
    X86AvxVphsubd128,
    X86AvxVphsubsw128,
    X86AvxVpaddsb128,
    X86AvxVpaddsw128,
    X86AvxVpaddusb128,
    X86AvxVpaddusw128,
    X86AvxVpsubsb128,
    X86AvxVpsubsw128,
    X86AvxVpsubusb128,
    X86AvxVpsubusw128,
    X86AvxVpavgb128,
    X86AvxVpavgw128,
    X86AvxVpsignb128,
    X86AvxVpsignw128,
    X86AvxVpsignd128,
    X86AvxVpabsb128,
    X86AvxVpabsw128,
    X86AvxVpabsd128,
    X86Avx2Vphaddw256,
    X86Avx2Vphaddd256,
    X86Avx2Vphaddsw256,
    X86Avx2Vphsubw256,
    X86Avx2Vphsubd256,
    X86Avx2Vphsubsw256,
    X86Avx2Vpaddsb256,
    X86Avx2Vpaddsw256,
    X86Avx2Vpaddusb256,
    X86Avx2Vpaddusw256,
    X86Avx2Vpsubsb256,
    X86Avx2Vpsubsw256,
    X86Avx2Vpsubusb256,
    X86Avx2Vpsubusw256,
    X86Avx2Vpavgb256,
    X86Avx2Vpavgw256,
    X86Avx2Vpsignb256,
    X86Avx2Vpsignw256,
    X86Avx2Vpsignd256,
    X86Avx2Vpabsb256,
    X86Avx2Vpabsw256,
    X86Avx2Vpabsd256,
    X86Sse2Packsswb128,
    X86Sse2Packssdw128,
    X86Sse2Packuswb128,
    X86Sse41Packusdw128,
    X86AvxVpacksswb128,
    X86AvxVpackssdw128,
    X86AvxVpackuswb128,
    X86AvxVpackusdw128,
    X86Avx2Vpacksswb256,
    X86Avx2Vpackssdw256,
    X86Avx2Vpackuswb256,
    X86Avx2Vpackusdw256,
    X86Clflush,
    X86X87Fcom,
    X86X87Fcomp,
    X86X87Fcompp,
    X86X87Fucom,
    X86X87Fucomp,
    X86X87Fucompp,
    X86X87Fcomi,
    X86X87Fcomip,
    X86X87Fucomi,
    X86X87Fucomip,
    // Owned legacy images; IDs appended to preserve existing discriminants.
    X86X87Fnstenv16,
    X86X87Fnstenv32,
    X86X87Fstenv16,
    X86X87Fstenv32,
    X86X87Fldenv16,
    X86X87Fldenv32,
    X86X87Fnsave16,
    X86X87Fnsave32,
    X86X87Fsave16,
    X86X87Fsave32,
    X86X87Frstor16,
    X86X87Frstor32,
    /// Bounded staged execution; existing 678 discriminants remain unchanged.
    X86X87Region,
    // Raw architectural division; appended after the original 679 IDs.
    X86DivU8,
    X86DivU16,
    X86DivU32,
    X86DivU64,
    X86DivS8,
    X86DivS16,
    X86DivS32,
    X86DivS64,
    // Exact long-mode descriptor/status queries; see docs/x86-descriptor-queries.md.
    X86Sgdt,
    X86Sidt,
    X86SldtMem,
    X86Sldt16,
    X86Sldt32,
    X86Sldt64,
    X86StrMem,
    X86Str16,
    X86Str32,
    X86Str64,
    X86SmswMem,
    X86Smsw16,
    X86Smsw32,
    X86Smsw64,
    // Descriptor-state queries: append-only IDs 701..716.
    X86Lar16,
    X86Lar16Mem,
    X86Lar32,
    X86Lar32Mem,
    X86Lar64,
    X86Lar64Mem,
    X86Lsl16,
    X86Lsl16Mem,
    X86Lsl32,
    X86Lsl32Mem,
    X86Lsl64,
    X86Lsl64Mem,
    X86Verr,
    X86VerrMem,
    X86Verw,
    X86VerwMem,
    X86In8,
    X86In16,
    X86In32,
    X86Out8,
    X86Out16,
    X86Out32,
    X86In8Imm,
    X86In16Imm,
    X86In32Imm,
    X86Out8Imm,
    X86Out16Imm,
    X86Out32Imm,
    /// Actual PUSHFQ image at the emitted instruction boundary, not function entry.
    X86RflagsRead,
    /// Reserved pending a verified owned-FLAGS region; codegen rejects this API.
    X86RflagsSwap,
    /// Permission- and model-conditioned raw EDX:EAX counter read; see docs/x86-rdpmc.md.
    X86Rdpmc,
    // Raw caller-owned physical x87 leaves; append after the 732-ID prefix.
    X86X87Ffree0,
    X86X87Ffree1,
    X86X87Ffree2,
    X86X87Ffree3,
    X86X87Ffree4,
    X86X87Ffree5,
    X86X87Ffree6,
    X86X87Ffree7,
    X86X87Fdecstp,
    X86X87Fincstp,
    X86X87Fnop,
    X86Ins8,
    X86Ins16,
    X86Ins32,
    X86Outs8,
    X86Outs16,
    X86Outs32,
    X86RepIns8,
    X86RepIns16,
    X86RepIns32,
    X86RepOuts8,
    X86RepOuts16,
    X86RepOuts32,
    X86Ins8OwnedTerminal,
    X86Ins16OwnedTerminal,
    X86Ins32OwnedTerminal,
    X86Outs8OwnedTerminal,
    X86Outs16OwnedTerminal,
    X86Outs32OwnedTerminal,
    X86RepIns8OwnedTerminal,
    X86RepIns16OwnedTerminal,
    X86RepIns32OwnedTerminal,
    X86RepOuts8OwnedTerminal,
    X86RepOuts16OwnedTerminal,
    X86RepOuts32OwnedTerminal,
    /// Legacy carryless 64x64 product, selected by imm8 bits 0 and 4.
    X86Pclmulqdq128,
    // Legacy AES-NI: append-only IDs 768..773, after PCLMULQDQ at 767.
    X86Aesenc128 = 768,
    X86Aesenclast128,
    X86Aesdec128,
    X86Aesdeclast128,
    X86Aesimc128,
    X86Aeskeygenassist128,
}

pub struct IntrinsicInfo {
    pub name: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
    /// Pure intrinsics float like pure instructions; the rest are pinned.
    pub pure: bool,
}

impl Intrinsic {
    define_intrinsic_inventory! {
        Intrinsic::X86Sse41Blendvps128,
        Intrinsic::X86Sse41Blendvpd128,
        Intrinsic::X86Sse41Pblendvb128,
        Intrinsic::X86AvxVblendvps128,
        Intrinsic::X86AvxVblendvpd128,
        Intrinsic::X86AvxVpblendvb128,
        Intrinsic::X86AvxVblendvps256,
        Intrinsic::X86AvxVblendvpd256,
        Intrinsic::X86Avx2Vpblendvb256,
        Intrinsic::X86Sse41Mpsadbw128,
        Intrinsic::X86AvxVmpsadbw128,
        Intrinsic::X86Avx2Vmpsadbw256,
        Intrinsic::X86Sse41Phminposuw128,
        Intrinsic::X86AvxVphminposuw128,
        Intrinsic::X86Sse2Pmaddwd128,
        Intrinsic::X86Ssse3Pmaddubsw128,
        Intrinsic::X86Sse2Pmulhw128,
        Intrinsic::X86Sse2Pmulhuw128,
        Intrinsic::X86Ssse3Pmulhrsw128,
        Intrinsic::X86Sse41Pmuldq128,
        Intrinsic::X86Sse2Pmuludq128,
        Intrinsic::X86Sse2Psadbw128,
        Intrinsic::X86AvxVpmaddwd128,
        Intrinsic::X86AvxVpmaddubsw128,
        Intrinsic::X86AvxVpmulhw128,
        Intrinsic::X86AvxVpmulhuw128,
        Intrinsic::X86AvxVpmulhrsw128,
        Intrinsic::X86AvxVpmuldq128,
        Intrinsic::X86AvxVpmuludq128,
        Intrinsic::X86AvxVpsadbw128,
        Intrinsic::X86Avx2Vpmaddwd256,
        Intrinsic::X86Avx2Vpmaddubsw256,
        Intrinsic::X86Avx2Vpmulhw256,
        Intrinsic::X86Avx2Vpmulhuw256,
        Intrinsic::X86Avx2Vpmulhrsw256,
        Intrinsic::X86Avx2Vpmuldq256,
        Intrinsic::X86Avx2Vpmuludq256,
        Intrinsic::X86Avx2Vpsadbw256,
        Intrinsic::X86Sse2Pslldq128,
        Intrinsic::X86Sse2Psrldq128,
        Intrinsic::X86AvxVpslldq128,
        Intrinsic::X86AvxVpsrldq128,
        Intrinsic::X86Avx2Vpslldq256,
        Intrinsic::X86Avx2Vpsrldq256,
        Intrinsic::X86Sse2Psllw128,
        Intrinsic::X86Sse2Pslld128,
        Intrinsic::X86Sse2Psllq128,
        Intrinsic::X86Sse2Psrlw128,
        Intrinsic::X86Sse2Psrld128,
        Intrinsic::X86Sse2Psrlq128,
        Intrinsic::X86Sse2Psraw128,
        Intrinsic::X86Sse2Psrad128,
        Intrinsic::X86AvxVpsllw128,
        Intrinsic::X86AvxVpslld128,
        Intrinsic::X86AvxVpsllq128,
        Intrinsic::X86AvxVpsrlw128,
        Intrinsic::X86AvxVpsrld128,
        Intrinsic::X86AvxVpsrlq128,
        Intrinsic::X86AvxVpsraw128,
        Intrinsic::X86AvxVpsrad128,
        Intrinsic::X86Avx2Vpsllw256,
        Intrinsic::X86Avx2Vpslld256,
        Intrinsic::X86Avx2Vpsllq256,
        Intrinsic::X86Avx2Vpsrlw256,
        Intrinsic::X86Avx2Vpsrld256,
        Intrinsic::X86Avx2Vpsrlq256,
        Intrinsic::X86Avx2Vpsraw256,
        Intrinsic::X86Avx2Vpsrad256,
        Intrinsic::X86Avx2Vpsllvd128,
        Intrinsic::X86Avx2Vpsllvq128,
        Intrinsic::X86Avx2Vpsrlvd128,
        Intrinsic::X86Avx2Vpsrlvq128,
        Intrinsic::X86Avx2Vpsravd128,
        Intrinsic::X86Avx2Vpsllvd256,
        Intrinsic::X86Avx2Vpsllvq256,
        Intrinsic::X86Avx2Vpsrlvd256,
        Intrinsic::X86Avx2Vpsrlvq256,
        Intrinsic::X86Avx2Vpsravd256,
        Intrinsic::X86Ssse3Pshufb128,
        Intrinsic::X86AvxVpshufb128,
        Intrinsic::X86Avx2Vpshufb256,
        Intrinsic::X86Xgetbv0,
        Intrinsic::X86Prefetchnta,
        Intrinsic::X86Prefetcht0,
        Intrinsic::X86Prefetcht1,
        Intrinsic::X86Prefetcht2,
        Intrinsic::X86SseMovntps128,
        Intrinsic::X86SseMovntpd128,
        Intrinsic::X86SseMovntdq128,
        Intrinsic::X86AvxVmovntps128,
        Intrinsic::X86AvxVmovntps256,
        Intrinsic::X86AvxVmovntpd128,
        Intrinsic::X86AvxVmovntpd256,
        Intrinsic::X86AvxVmovntdq128,
        Intrinsic::X86AvxVmovntdq256,
        Intrinsic::X86Movnti32,
        Intrinsic::X86Movnti64,
        Intrinsic::X86Sse41Movntdqa128,
        Intrinsic::X86Avx2Vmovntdqa256,
        Intrinsic::X86Xsave64,
        Intrinsic::X86Lahf,
        Intrinsic::X86SahfExchange,
        Intrinsic::X86Fxsave64,
        Intrinsic::X86Fxrstor64,
        Intrinsic::X86X87ReadCwNowait,
        Intrinsic::X86X87ReadSwNowait,
        Intrinsic::X86X87ReadCwWait,
        Intrinsic::X86X87ReadSwWait,
        Intrinsic::X86X87ExchangeCw,
        Intrinsic::X86X87ClexNowait,
        Intrinsic::X86X87ClexWait,
        Intrinsic::X86X87InitNowait,
        Intrinsic::X86X87InitWait,
        Intrinsic::X86X87Wait,
        Intrinsic::X86X87Fist16,
        Intrinsic::X86X87Fist32,
        Intrinsic::X86X87Fistp16,
        Intrinsic::X86X87Fistp32,
        Intrinsic::X86X87Fistp64,
        Intrinsic::X86X87Fisttp16,
        Intrinsic::X86X87Fisttp32,
        Intrinsic::X86X87Fisttp64,
        Intrinsic::X86X87Fild16,
        Intrinsic::X86X87Fild32,
        Intrinsic::X86X87Fild64,
        Intrinsic::X86X87Fbld,
        Intrinsic::X86X87Fbstp,
        Intrinsic::X86X87Fldpi,
        Intrinsic::X86X87Fldl2e,
        Intrinsic::X86X87Fldl2t,
        Intrinsic::X86X87Fldlg2,
        Intrinsic::X86X87Fldln2,
        Intrinsic::X86X87Fiadd16,
        Intrinsic::X86X87Fiadd32,
        Intrinsic::X86X87Fimul16,
        Intrinsic::X86X87Fimul32,
        Intrinsic::X86X87Fisub16,
        Intrinsic::X86X87Fisub32,
        Intrinsic::X86X87Fisubr16,
        Intrinsic::X86X87Fisubr32,
        Intrinsic::X86X87Fidiv16,
        Intrinsic::X86X87Fidiv32,
        Intrinsic::X86X87Fidivr16,
        Intrinsic::X86X87Fidivr32,
        Intrinsic::X86X87FaddSt1,
        Intrinsic::X86X87FmulSt1,
        Intrinsic::X86X87FsubSt1,
        Intrinsic::X86X87FsubrSt1,
        Intrinsic::X86X87FdivSt1,
        Intrinsic::X86X87FdivrSt1,
        Intrinsic::X86X87Fadd,
        Intrinsic::X86X87Fmul,
        Intrinsic::X86X87Fsub,
        Intrinsic::X86X87Fsubr,
        Intrinsic::X86X87Fdiv,
        Intrinsic::X86X87Fdivr,
        Intrinsic::X86X87Faddp,
        Intrinsic::X86X87Fmulp,
        Intrinsic::X86X87Fsubp,
        Intrinsic::X86X87Fsubrp,
        Intrinsic::X86X87Fdivp,
        Intrinsic::X86X87Fdivrp,
        Intrinsic::X86X87Fabs,
        Intrinsic::X86X87Fchs,
        Intrinsic::X86X87Fsqrt,
        Intrinsic::X86X87Frndint,
        Intrinsic::X86X87Ftst,
        Intrinsic::X86X87Fxam,
        Intrinsic::X86X87Fsincos,
        Intrinsic::X86X87Fptan,
        Intrinsic::X86X87Fxtract,
        Intrinsic::X86X87Fsin,
        Intrinsic::X86X87Fcos,
        Intrinsic::X86X87F2xm1,
        Intrinsic::X86X87Fpatan,
        Intrinsic::X86X87Fyl2x,
        Intrinsic::X86X87Fyl2xp1,
        Intrinsic::X86X87Fscale,
        Intrinsic::X86X87Fprem,
        Intrinsic::X86X87Fprem1,
        Intrinsic::X86MmxEmms,
        Intrinsic::X86Xrstor64,
        Intrinsic::X86Cpuid,
        Intrinsic::X86Rdtsc,
        Intrinsic::X86Lfence,
        Intrinsic::X86Sfence,
        Intrinsic::X86Mfence,
        Intrinsic::X86Pause,
        Intrinsic::X86AvxVzeroall,
        Intrinsic::X86SseReadMxcsr,
        Intrinsic::X86SseSwapMxcsr,
        Intrinsic::X86SseMmxvalueCvtpi2ps,
        Intrinsic::X86SseMmxvalueCvtps2pi,
        Intrinsic::X86SseMmxvalueCvttps2pi,
        Intrinsic::X86SseMmxvalueCvtpi2pd,
        Intrinsic::X86SseMmxvalueCvtpd2pi,
        Intrinsic::X86SseMmxvalueCvttpd2pi,
        Intrinsic::Memcpy,
        Intrinsic::Memmove,
        Intrinsic::Memset,
        Intrinsic::Memcmp,
        Intrinsic::Debugtrap,
        Intrinsic::VaStart,
        Intrinsic::StackAlloc,
        Intrinsic::StackSave,
        Intrinsic::StackRestore,
        Intrinsic::FrameAddress,
        Intrinsic::X86F16cPhToPs128,
        Intrinsic::X86F16cPhToPs256,
        Intrinsic::X86F16cPsToPh128,
        Intrinsic::X86F16cPsToPh256,
        Intrinsic::X86Bmi1Bextr32,
        Intrinsic::X86Bmi1Bextr64,
        Intrinsic::X86Bmi2Pdep32,
        Intrinsic::X86Bmi2Pdep64,
        Intrinsic::X86Bmi2Pext32,
        Intrinsic::X86Bmi2Pext64,
        Intrinsic::X86Avx2Vgatherdps128,
        Intrinsic::X86Avx2Vgatherdps256,
        Intrinsic::X86Avx2Vgatherdpd128,
        Intrinsic::X86Avx2Vgatherdpd256,
        Intrinsic::X86Avx2Vgatherqps128,
        Intrinsic::X86Avx2Vgatherqps256,
        Intrinsic::X86Avx2Vgatherqpd128,
        Intrinsic::X86Avx2Vgatherqpd256,
        Intrinsic::X86Avx2Vpgatherdd128,
        Intrinsic::X86Avx2Vpgatherdd256,
        Intrinsic::X86Avx2Vpgatherdq128,
        Intrinsic::X86Avx2Vpgatherdq256,
        Intrinsic::X86Avx2Vpgatherqd128,
        Intrinsic::X86Avx2Vpgatherqd256,
        Intrinsic::X86Avx2Vpgatherqq128,
        Intrinsic::X86Avx2Vpgatherqq256,
        Intrinsic::X86AvxVmaskmovpsLoad128,
        Intrinsic::X86AvxVmaskmovpsStore128,
        Intrinsic::X86AvxVmaskmovpsLoad256,
        Intrinsic::X86AvxVmaskmovpsStore256,
        Intrinsic::X86AvxVmaskmovpdLoad128,
        Intrinsic::X86AvxVmaskmovpdStore128,
        Intrinsic::X86AvxVmaskmovpdLoad256,
        Intrinsic::X86AvxVmaskmovpdStore256,
        Intrinsic::X86Avx2VpmaskmovdLoad128,
        Intrinsic::X86Avx2VpmaskmovdStore128,
        Intrinsic::X86Avx2VpmaskmovdLoad256,
        Intrinsic::X86Avx2VpmaskmovdStore256,
        Intrinsic::X86Avx2VpmaskmovqLoad128,
        Intrinsic::X86Avx2VpmaskmovqStore128,
        Intrinsic::X86Avx2VpmaskmovqLoad256,
        Intrinsic::X86Avx2VpmaskmovqStore256,
        Intrinsic::X86AvxVcvtdq2pd128,
        Intrinsic::X86AvxVcvtdq2pd256,
        Intrinsic::X86AvxVcvtdq2ps128,
        Intrinsic::X86AvxVcvtdq2ps256,
        Intrinsic::X86AvxVcvtpd2dq128,
        Intrinsic::X86AvxVcvtpd2dq256,
        Intrinsic::X86AvxVcvtpd2ps128,
        Intrinsic::X86AvxVcvtpd2ps256,
        Intrinsic::X86AvxVcvtps2dq128,
        Intrinsic::X86AvxVcvtps2dq256,
        Intrinsic::X86AvxVcvtps2pd128,
        Intrinsic::X86AvxVcvtps2pd256,
        Intrinsic::X86AvxVcvttpd2dq128,
        Intrinsic::X86AvxVcvttpd2dq256,
        Intrinsic::X86AvxVcvttps2dq128,
        Intrinsic::X86AvxVcvttps2dq256,
        Intrinsic::X86AvxVcvtss2si32,
        Intrinsic::X86AvxVcvttss2si32,
        Intrinsic::X86AvxVcvtss2si64,
        Intrinsic::X86AvxVcvttss2si64,
        Intrinsic::X86AvxVcvtsd2si32,
        Intrinsic::X86AvxVcvttsd2si32,
        Intrinsic::X86AvxVcvtsd2si64,
        Intrinsic::X86AvxVcvttsd2si64,
        Intrinsic::X86AvxVcvtsi2ss32,
        Intrinsic::X86AvxVcvtsi2ss64,
        Intrinsic::X86AvxVcvtsi2sd32,
        Intrinsic::X86AvxVcvtsi2sd64,
        Intrinsic::X86AvxVcvtsd2ss,
        Intrinsic::X86AvxVcvtss2sd,
        Intrinsic::X86AvxVcmpps128,
        Intrinsic::X86AvxVcmpps256,
        Intrinsic::X86AvxVcmppd128,
        Intrinsic::X86AvxVcmppd256,
        Intrinsic::X86AvxVcmpss,
        Intrinsic::X86AvxVcmpsd,
        Intrinsic::X86AvxVpermilps128,
        Intrinsic::X86AvxVpermilps256,
        Intrinsic::X86AvxVpermilpd128,
        Intrinsic::X86AvxVpermilpd256,
        Intrinsic::X86Avx2Vpermd256,
        Intrinsic::X86Avx2Vpermps256,
        Intrinsic::X86AvxVmovmskps128,
        Intrinsic::X86AvxVmovmskps256,
        Intrinsic::X86AvxVmovmskpd128,
        Intrinsic::X86AvxVmovmskpd256,
        Intrinsic::X86AvxVpmovmskb128,
        Intrinsic::X86Avx2Vpmovmskb256,
        Intrinsic::X86AvxVtestps128,
        Intrinsic::X86AvxVtestps256,
        Intrinsic::X86AvxVtestpd128,
        Intrinsic::X86AvxVtestpd256,
        Intrinsic::X86AvxVptest128,
        Intrinsic::X86AvxVptest256,
        Intrinsic::X86AvxVroundps128,
        Intrinsic::X86AvxVroundps256,
        Intrinsic::X86AvxVroundpd128,
        Intrinsic::X86AvxVroundpd256,
        Intrinsic::X86AvxVroundss,
        Intrinsic::X86AvxVroundsd,
        Intrinsic::X86AvxVminps128,
        Intrinsic::X86AvxVminps256,
        Intrinsic::X86AvxVminpd128,
        Intrinsic::X86AvxVminpd256,
        Intrinsic::X86AvxVminss,
        Intrinsic::X86AvxVminsd,
        Intrinsic::X86AvxVmaxps128,
        Intrinsic::X86AvxVmaxps256,
        Intrinsic::X86AvxVmaxpd128,
        Intrinsic::X86AvxVmaxpd256,
        Intrinsic::X86AvxVmaxss,
        Intrinsic::X86AvxVmaxsd,
        Intrinsic::X86AvxVrcpps128,
        Intrinsic::X86AvxVrcpps256,
        Intrinsic::X86AvxVrcpss,
        Intrinsic::X86AvxVrsqrtps128,
        Intrinsic::X86AvxVrsqrtps256,
        Intrinsic::X86AvxVrsqrtss,
        Intrinsic::X86AvxVcomiss,
        Intrinsic::X86AvxVucomiss,
        Intrinsic::X86AvxVcomisd,
        Intrinsic::X86AvxVucomisd,
        Intrinsic::X86AvxVmaskmovdqu,
        Intrinsic::X86Ssecvtdq2pd128,
        Intrinsic::X86Ssecvtdq2ps128,
        Intrinsic::X86Ssecvtpd2dq128,
        Intrinsic::X86Ssecvtpd2ps128,
        Intrinsic::X86Ssecvtps2dq128,
        Intrinsic::X86Ssecvtps2pd128,
        Intrinsic::X86Ssecvttpd2dq128,
        Intrinsic::X86Ssecvttps2dq128,
        Intrinsic::X86Ssecvtss2si32,
        Intrinsic::X86Ssecvttss2si32,
        Intrinsic::X86Ssecvtss2si64,
        Intrinsic::X86Ssecvttss2si64,
        Intrinsic::X86Ssecvtsd2si32,
        Intrinsic::X86Ssecvttsd2si32,
        Intrinsic::X86Ssecvtsd2si64,
        Intrinsic::X86Ssecvttsd2si64,
        Intrinsic::X86Ssecvtsi2ss32,
        Intrinsic::X86Ssecvtsi2ss64,
        Intrinsic::X86Ssecvtsi2sd32,
        Intrinsic::X86Ssecvtsi2sd64,
        Intrinsic::X86Ssecvtsd2ss,
        Intrinsic::X86Ssecvtss2sd,
        Intrinsic::X86SseMinps128,
        Intrinsic::X86SseMinpd128,
        Intrinsic::X86SseMinss,
        Intrinsic::X86SseMinsd,
        Intrinsic::X86SseMaxps128,
        Intrinsic::X86SseMaxpd128,
        Intrinsic::X86SseMaxss,
        Intrinsic::X86SseMaxsd,
        Intrinsic::X86SseRcpps128,
        Intrinsic::X86SseRcpss,
        Intrinsic::X86SseRsqrtps128,
        Intrinsic::X86SseRsqrtss,
        Intrinsic::X86SseComiss,
        Intrinsic::X86SseUcomiss,
        Intrinsic::X86SseComisd,
        Intrinsic::X86SseUcomisd,
        Intrinsic::X86SseCmpps128,
        Intrinsic::X86SseCmppd128,
        Intrinsic::X86SseCmpss,
        Intrinsic::X86SseCmpsd,
        Intrinsic::X86SseAddps128,
        Intrinsic::X86SseAddpd128,
        Intrinsic::X86SseAddss,
        Intrinsic::X86SseAddsd,
        Intrinsic::X86SseSubps128,
        Intrinsic::X86SseSubpd128,
        Intrinsic::X86SseSubss,
        Intrinsic::X86SseSubsd,
        Intrinsic::X86SseMulps128,
        Intrinsic::X86SseMulpd128,
        Intrinsic::X86SseMulss,
        Intrinsic::X86SseMulsd,
        Intrinsic::X86SseDivps128,
        Intrinsic::X86SseDivpd128,
        Intrinsic::X86SseDivss,
        Intrinsic::X86SseDivsd,
        Intrinsic::X86SseSqrtps128,
        Intrinsic::X86SseSqrtpd128,
        Intrinsic::X86SseSqrtss,
        Intrinsic::X86SseSqrtsd,
        Intrinsic::X86SseMovmskps128,
        Intrinsic::X86SseMovmskpd128,
        Intrinsic::X86SsePmovmskb128,
        Intrinsic::X86SseMaskmovdqu,
        Intrinsic::X86Sse3Haddps,
        Intrinsic::X86Sse3Haddpd,
        Intrinsic::X86Sse3Hsubps,
        Intrinsic::X86Sse3Hsubpd,
        Intrinsic::X86Sse3Addsubps,
        Intrinsic::X86Sse3Addsubpd,
        Intrinsic::X86Sse41Roundps128,
        Intrinsic::X86Sse41Roundpd128,
        Intrinsic::X86Sse41Roundss,
        Intrinsic::X86Sse41Roundsd,
        Intrinsic::X86Sse41Dpps,
        Intrinsic::X86Sse41Dppd,
        Intrinsic::X86Sse41Ptest128,
        Intrinsic::X86Sse42Crc32b,
        Intrinsic::X86Sse42Crc32w,
        Intrinsic::X86Sse42Crc32d,
        Intrinsic::X86Sse42Crc32q,
        Intrinsic::X86Sse42PcmpistriIndex,
        Intrinsic::X86Sse42Pcmpistri64Index,
        Intrinsic::X86Sse42PcmpistrmMask,
        Intrinsic::X86Sse42PcmpistriFlags,
        Intrinsic::X86Sse42PcmpestriIndex,
        Intrinsic::X86Sse42PcmpestrmMask,
        Intrinsic::X86Sse42PcmpestriFlags,
        Intrinsic::X86Sse42Pcmpestri64Index,
        Intrinsic::X86Sse42Pcmpestrm64Mask,
        Intrinsic::X86Sse42Pcmpestri64Flags,
        Intrinsic::X86FmaVfmadd132pd128,
        Intrinsic::X86FmaVfmadd132pd256,
        Intrinsic::X86FmaVfmadd132ps128,
        Intrinsic::X86FmaVfmadd132ps256,
        Intrinsic::X86FmaVfmadd132sd128,
        Intrinsic::X86FmaVfmadd132ss128,
        Intrinsic::X86FmaVfmadd213pd128,
        Intrinsic::X86FmaVfmadd213pd256,
        Intrinsic::X86FmaVfmadd213ps128,
        Intrinsic::X86FmaVfmadd213ps256,
        Intrinsic::X86FmaVfmadd213sd128,
        Intrinsic::X86FmaVfmadd213ss128,
        Intrinsic::X86FmaVfmadd231pd128,
        Intrinsic::X86FmaVfmadd231pd256,
        Intrinsic::X86FmaVfmadd231ps128,
        Intrinsic::X86FmaVfmadd231ps256,
        Intrinsic::X86FmaVfmadd231sd128,
        Intrinsic::X86FmaVfmadd231ss128,
        Intrinsic::X86FmaVfmaddsub132pd128,
        Intrinsic::X86FmaVfmaddsub132pd256,
        Intrinsic::X86FmaVfmaddsub213pd128,
        Intrinsic::X86FmaVfmaddsub213pd256,
        Intrinsic::X86FmaVfmaddsub231pd128,
        Intrinsic::X86FmaVfmaddsub231pd256,
        Intrinsic::X86FmaVfmaddsub132ps128,
        Intrinsic::X86FmaVfmaddsub132ps256,
        Intrinsic::X86FmaVfmaddsub213ps128,
        Intrinsic::X86FmaVfmaddsub213ps256,
        Intrinsic::X86FmaVfmaddsub231ps128,
        Intrinsic::X86FmaVfmaddsub231ps256,
        Intrinsic::X86FmaVfmsubadd132pd128,
        Intrinsic::X86FmaVfmsubadd132pd256,
        Intrinsic::X86FmaVfmsubadd213pd128,
        Intrinsic::X86FmaVfmsubadd213pd256,
        Intrinsic::X86FmaVfmsubadd231pd128,
        Intrinsic::X86FmaVfmsubadd231pd256,
        Intrinsic::X86FmaVfmsubadd132ps128,
        Intrinsic::X86FmaVfmsubadd132ps256,
        Intrinsic::X86FmaVfmsubadd213ps128,
        Intrinsic::X86FmaVfmsubadd213ps256,
        Intrinsic::X86FmaVfmsubadd231ps128,
        Intrinsic::X86FmaVfmsubadd231ps256,
        Intrinsic::X86FmaVfmsub132pd128,
        Intrinsic::X86FmaVfmsub132pd256,
        Intrinsic::X86FmaVfmsub132ps128,
        Intrinsic::X86FmaVfmsub132ps256,
        Intrinsic::X86FmaVfmsub132sd128,
        Intrinsic::X86FmaVfmsub132ss128,
        Intrinsic::X86FmaVfmsub213pd128,
        Intrinsic::X86FmaVfmsub213pd256,
        Intrinsic::X86FmaVfmsub213ps128,
        Intrinsic::X86FmaVfmsub213ps256,
        Intrinsic::X86FmaVfmsub213sd128,
        Intrinsic::X86FmaVfmsub213ss128,
        Intrinsic::X86FmaVfmsub231pd128,
        Intrinsic::X86FmaVfmsub231pd256,
        Intrinsic::X86FmaVfmsub231ps128,
        Intrinsic::X86FmaVfmsub231ps256,
        Intrinsic::X86FmaVfmsub231sd128,
        Intrinsic::X86FmaVfmsub231ss128,
        Intrinsic::X86FmaVfnmadd132pd128,
        Intrinsic::X86FmaVfnmadd132pd256,
        Intrinsic::X86FmaVfnmadd132ps128,
        Intrinsic::X86FmaVfnmadd132ps256,
        Intrinsic::X86FmaVfnmadd132sd128,
        Intrinsic::X86FmaVfnmadd132ss128,
        Intrinsic::X86FmaVfnmadd213pd128,
        Intrinsic::X86FmaVfnmadd213pd256,
        Intrinsic::X86FmaVfnmadd213ps128,
        Intrinsic::X86FmaVfnmadd213ps256,
        Intrinsic::X86FmaVfnmadd213sd128,
        Intrinsic::X86FmaVfnmadd213ss128,
        Intrinsic::X86FmaVfnmadd231pd128,
        Intrinsic::X86FmaVfnmadd231pd256,
        Intrinsic::X86FmaVfnmadd231ps128,
        Intrinsic::X86FmaVfnmadd231ps256,
        Intrinsic::X86FmaVfnmadd231sd128,
        Intrinsic::X86FmaVfnmadd231ss128,
        Intrinsic::X86FmaVfnmsub132pd128,
        Intrinsic::X86FmaVfnmsub132pd256,
        Intrinsic::X86FmaVfnmsub132ps128,
        Intrinsic::X86FmaVfnmsub132ps256,
        Intrinsic::X86FmaVfnmsub132sd128,
        Intrinsic::X86FmaVfnmsub132ss128,
        Intrinsic::X86FmaVfnmsub213pd128,
        Intrinsic::X86FmaVfnmsub213pd256,
        Intrinsic::X86FmaVfnmsub213ps128,
        Intrinsic::X86FmaVfnmsub213ps256,
        Intrinsic::X86FmaVfnmsub213sd128,
        Intrinsic::X86FmaVfnmsub213ss128,
        Intrinsic::X86FmaVfnmsub231pd128,
        Intrinsic::X86FmaVfnmsub231pd256,
        Intrinsic::X86FmaVfnmsub231ps128,
        Intrinsic::X86FmaVfnmsub231ps256,
        Intrinsic::X86FmaVfnmsub231sd128,
        Intrinsic::X86FmaVfnmsub231ss128,
        Intrinsic::X86AvxPcmpistriIndex,
        Intrinsic::X86AvxPcmpistri64Index,
        Intrinsic::X86AvxPcmpistrmMask,
        Intrinsic::X86AvxPcmpistriFlags,
        Intrinsic::X86AvxPcmpestriIndex,
        Intrinsic::X86AvxPcmpestrmMask,
        Intrinsic::X86AvxPcmpestriFlags,
        Intrinsic::X86AvxPcmpestri64Index,
        Intrinsic::X86AvxPcmpestrm64Mask,
        Intrinsic::X86AvxPcmpestri64Flags,
        Intrinsic::X86AvxVaddps128,
        Intrinsic::X86AvxVaddpd128,
        Intrinsic::X86AvxVaddss,
        Intrinsic::X86AvxVaddsd,
        Intrinsic::X86AvxVaddps256,
        Intrinsic::X86AvxVaddpd256,
        Intrinsic::X86AvxVsubps128,
        Intrinsic::X86AvxVsubpd128,
        Intrinsic::X86AvxVsubss,
        Intrinsic::X86AvxVsubsd,
        Intrinsic::X86AvxVsubps256,
        Intrinsic::X86AvxVsubpd256,
        Intrinsic::X86AvxVmulps128,
        Intrinsic::X86AvxVmulpd128,
        Intrinsic::X86AvxVmulss,
        Intrinsic::X86AvxVmulsd,
        Intrinsic::X86AvxVmulps256,
        Intrinsic::X86AvxVmulpd256,
        Intrinsic::X86AvxVdivps128,
        Intrinsic::X86AvxVdivpd128,
        Intrinsic::X86AvxVdivss,
        Intrinsic::X86AvxVdivsd,
        Intrinsic::X86AvxVdivps256,
        Intrinsic::X86AvxVdivpd256,
        Intrinsic::X86AvxVsqrtps128,
        Intrinsic::X86AvxVsqrtpd128,
        Intrinsic::X86AvxVsqrtss,
        Intrinsic::X86AvxVsqrtsd,
        Intrinsic::X86AvxVsqrtps256,
        Intrinsic::X86AvxVsqrtpd256,
        Intrinsic::X86Cx16Cmpxchg16b,
        Intrinsic::X86AvxVhaddps128,
        Intrinsic::X86AvxVhaddps256,
        Intrinsic::X86AvxVhaddpd128,
        Intrinsic::X86AvxVhaddpd256,
        Intrinsic::X86AvxVhsubps128,
        Intrinsic::X86AvxVhsubps256,
        Intrinsic::X86AvxVhsubpd128,
        Intrinsic::X86AvxVhsubpd256,
        Intrinsic::X86AvxVaddsubps128,
        Intrinsic::X86AvxVaddsubps256,
        Intrinsic::X86AvxVaddsubpd128,
        Intrinsic::X86AvxVaddsubpd256,
        Intrinsic::X86AvxVdpps128,
        Intrinsic::X86AvxVdpps256,
        Intrinsic::X86AvxVdppd128,
        Intrinsic::X86Ssse3Phaddw128,
        Intrinsic::X86Ssse3Phaddd128,
        Intrinsic::X86Ssse3Phaddsw128,
        Intrinsic::X86Ssse3Phsubw128,
        Intrinsic::X86Ssse3Phsubd128,
        Intrinsic::X86Ssse3Phsubsw128,
        Intrinsic::X86Sse2Paddsb128,
        Intrinsic::X86Sse2Paddsw128,
        Intrinsic::X86Sse2Paddusb128,
        Intrinsic::X86Sse2Paddusw128,
        Intrinsic::X86Sse2Psubsb128,
        Intrinsic::X86Sse2Psubsw128,
        Intrinsic::X86Sse2Psubusb128,
        Intrinsic::X86Sse2Psubusw128,
        Intrinsic::X86Sse2Pavgb128,
        Intrinsic::X86Sse2Pavgw128,
        Intrinsic::X86Ssse3Psignb128,
        Intrinsic::X86Ssse3Psignw128,
        Intrinsic::X86Ssse3Psignd128,
        Intrinsic::X86Ssse3Pabsb128,
        Intrinsic::X86Ssse3Pabsw128,
        Intrinsic::X86Ssse3Pabsd128,
        Intrinsic::X86AvxVphaddw128,
        Intrinsic::X86AvxVphaddd128,
        Intrinsic::X86AvxVphaddsw128,
        Intrinsic::X86AvxVphsubw128,
        Intrinsic::X86AvxVphsubd128,
        Intrinsic::X86AvxVphsubsw128,
        Intrinsic::X86AvxVpaddsb128,
        Intrinsic::X86AvxVpaddsw128,
        Intrinsic::X86AvxVpaddusb128,
        Intrinsic::X86AvxVpaddusw128,
        Intrinsic::X86AvxVpsubsb128,
        Intrinsic::X86AvxVpsubsw128,
        Intrinsic::X86AvxVpsubusb128,
        Intrinsic::X86AvxVpsubusw128,
        Intrinsic::X86AvxVpavgb128,
        Intrinsic::X86AvxVpavgw128,
        Intrinsic::X86AvxVpsignb128,
        Intrinsic::X86AvxVpsignw128,
        Intrinsic::X86AvxVpsignd128,
        Intrinsic::X86AvxVpabsb128,
        Intrinsic::X86AvxVpabsw128,
        Intrinsic::X86AvxVpabsd128,
        Intrinsic::X86Avx2Vphaddw256,
        Intrinsic::X86Avx2Vphaddd256,
        Intrinsic::X86Avx2Vphaddsw256,
        Intrinsic::X86Avx2Vphsubw256,
        Intrinsic::X86Avx2Vphsubd256,
        Intrinsic::X86Avx2Vphsubsw256,
        Intrinsic::X86Avx2Vpaddsb256,
        Intrinsic::X86Avx2Vpaddsw256,
        Intrinsic::X86Avx2Vpaddusb256,
        Intrinsic::X86Avx2Vpaddusw256,
        Intrinsic::X86Avx2Vpsubsb256,
        Intrinsic::X86Avx2Vpsubsw256,
        Intrinsic::X86Avx2Vpsubusb256,
        Intrinsic::X86Avx2Vpsubusw256,
        Intrinsic::X86Avx2Vpavgb256,
        Intrinsic::X86Avx2Vpavgw256,
        Intrinsic::X86Avx2Vpsignb256,
        Intrinsic::X86Avx2Vpsignw256,
        Intrinsic::X86Avx2Vpsignd256,
        Intrinsic::X86Avx2Vpabsb256,
        Intrinsic::X86Avx2Vpabsw256,
        Intrinsic::X86Avx2Vpabsd256,
        Intrinsic::X86Sse2Packsswb128,
        Intrinsic::X86Sse2Packssdw128,
        Intrinsic::X86Sse2Packuswb128,
        Intrinsic::X86Sse41Packusdw128,
        Intrinsic::X86AvxVpacksswb128,
        Intrinsic::X86AvxVpackssdw128,
        Intrinsic::X86AvxVpackuswb128,
        Intrinsic::X86AvxVpackusdw128,
        Intrinsic::X86Avx2Vpacksswb256,
        Intrinsic::X86Avx2Vpackssdw256,
        Intrinsic::X86Avx2Vpackuswb256,
        Intrinsic::X86Avx2Vpackusdw256,
        Intrinsic::X86Clflush,
        Intrinsic::X86X87Fcom,
        Intrinsic::X86X87Fcomp,
        Intrinsic::X86X87Fcompp,
        Intrinsic::X86X87Fucom,
        Intrinsic::X86X87Fucomp,
        Intrinsic::X86X87Fucompp,
        Intrinsic::X86X87Fcomi,
        Intrinsic::X86X87Fcomip,
        Intrinsic::X86X87Fucomi,
        Intrinsic::X86X87Fucomip,
        Intrinsic::X86X87Fnstenv16,
        Intrinsic::X86X87Fnstenv32,
        Intrinsic::X86X87Fstenv16,
        Intrinsic::X86X87Fstenv32,
        Intrinsic::X86X87Fldenv16,
        Intrinsic::X86X87Fldenv32,
        Intrinsic::X86X87Fnsave16,
        Intrinsic::X86X87Fnsave32,
        Intrinsic::X86X87Fsave16,
        Intrinsic::X86X87Fsave32,
        Intrinsic::X86X87Frstor16,
        Intrinsic::X86X87Frstor32,
        Intrinsic::X86X87Region,
        Intrinsic::X86DivU8,
        Intrinsic::X86DivU16,
        Intrinsic::X86DivU32,
        Intrinsic::X86DivU64,
        Intrinsic::X86DivS8,
        Intrinsic::X86DivS16,
        Intrinsic::X86DivS32,
        Intrinsic::X86DivS64,
        Intrinsic::X86Sgdt,
        Intrinsic::X86Sidt,
        Intrinsic::X86SldtMem,
        Intrinsic::X86Sldt16,
        Intrinsic::X86Sldt32,
        Intrinsic::X86Sldt64,
        Intrinsic::X86StrMem,
        Intrinsic::X86Str16,
        Intrinsic::X86Str32,
        Intrinsic::X86Str64,
        Intrinsic::X86SmswMem,
        Intrinsic::X86Smsw16,
        Intrinsic::X86Smsw32,
        Intrinsic::X86Smsw64,
        Intrinsic::X86Lar16,
        Intrinsic::X86Lar16Mem,
        Intrinsic::X86Lar32,
        Intrinsic::X86Lar32Mem,
        Intrinsic::X86Lar64,
        Intrinsic::X86Lar64Mem,
        Intrinsic::X86Lsl16,
        Intrinsic::X86Lsl16Mem,
        Intrinsic::X86Lsl32,
        Intrinsic::X86Lsl32Mem,
        Intrinsic::X86Lsl64,
        Intrinsic::X86Lsl64Mem,
        Intrinsic::X86Verr,
        Intrinsic::X86VerrMem,
        Intrinsic::X86Verw,
        Intrinsic::X86VerwMem,
        Intrinsic::X86In8,
        Intrinsic::X86In16,
        Intrinsic::X86In32,
        Intrinsic::X86Out8,
        Intrinsic::X86Out16,
        Intrinsic::X86Out32,
        Intrinsic::X86In8Imm,
        Intrinsic::X86In16Imm,
        Intrinsic::X86In32Imm,
        Intrinsic::X86Out8Imm,
        Intrinsic::X86Out16Imm,
        Intrinsic::X86Out32Imm,
        Intrinsic::X86RflagsRead,
        Intrinsic::X86RflagsSwap,
        Intrinsic::X86Rdpmc,
        Intrinsic::X86X87Ffree0,
        Intrinsic::X86X87Ffree1,
        Intrinsic::X86X87Ffree2,
        Intrinsic::X86X87Ffree3,
        Intrinsic::X86X87Ffree4,
        Intrinsic::X86X87Ffree5,
        Intrinsic::X86X87Ffree6,
        Intrinsic::X86X87Ffree7,
        Intrinsic::X86X87Fdecstp,
        Intrinsic::X86X87Fincstp,
        Intrinsic::X86X87Fnop,
        Intrinsic::X86Ins8,
        Intrinsic::X86Ins16,
        Intrinsic::X86Ins32,
        Intrinsic::X86Outs8,
        Intrinsic::X86Outs16,
        Intrinsic::X86Outs32,
        Intrinsic::X86RepIns8,
        Intrinsic::X86RepIns16,
        Intrinsic::X86RepIns32,
        Intrinsic::X86RepOuts8,
        Intrinsic::X86RepOuts16,
        Intrinsic::X86RepOuts32,
        Intrinsic::X86Ins8OwnedTerminal,
        Intrinsic::X86Ins16OwnedTerminal,
        Intrinsic::X86Ins32OwnedTerminal,
        Intrinsic::X86Outs8OwnedTerminal,
        Intrinsic::X86Outs16OwnedTerminal,
        Intrinsic::X86Outs32OwnedTerminal,
        Intrinsic::X86RepIns8OwnedTerminal,
        Intrinsic::X86RepIns16OwnedTerminal,
        Intrinsic::X86RepIns32OwnedTerminal,
        Intrinsic::X86RepOuts8OwnedTerminal,
        Intrinsic::X86RepOuts16OwnedTerminal,
        Intrinsic::X86RepOuts32OwnedTerminal,
        Intrinsic::X86Pclmulqdq128,
        Intrinsic::X86Aesenc128,
        Intrinsic::X86Aesenclast128,
        Intrinsic::X86Aesdec128,
        Intrinsic::X86Aesdeclast128,
        Intrinsic::X86Aesimc128,
        Intrinsic::X86Aeskeygenassist128,
    }

    pub fn info(self) -> IntrinsicInfo {
        const MEM: &[Type] = &[Type::Ptr, Type::Ptr, Type::I64];
        let (name, params, returns, pure): (_, &'static [Type], &'static [Type], _) = match self {
            x86_system_intrinsics!(all) => return self.generated_system_info().unwrap(),
            x86_query_intrinsics!(all) => return self.generated_query_info().unwrap(),
            Intrinsic::X86Pclmulqdq128 => (
                "x86_pclmulqdq128",
                &[
                    Type::Vector(VectorType::I64x2),
                    Type::Vector(VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesenc128 => (
                "x86_aesenc128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesenclast128 => (
                "x86_aesenclast128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesdec128 => (
                "x86_aesdec128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesdeclast128 => (
                "x86_aesdeclast128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aesimc128 => (
                "x86_aesimc128",
                &[Type::Vector(crate::VectorType::I64x2)],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Aeskeygenassist128 => (
                "x86_aeskeygenassist128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I32],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Ins8 => ("x86_ins8", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Ins16 => ("x86_ins16", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Ins32 => ("x86_ins32", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Outs8 => ("x86_outs8", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Outs16 => ("x86_outs16", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86Outs32 => ("x86_outs32", &[Type::I16, Type::Ptr], &[Type::Ptr], false),
            Intrinsic::X86RepIns8 => ("x86_rep_ins8", &[Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64], false),
            Intrinsic::X86RepIns16 => ("x86_rep_ins16", &[Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64], false),
            Intrinsic::X86RepIns32 => ("x86_rep_ins32", &[Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64], false),
            Intrinsic::X86RepOuts8 => ("x86_rep_outs8", &[Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64], false),
            Intrinsic::X86RepOuts16 => ("x86_rep_outs16", &[Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64], false),
            Intrinsic::X86RepOuts32 => ("x86_rep_outs32", &[Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64], false),
            Intrinsic::X86Ins8OwnedTerminal => ("x86_ins8_owned_terminal", &[Type::I64, Type::I16, Type::Ptr], &[Type::Ptr, Type::I64, Type::I64], false),
            Intrinsic::X86Ins16OwnedTerminal => ("x86_ins16_owned_terminal", &[Type::I64, Type::I16, Type::Ptr], &[Type::Ptr, Type::I64, Type::I64], false),
            Intrinsic::X86Ins32OwnedTerminal => ("x86_ins32_owned_terminal", &[Type::I64, Type::I16, Type::Ptr], &[Type::Ptr, Type::I64, Type::I64], false),
            Intrinsic::X86Outs8OwnedTerminal => ("x86_outs8_owned_terminal", &[Type::I64, Type::I16, Type::Ptr], &[Type::Ptr, Type::I64, Type::I64], false),
            Intrinsic::X86Outs16OwnedTerminal => ("x86_outs16_owned_terminal", &[Type::I64, Type::I16, Type::Ptr], &[Type::Ptr, Type::I64, Type::I64], false),
            Intrinsic::X86Outs32OwnedTerminal => ("x86_outs32_owned_terminal", &[Type::I64, Type::I16, Type::Ptr], &[Type::Ptr, Type::I64, Type::I64], false),
            Intrinsic::X86RepIns8OwnedTerminal => ("x86_rep_ins8_owned_terminal", &[Type::I64, Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64, Type::I64, Type::I64], false),
            Intrinsic::X86RepIns16OwnedTerminal => ("x86_rep_ins16_owned_terminal", &[Type::I64, Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64, Type::I64, Type::I64], false),
            Intrinsic::X86RepIns32OwnedTerminal => ("x86_rep_ins32_owned_terminal", &[Type::I64, Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64, Type::I64, Type::I64], false),
            Intrinsic::X86RepOuts8OwnedTerminal => ("x86_rep_outs8_owned_terminal", &[Type::I64, Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64, Type::I64, Type::I64], false),
            Intrinsic::X86RepOuts16OwnedTerminal => ("x86_rep_outs16_owned_terminal", &[Type::I64, Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64, Type::I64, Type::I64], false),
            Intrinsic::X86RepOuts32OwnedTerminal => ("x86_rep_outs32_owned_terminal", &[Type::I64, Type::I16, Type::Ptr, Type::I64], &[Type::Ptr, Type::I64, Type::I64, Type::I64], false),


            Intrinsic::X86Rdpmc => ("x86_rdpmc", &[Type::I32], &[Type::I64], false),
            Intrinsic::X86X87Ffree0 => ("x86_x87_ffree0", &[], &[], false),
            Intrinsic::X86X87Ffree1 => ("x86_x87_ffree1", &[], &[], false),
            Intrinsic::X86X87Ffree2 => ("x86_x87_ffree2", &[], &[], false),
            Intrinsic::X86X87Ffree3 => ("x86_x87_ffree3", &[], &[], false),
            Intrinsic::X86X87Ffree4 => ("x86_x87_ffree4", &[], &[], false),
            Intrinsic::X86X87Ffree5 => ("x86_x87_ffree5", &[], &[], false),
            Intrinsic::X86X87Ffree6 => ("x86_x87_ffree6", &[], &[], false),
            Intrinsic::X86X87Ffree7 => ("x86_x87_ffree7", &[], &[], false),
            Intrinsic::X86X87Fdecstp => ("x86_x87_fdecstp", &[], &[], false),
            Intrinsic::X86X87Fincstp => ("x86_x87_fincstp", &[], &[], false),
            Intrinsic::X86X87Fnop => ("x86_x87_fnop", &[], &[], false),

            Intrinsic::X86RflagsRead => ("x86_rflags_read", &[], &[Type::I64], false),
            Intrinsic::X86RflagsSwap => ("x86_rflags_swap", &[Type::I64], &[Type::I64], false),
            Intrinsic::X86DivU8 => ("x86_div_u8", &[Type::I8; 3], &[Type::I8; 2], false),
            Intrinsic::X86DivU16 => ("x86_div_u16", &[Type::I16; 3], &[Type::I16; 2], false),
            Intrinsic::X86DivU32 => ("x86_div_u32", &[Type::I32; 3], &[Type::I32; 2], false),
            Intrinsic::X86DivU64 => ("x86_div_u64", &[Type::I64; 3], &[Type::I64; 2], false),
            Intrinsic::X86DivS8 => ("x86_div_s8", &[Type::I8; 3], &[Type::I8; 2], false),
            Intrinsic::X86DivS16 => ("x86_div_s16", &[Type::I16; 3], &[Type::I16; 2], false),
            Intrinsic::X86DivS32 => ("x86_div_s32", &[Type::I32; 3], &[Type::I32; 2], false),
            Intrinsic::X86DivS64 => ("x86_div_s64", &[Type::I64; 3], &[Type::I64; 2], false),
            Intrinsic::X86X87Region => (
                "x86_x87_region",
                &[Type::Ptr, Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86Sse2Packsswb128 => (
                "x86_sse2_packsswb128",
                &[Type::Vector(crate::VectorType::I16x8); 2],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Packssdw128 => (
                "x86_sse2_packssdw128",
                &[Type::Vector(crate::VectorType::I32x4); 2],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Packuswb128 => (
                "x86_sse2_packuswb128",
                &[Type::Vector(crate::VectorType::I16x8); 2],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse41Packusdw128 => (
                "x86_sse41_packusdw128",
                &[Type::Vector(crate::VectorType::I32x4); 2],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpacksswb128 => (
                "x86_avx_vpacksswb128",
                &[Type::Vector(crate::VectorType::I16x8); 2],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpackssdw128 => (
                "x86_avx_vpackssdw128",
                &[Type::Vector(crate::VectorType::I32x4); 2],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpackuswb128 => (
                "x86_avx_vpackuswb128",
                &[Type::Vector(crate::VectorType::I16x8); 2],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpackusdw128 => (
                "x86_avx_vpackusdw128",
                &[Type::Vector(crate::VectorType::I32x4); 2],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpacksswb256 => (
                "x86_avx2_vpacksswb256",
                &[Type::Vector(crate::VectorType::I16x16); 2],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpackssdw256 => (
                "x86_avx2_vpackssdw256",
                &[Type::Vector(crate::VectorType::I32x8); 2],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpackuswb256 => (
                "x86_avx2_vpackuswb256",
                &[Type::Vector(crate::VectorType::I16x16); 2],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpackusdw256 => (
                "x86_avx2_vpackusdw256",
                &[Type::Vector(crate::VectorType::I32x8); 2],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Ssse3Phaddw128 => (
                "x86_ssse3_phaddw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Phaddd128 => (
                "x86_ssse3_phaddd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Ssse3Phaddsw128 => (
                "x86_ssse3_phaddsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Phsubw128 => (
                "x86_ssse3_phsubw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Phsubd128 => (
                "x86_ssse3_phsubd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Ssse3Phsubsw128 => (
                "x86_ssse3_phsubsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Paddsb128 => (
                "x86_sse2_paddsb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Paddsw128 => (
                "x86_sse2_paddsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Paddusb128 => (
                "x86_sse2_paddusb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Paddusw128 => (
                "x86_sse2_paddusw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Psubsb128 => (
                "x86_sse2_psubsb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Psubsw128 => (
                "x86_sse2_psubsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Psubusb128 => (
                "x86_sse2_psubusb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Psubusw128 => (
                "x86_sse2_psubusw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pavgb128 => (
                "x86_sse2_pavgb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Pavgw128 => (
                "x86_sse2_pavgw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Psignb128 => (
                "x86_ssse3_psignb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Ssse3Psignw128 => (
                "x86_ssse3_psignw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Psignd128 => (
                "x86_ssse3_psignd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Ssse3Pabsb128 => (
                "x86_ssse3_pabsb128",
                &[Type::Vector(crate::VectorType::I8x16)],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Ssse3Pabsw128 => (
                "x86_ssse3_pabsw128",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Pabsd128 => (
                "x86_ssse3_pabsd128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVphaddw128 => (
                "x86_avx_vphaddw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVphaddd128 => (
                "x86_avx_vphaddd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVphaddsw128 => (
                "x86_avx_vphaddsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVphsubw128 => (
                "x86_avx_vphsubw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVphsubd128 => (
                "x86_avx_vphsubd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVphsubsw128 => (
                "x86_avx_vphsubsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpaddsb128 => (
                "x86_avx_vpaddsb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpaddsw128 => (
                "x86_avx_vpaddsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpaddusb128 => (
                "x86_avx_vpaddusb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpaddusw128 => (
                "x86_avx_vpaddusw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsubsb128 => (
                "x86_avx_vpsubsb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpsubsw128 => (
                "x86_avx_vpsubsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsubusb128 => (
                "x86_avx_vpsubusb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpsubusw128 => (
                "x86_avx_vpsubusw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpavgb128 => (
                "x86_avx_vpavgb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpavgw128 => (
                "x86_avx_vpavgw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsignb128 => (
                "x86_avx_vpsignb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpsignw128 => (
                "x86_avx_vpsignw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsignd128 => (
                "x86_avx_vpsignd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpabsb128 => (
                "x86_avx_vpabsb128",
                &[Type::Vector(crate::VectorType::I8x16)],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpabsw128 => (
                "x86_avx_vpabsw128",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpabsd128 => (
                "x86_avx_vpabsd128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vphaddw256 => (
                "x86_avx2_vphaddw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vphaddd256 => (
                "x86_avx2_vphaddd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vphaddsw256 => (
                "x86_avx2_vphaddsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vphsubw256 => (
                "x86_avx2_vphsubw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vphsubd256 => (
                "x86_avx2_vphsubd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vphsubsw256 => (
                "x86_avx2_vphsubsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpaddsb256 => (
                "x86_avx2_vpaddsb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpaddsw256 => (
                "x86_avx2_vpaddsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpaddusb256 => (
                "x86_avx2_vpaddusb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpaddusw256 => (
                "x86_avx2_vpaddusw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsubsb256 => (
                "x86_avx2_vpsubsb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpsubsw256 => (
                "x86_avx2_vpsubsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsubusb256 => (
                "x86_avx2_vpsubusb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpsubusw256 => (
                "x86_avx2_vpsubusw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpavgb256 => (
                "x86_avx2_vpavgb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpavgw256 => (
                "x86_avx2_vpavgw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsignb256 => (
                "x86_avx2_vpsignb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpsignw256 => (
                "x86_avx2_vpsignw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsignd256 => (
                "x86_avx2_vpsignd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpabsb256 => (
                "x86_avx2_vpabsb256",
                &[Type::Vector(crate::VectorType::I8x32)],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpabsw256 => (
                "x86_avx2_vpabsw256",
                &[Type::Vector(crate::VectorType::I16x16)],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpabsd256 => (
                "x86_avx2_vpabsd256",
                &[Type::Vector(crate::VectorType::I32x8)],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Sse41Blendvps128 => (
                "x86_sse41_blendvps128",
                &[Type::Vector(crate::VectorType::F32x4); 3],
                &[Type::Vector(crate::VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86Sse41Blendvpd128 => (
                "x86_sse41_blendvpd128",
                &[Type::Vector(crate::VectorType::F64x2); 3],
                &[Type::Vector(crate::VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86Sse41Pblendvb128 => (
                "x86_sse41_pblendvb128",
                &[Type::Vector(crate::VectorType::I8x16); 3],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVblendvps128 => (
                "x86_avx_vblendvps128",
                &[Type::Vector(crate::VectorType::F32x4); 3],
                &[Type::Vector(crate::VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86AvxVblendvpd128 => (
                "x86_avx_vblendvpd128",
                &[Type::Vector(crate::VectorType::F64x2); 3],
                &[Type::Vector(crate::VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86AvxVpblendvb128 => (
                "x86_avx_vpblendvb128",
                &[Type::Vector(crate::VectorType::I8x16); 3],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVblendvps256 => (
                "x86_avx_vblendvps256",
                &[Type::Vector(crate::VectorType::F32x8); 3],
                &[Type::Vector(crate::VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86AvxVblendvpd256 => (
                "x86_avx_vblendvpd256",
                &[Type::Vector(crate::VectorType::F64x4); 3],
                &[Type::Vector(crate::VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpblendvb256 => (
                "x86_avx2_vpblendvb256",
                &[Type::Vector(crate::VectorType::I8x32); 3],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Sse41Mpsadbw128 => (
                "x86_sse41_mpsadbw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVmpsadbw128 => (
                "x86_avx_vmpsadbw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Avx2Vmpsadbw256 => (
                "x86_avx2_vmpsadbw256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Sse41Phminposuw128 => (
                "x86_sse41_phminposuw128",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVphminposuw128 => (
                "x86_avx_vphminposuw128",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pmaddwd128 => (
                "x86_sse2_pmaddwd128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Ssse3Pmaddubsw128 => (
                "x86_ssse3_pmaddubsw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pmulhw128 => (
                "x86_sse2_pmulhw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pmulhuw128 => (
                "x86_sse2_pmulhuw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Ssse3Pmulhrsw128 => (
                "x86_ssse3_pmulhrsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse41Pmuldq128 => (
                "x86_sse41_pmuldq128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Sse2Pmuludq128 => (
                "x86_sse2_pmuludq128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Sse2Psadbw128 => (
                "x86_sse2_psadbw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpmaddwd128 => (
                "x86_avx_vpmaddwd128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpmaddubsw128 => (
                "x86_avx_vpmaddubsw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpmulhw128 => (
                "x86_avx_vpmulhw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpmulhuw128 => (
                "x86_avx_vpmulhuw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpmulhrsw128 => (
                "x86_avx_vpmulhrsw128",
                &[
                    Type::Vector(crate::VectorType::I16x8),
                    Type::Vector(crate::VectorType::I16x8),
                ],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpmuldq128 => (
                "x86_avx_vpmuldq128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpmuludq128 => (
                "x86_avx_vpmuludq128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpsadbw128 => (
                "x86_avx_vpsadbw128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx2Vpmaddwd256 => (
                "x86_avx2_vpmaddwd256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpmaddubsw256 => (
                "x86_avx2_vpmaddubsw256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpmulhw256 => (
                "x86_avx2_vpmulhw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpmulhuw256 => (
                "x86_avx2_vpmulhuw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpmulhrsw256 => (
                "x86_avx2_vpmulhrsw256",
                &[
                    Type::Vector(crate::VectorType::I16x16),
                    Type::Vector(crate::VectorType::I16x16),
                ],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpmuldq256 => (
                "x86_avx2_vpmuldq256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpmuludq256 => (
                "x86_avx2_vpmuludq256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsadbw256 => (
                "x86_avx2_vpsadbw256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Sse2Pslldq128 => (
                "x86_sse2_pslldq128",
                &[Type::Vector(crate::VectorType::I8x16), Type::I32],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse2Psrldq128 => (
                "x86_sse2_psrldq128",
                &[Type::Vector(crate::VectorType::I8x16), Type::I32],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpslldq128 => (
                "x86_avx_vpslldq128",
                &[Type::Vector(crate::VectorType::I8x16), Type::I32],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpsrldq128 => (
                "x86_avx_vpsrldq128",
                &[Type::Vector(crate::VectorType::I8x16), Type::I32],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpslldq256 => (
                "x86_avx2_vpslldq256",
                &[Type::Vector(crate::VectorType::I8x32), Type::I32],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrldq256 => (
                "x86_avx2_vpsrldq256",
                &[Type::Vector(crate::VectorType::I8x32), Type::I32],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),
            Intrinsic::X86Sse2Psllw128 => (
                "x86_sse2_psllw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Pslld128 => (
                "x86_sse2_pslld128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Sse2Psllq128 => (
                "x86_sse2_psllq128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I64],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Sse2Psrlw128 => (
                "x86_sse2_psrlw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Psrld128 => (
                "x86_sse2_psrld128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Sse2Psrlq128 => (
                "x86_sse2_psrlq128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I64],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Sse2Psraw128 => (
                "x86_sse2_psraw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86Sse2Psrad128 => (
                "x86_sse2_psrad128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpsllw128 => (
                "x86_avx_vpsllw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpslld128 => (
                "x86_avx_vpslld128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpsllq128 => (
                "x86_avx_vpsllq128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I64],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpsrlw128 => (
                "x86_avx_vpsrlw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsrld128 => (
                "x86_avx_vpsrld128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86AvxVpsrlq128 => (
                "x86_avx_vpsrlq128",
                &[Type::Vector(crate::VectorType::I64x2), Type::I64],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86AvxVpsraw128 => (
                "x86_avx_vpsraw128",
                &[Type::Vector(crate::VectorType::I16x8), Type::I64],
                &[Type::Vector(crate::VectorType::I16x8)],
                true,
            ),
            Intrinsic::X86AvxVpsrad128 => (
                "x86_avx_vpsrad128",
                &[Type::Vector(crate::VectorType::I32x4), Type::I64],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllw256 => (
                "x86_avx2_vpsllw256",
                &[Type::Vector(crate::VectorType::I16x16), Type::I64],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpslld256 => (
                "x86_avx2_vpslld256",
                &[Type::Vector(crate::VectorType::I32x8), Type::I64],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllq256 => (
                "x86_avx2_vpsllq256",
                &[Type::Vector(crate::VectorType::I64x4), Type::I64],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlw256 => (
                "x86_avx2_vpsrlw256",
                &[Type::Vector(crate::VectorType::I16x16), Type::I64],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrld256 => (
                "x86_avx2_vpsrld256",
                &[Type::Vector(crate::VectorType::I32x8), Type::I64],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlq256 => (
                "x86_avx2_vpsrlq256",
                &[Type::Vector(crate::VectorType::I64x4), Type::I64],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsraw256 => (
                "x86_avx2_vpsraw256",
                &[Type::Vector(crate::VectorType::I16x16), Type::I64],
                &[Type::Vector(crate::VectorType::I16x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrad256 => (
                "x86_avx2_vpsrad256",
                &[Type::Vector(crate::VectorType::I32x8), Type::I64],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllvd128 => (
                "x86_avx2_vpsllvd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllvq128 => (
                "x86_avx2_vpsllvq128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlvd128 => (
                "x86_avx2_vpsrlvd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlvq128 => (
                "x86_avx2_vpsrlvq128",
                &[
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                true,
            ),
            Intrinsic::X86Avx2Vpsravd128 => (
                "x86_avx2_vpsravd128",
                &[
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllvd256 => (
                "x86_avx2_vpsllvd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsllvq256 => (
                "x86_avx2_vpsllvq256",
                &[
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlvd256 => (
                "x86_avx2_vpsrlvd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpsrlvq256 => (
                "x86_avx2_vpsrlvq256",
                &[
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpsravd256 => (
                "x86_avx2_vpsravd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Ssse3Pshufb128 => (
                "x86_ssse3_pshufb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxVpshufb128 => (
                "x86_avx_vpshufb128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Avx2Vpshufb256 => (
                "x86_avx2_vpshufb256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::Vector(crate::VectorType::I8x32)],
                true,
            ),

            Intrinsic::X86Xrstor64 => ("x86_xrstor64", &[Type::Ptr, Type::I64], &[], false),
            Intrinsic::X86X87ReadCwNowait => ("x86_x87_read_cw_nowait", &[], &[Type::I16], false),
            Intrinsic::X86X87ReadSwNowait => ("x86_x87_read_sw_nowait", &[], &[Type::I16], false),
            Intrinsic::X86X87ReadCwWait => ("x86_x87_read_cw_wait", &[], &[Type::I16], false),
            Intrinsic::X86X87ReadSwWait => ("x86_x87_read_sw_wait", &[], &[Type::I16], false),
            Intrinsic::X86X87ExchangeCw => {
                ("x86_x87_exchange_cw", &[Type::I16], &[Type::I16], false)
            }
            Intrinsic::X86X87ClexNowait => ("x86_x87_clex_nowait", &[], &[], false),
            Intrinsic::X86X87ClexWait => ("x86_x87_clex_wait", &[], &[], false),
            Intrinsic::X86X87InitNowait => ("x86_x87_init_nowait", &[], &[], false),
            Intrinsic::X86X87InitWait => ("x86_x87_init_wait", &[], &[], false),
            Intrinsic::X86X87Fist16 => (
                "x86_x87_fist16",
                &[Type::F80],
                &[Type::I16, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fist32 => (
                "x86_x87_fist32",
                &[Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fistp16 => (
                "x86_x87_fistp16",
                &[Type::F80],
                &[Type::I16, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fistp32 => (
                "x86_x87_fistp32",
                &[Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fistp64 => (
                "x86_x87_fistp64",
                &[Type::F80],
                &[Type::I64, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisttp16 => (
                "x86_x87_fisttp16",
                &[Type::F80],
                &[Type::I16, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisttp32 => (
                "x86_x87_fisttp32",
                &[Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisttp64 => (
                "x86_x87_fisttp64",
                &[Type::F80],
                &[Type::I64, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fild16 => (
                "x86_x87_fild16",
                &[Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fild32 => (
                "x86_x87_fild32",
                &[Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fild64 => (
                "x86_x87_fild64",
                &[Type::I64],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fbld => ("x86_x87_fbld", &[Type::Ptr], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fbstp => (
                "x86_x87_fbstp",
                &[Type::F80, Type::Ptr],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fldpi => ("x86_x87_fldpi", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fldl2e => ("x86_x87_fldl2e", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fldl2t => ("x86_x87_fldl2t", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fldlg2 => ("x86_x87_fldlg2", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fldln2 => ("x86_x87_fldln2", &[], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fiadd16 => (
                "x86_x87_fiadd16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fiadd32 => (
                "x86_x87_fiadd32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fimul16 => (
                "x86_x87_fimul16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fimul32 => (
                "x86_x87_fimul32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisub16 => (
                "x86_x87_fisub16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisub32 => (
                "x86_x87_fisub32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisubr16 => (
                "x86_x87_fisubr16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fisubr32 => (
                "x86_x87_fisubr32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fidiv16 => (
                "x86_x87_fidiv16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fidiv32 => (
                "x86_x87_fidiv32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fidivr16 => (
                "x86_x87_fidivr16",
                &[Type::F80, Type::I16],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fidivr32 => (
                "x86_x87_fidivr32",
                &[Type::F80, Type::I32],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fcom => ("x86_x87_fcom", &[Type::F80, Type::F80], &[Type::I32], false),
            Intrinsic::X86X87Fcomp => (
                "x86_x87_fcomp",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fcompp => (
                "x86_x87_fcompp",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucom => (
                "x86_x87_fucom",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucomp => (
                "x86_x87_fucomp",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucompp => (
                "x86_x87_fucompp",
                &[Type::F80, Type::F80],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86X87Fcomi => (
                "x86_x87_fcomi",
                &[Type::F80, Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fcomip => (
                "x86_x87_fcomip",
                &[Type::F80, Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucomi => (
                "x86_x87_fucomi",
                &[Type::F80, Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fucomip => (
                "x86_x87_fucomip",
                &[Type::F80, Type::F80],
                &[Type::I32, Type::I32],
                false,
            ),
            Intrinsic::X86X87FaddSt1 => (
                "x86_x87_fadd_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FmulSt1 => (
                "x86_x87_fmul_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FsubSt1 => (
                "x86_x87_fsub_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FsubrSt1 => (
                "x86_x87_fsubr_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FdivSt1 => (
                "x86_x87_fdiv_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87FdivrSt1 => (
                "x86_x87_fdivr_st1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fadd => (
                "x86_x87_fadd",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fmul => (
                "x86_x87_fmul",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsub => (
                "x86_x87_fsub",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsubr => (
                "x86_x87_fsubr",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fdiv => (
                "x86_x87_fdiv",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fdivr => (
                "x86_x87_fdivr",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Faddp => (
                "x86_x87_faddp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fmulp => (
                "x86_x87_fmulp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsubp => (
                "x86_x87_fsubp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsubrp => (
                "x86_x87_fsubrp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fdivp => (
                "x86_x87_fdivp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fdivrp => (
                "x86_x87_fdivrp",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fabs => ("x86_x87_fabs", &[Type::F80], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fchs => ("x86_x87_fchs", &[Type::F80], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fsqrt => (
                "x86_x87_fsqrt",
                &[Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Frndint => (
                "x86_x87_frndint",
                &[Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Ftst => ("x86_x87_ftst", &[Type::F80], &[Type::I32], false),
            Intrinsic::X86X87Fxam => ("x86_x87_fxam", &[Type::F80], &[Type::I32], false),
            Intrinsic::X86X87Fsincos => (
                "x86_x87_fsincos",
                &[Type::F80],
                &[Type::F80, Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fptan => (
                "x86_x87_fptan",
                &[Type::F80],
                &[Type::F80, Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fxtract => (
                "x86_x87_fxtract",
                &[Type::F80],
                &[Type::F80, Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fsin => ("x86_x87_fsin", &[Type::F80], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87Fcos => ("x86_x87_fcos", &[Type::F80], &[Type::F80, Type::I32], false),
            Intrinsic::X86X87F2xm1 => (
                "x86_x87_f2xm1",
                &[Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fpatan => (
                "x86_x87_fpatan",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fyl2x => (
                "x86_x87_fyl2x",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fyl2xp1 => (
                "x86_x87_fyl2xp1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fscale => (
                "x86_x87_fscale",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fprem => (
                "x86_x87_fprem",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Fprem1 => (
                "x86_x87_fprem1",
                &[Type::F80, Type::F80],
                &[Type::F80, Type::I32],
                false,
            ),
            Intrinsic::X86X87Wait => ("x86_x87_wait", &[], &[], false),
            Intrinsic::X86MmxEmms => ("x86_mmx_emms", &[], &[], false),
            Intrinsic::X86Lahf => ("x86_lahf", &[], &[Type::I8], false),
            Intrinsic::X86SahfExchange => ("x86_sahf_exchange", &[Type::I8], &[Type::I8], false),
            Intrinsic::X86X87Fnstenv16 => ("x86_x87_fnstenv16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fnstenv32 => ("x86_x87_fnstenv32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fstenv16 => ("x86_x87_fstenv16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fstenv32 => ("x86_x87_fstenv32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fldenv16 => ("x86_x87_fldenv16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fldenv32 => ("x86_x87_fldenv32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fnsave16 => ("x86_x87_fnsave16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fnsave32 => ("x86_x87_fnsave32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fsave16 => ("x86_x87_fsave16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Fsave32 => ("x86_x87_fsave32", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Frstor16 => ("x86_x87_frstor16", &[Type::Ptr], &[], false),
            Intrinsic::X86X87Frstor32 => ("x86_x87_frstor32", &[Type::Ptr], &[], false),
            Intrinsic::X86Fxsave64 => ("x86_fxsave64", &[Type::Ptr], &[], false),
            Intrinsic::X86Fxrstor64 => ("x86_fxrstor64", &[Type::Ptr], &[], false),
            Intrinsic::X86Xsave64 => ("x86_xsave64", &[Type::Ptr, Type::I64], &[], false),
            Intrinsic::X86Xgetbv0 => ("x86_xgetbv0", &[], &[Type::I64], false),
            Intrinsic::X86Clflush => ("x86_clflush", &[Type::Ptr], &[], false),
            Intrinsic::X86Prefetchnta => ("x86_prefetchnta", &[Type::Ptr], &[], false),
            Intrinsic::X86Prefetcht0 => ("x86_prefetcht0", &[Type::Ptr], &[], false),
            Intrinsic::X86Prefetcht1 => ("x86_prefetcht1", &[Type::Ptr], &[], false),
            Intrinsic::X86Prefetcht2 => ("x86_prefetcht2", &[Type::Ptr], &[], false),
            Intrinsic::X86SseMovntps128 => (
                "x86_sse_movntps128",
                &[Type::Ptr, Type::Vector(crate::VectorType::F32x4)],
                &[],
                false,
            ),
            Intrinsic::X86SseMovntpd128 => (
                "x86_sse_movntpd128",
                &[Type::Ptr, Type::Vector(crate::VectorType::F64x2)],
                &[],
                false,
            ),
            Intrinsic::X86SseMovntdq128 => (
                "x86_sse_movntdq128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x2)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntps128 => (
                "x86_avx_vmovntps128",
                &[Type::Ptr, Type::Vector(crate::VectorType::F32x4)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntps256 => (
                "x86_avx_vmovntps256",
                &[Type::Ptr, Type::Vector(crate::VectorType::F32x8)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntpd128 => (
                "x86_avx_vmovntpd128",
                &[Type::Ptr, Type::Vector(crate::VectorType::F64x2)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntpd256 => (
                "x86_avx_vmovntpd256",
                &[Type::Ptr, Type::Vector(crate::VectorType::F64x4)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntdq128 => (
                "x86_avx_vmovntdq128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x2)],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmovntdq256 => (
                "x86_avx_vmovntdq256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x4)],
                &[],
                false,
            ),
            Intrinsic::X86Movnti32 => ("x86_movnti32", &[Type::Ptr, Type::I32], &[], false),
            Intrinsic::X86Movnti64 => ("x86_movnti64", &[Type::Ptr, Type::I64], &[], false),
            Intrinsic::X86Sse41Movntdqa128 => (
                "x86_sse41_movntdqa128",
                &[Type::Ptr],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vmovntdqa256 => (
                "x86_avx2_vmovntdqa256",
                &[Type::Ptr],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Cpuid => ("x86_cpuid", &[Type::I32, Type::I32], &[Type::I128], false),
            Intrinsic::X86Rdtsc => ("x86_rdtsc", &[], &[Type::I64], false),
            Intrinsic::X86Lfence => ("x86_lfence", &[], &[], false),
            Intrinsic::X86Sfence => ("x86_sfence", &[], &[], false),
            Intrinsic::X86Mfence => ("x86_mfence", &[], &[], false),
            Intrinsic::X86Pause => ("x86_pause", &[], &[], false),

            Intrinsic::X86AvxVzeroall => ("x86_avx_vzeroall", &[], &[], false),
            Intrinsic::X86Cx16Cmpxchg16b => (
                "x86_cx16_cmpxchg16b",
                &[Type::Ptr, Type::I128, Type::I128],
                &[Type::I128],
                false,
            ),
            Intrinsic::Memcpy => ("memcpy", MEM, &[], false),
            Intrinsic::Memmove => ("memmove", MEM, &[], false),
            Intrinsic::Memcmp => ("memcmp", MEM, &[Type::I32], false),
            Intrinsic::Memset => ("memset", &[Type::Ptr, Type::I8, Type::I64], &[], false),
            Intrinsic::Debugtrap => ("debugtrap", &[], &[], false),
            Intrinsic::VaStart => ("va_start", &[Type::Ptr], &[], false),
            Intrinsic::StackAlloc => ("stack_alloc", &[Type::I64], &[Type::Ptr], false),
            Intrinsic::StackSave => ("stack_save", &[], &[Type::Ptr], false),
            Intrinsic::StackRestore => ("stack_restore", &[Type::Ptr], &[], false),
            Intrinsic::FrameAddress => ("frame_address", &[], &[Type::Ptr], false),
            Intrinsic::X86F16cPhToPs128 => (
                "x86_f16c_ph_to_ps128",
                &[Type::I64],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86F16cPhToPs256 => (
                "x86_f16c_ph_to_ps256",
                &[Type::Vector(crate::VectorType::I16x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86F16cPsToPh128 => (
                "x86_f16c_ps_to_ph128",
                &[Type::Vector(crate::VectorType::F32x4), Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86F16cPsToPh256 => (
                "x86_f16c_ps_to_ph256",
                &[Type::Vector(crate::VectorType::F32x8), Type::I32],
                &[Type::Vector(crate::VectorType::I16x8)],
                false,
            ),
            Intrinsic::X86Bmi1Bextr32 => (
                "x86_bmi1_bextr32",
                &[Type::I32, Type::I32],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Bmi1Bextr64 => (
                "x86_bmi1_bextr64",
                &[Type::I64, Type::I64],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Bmi2Pdep32 => (
                "x86_bmi2_pdep32",
                &[Type::I32, Type::I32],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Bmi2Pdep64 => (
                "x86_bmi2_pdep64",
                &[Type::I64, Type::I64],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Bmi2Pext32 => (
                "x86_bmi2_pext32",
                &[Type::I32, Type::I32],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Bmi2Pext64 => (
                "x86_bmi2_pext64",
                &[Type::I64, Type::I64],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Avx2Vgatherdps128 => (
                "x86_avx2_vgatherdps128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherdps256 => (
                "x86_avx2_vgatherdps256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::I32x8),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherdpd128 => (
                "x86_avx2_vgatherdpd128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherdpd256 => (
                "x86_avx2_vgatherdpd256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherqps128 => (
                "x86_avx2_vgatherqps128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherqps256 => (
                "x86_avx2_vgatherqps256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherqpd128 => (
                "x86_avx2_vgatherqpd128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vgatherqpd256 => (
                "x86_avx2_vgatherqpd256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherdd128 => (
                "x86_avx2_vpgatherdd128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherdd256 => (
                "x86_avx2_vpgatherdd256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherdq128 => (
                "x86_avx2_vpgatherdq128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherdq256 => (
                "x86_avx2_vpgatherdq256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherqd128 => (
                "x86_avx2_vpgatherqd128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherqd256 => (
                "x86_avx2_vpgatherqd256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherqq128 => (
                "x86_avx2_vpgatherqq128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx2Vpgatherqq256 => (
                "x86_avx2_vpgatherqq256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpsLoad128 => (
                "x86_avx_vmaskmovps_load128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpsStore128 => (
                "x86_avx_vmaskmovps_store128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpsLoad256 => (
                "x86_avx_vmaskmovps_load256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpsStore256 => (
                "x86_avx_vmaskmovps_store256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpdLoad128 => (
                "x86_avx_vmaskmovpd_load128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x2)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpdStore128 => (
                "x86_avx_vmaskmovpd_store128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpdLoad256 => (
                "x86_avx_vmaskmovpd_load256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x4)],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVmaskmovpdStore256 => (
                "x86_avx_vmaskmovpd_store256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovdLoad128 => (
                "x86_avx2_vpmaskmovd_load128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovdStore128 => (
                "x86_avx2_vpmaskmovd_store128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovdLoad256 => (
                "x86_avx2_vpmaskmovd_load256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I32x8)],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovdStore256 => (
                "x86_avx2_vpmaskmovd_store256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovqLoad128 => (
                "x86_avx2_vpmaskmovq_load128",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x2)],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovqStore128 => (
                "x86_avx2_vpmaskmovq_store128",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovqLoad256 => (
                "x86_avx2_vpmaskmovq_load256",
                &[Type::Ptr, Type::Vector(crate::VectorType::I64x4)],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86Avx2VpmaskmovqStore256 => (
                "x86_avx2_vpmaskmovq_store256",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[],
                false,
            ),
            Intrinsic::X86AvxVcvtdq2pd128 => (
                "x86_avx_vcvtdq2pd128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVcvtdq2pd256 => (
                "x86_avx_vcvtdq2pd256",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtdq2ps128 => (
                "x86_avx_vcvtdq2ps128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtdq2ps256 => (
                "x86_avx_vcvtdq2ps256",
                &[Type::Vector(crate::VectorType::I32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVcvtpd2dq128 => (
                "x86_avx_vcvtpd2dq128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtpd2dq256 => (
                "x86_avx_vcvtpd2dq256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtpd2ps128 => (
                "x86_avx_vcvtpd2ps128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtpd2ps256 => (
                "x86_avx_vcvtpd2ps256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtps2dq128 => (
                "x86_avx_vcvtps2dq128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvtps2dq256 => (
                "x86_avx_vcvtps2dq256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86AvxVcvtps2pd128 => (
                "x86_avx_vcvtps2pd128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVcvtps2pd256 => (
                "x86_avx_vcvtps2pd256",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVcvttpd2dq128 => (
                "x86_avx_vcvttpd2dq128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvttpd2dq256 => (
                "x86_avx_vcvttpd2dq256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvttps2dq128 => (
                "x86_avx_vcvttps2dq128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcvttps2dq256 => (
                "x86_avx_vcvttps2dq256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86AvxVcvtss2si32 => {
                ("x86_avx_vcvtss2si32", &[Type::F32], &[Type::I32], false)
            }
            Intrinsic::X86AvxVcvttss2si32 => {
                ("x86_avx_vcvttss2si32", &[Type::F32], &[Type::I32], false)
            }
            Intrinsic::X86AvxVcvtss2si64 => {
                ("x86_avx_vcvtss2si64", &[Type::F32], &[Type::I64], false)
            }
            Intrinsic::X86AvxVcvttss2si64 => {
                ("x86_avx_vcvttss2si64", &[Type::F32], &[Type::I64], false)
            }
            Intrinsic::X86AvxVcvtsd2si32 => {
                ("x86_avx_vcvtsd2si32", &[Type::F64], &[Type::I32], false)
            }
            Intrinsic::X86AvxVcvttsd2si32 => {
                ("x86_avx_vcvttsd2si32", &[Type::F64], &[Type::I32], false)
            }
            Intrinsic::X86AvxVcvtsd2si64 => {
                ("x86_avx_vcvtsd2si64", &[Type::F64], &[Type::I64], false)
            }
            Intrinsic::X86AvxVcvttsd2si64 => {
                ("x86_avx_vcvttsd2si64", &[Type::F64], &[Type::I64], false)
            }
            Intrinsic::X86AvxVcvtsi2ss32 => {
                ("x86_avx_vcvtsi2ss32", &[Type::I32], &[Type::F32], false)
            }
            Intrinsic::X86AvxVcvtsi2ss64 => {
                ("x86_avx_vcvtsi2ss64", &[Type::I64], &[Type::F32], false)
            }
            Intrinsic::X86AvxVcvtsi2sd32 => {
                ("x86_avx_vcvtsi2sd32", &[Type::I32], &[Type::F64], false)
            }
            Intrinsic::X86AvxVcvtsi2sd64 => {
                ("x86_avx_vcvtsi2sd64", &[Type::I64], &[Type::F64], false)
            }
            Intrinsic::X86AvxVcvtsd2ss => ("x86_avx_vcvtsd2ss", &[Type::F64], &[Type::F32], false),
            Intrinsic::X86AvxVcvtss2sd => ("x86_avx_vcvtss2sd", &[Type::F32], &[Type::F64], false),
            Intrinsic::X86AvxVcmpps128 => (
                "x86_avx_vcmpps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86AvxVcmpps256 => (
                "x86_avx_vcmpps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                false,
            ),
            Intrinsic::X86AvxVcmppd128 => (
                "x86_avx_vcmppd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86AvxVcmppd256 => (
                "x86_avx_vcmppd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x4)],
                false,
            ),
            Intrinsic::X86AvxVcmpss => (
                "x86_avx_vcmpss",
                &[Type::F32, Type::F32, Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVcmpsd => (
                "x86_avx_vcmpsd",
                &[Type::F64, Type::F64, Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86AvxVpermilps128 => (
                "x86_avx_vpermilps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::I32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                true,
            ),
            Intrinsic::X86AvxVpermilps256 => (
                "x86_avx_vpermilps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86AvxVpermilpd128 => (
                "x86_avx_vpermilpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::I64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                true,
            ),
            Intrinsic::X86AvxVpermilpd256 => (
                "x86_avx_vpermilpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::I64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                true,
            ),
            Intrinsic::X86Avx2Vpermd256 => (
                "x86_avx2_vpermd256",
                &[
                    Type::Vector(crate::VectorType::I32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::I32x8)],
                true,
            ),
            Intrinsic::X86Avx2Vpermps256 => (
                "x86_avx2_vpermps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::I32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                true,
            ),
            Intrinsic::X86AvxVmovmskps128 => (
                "x86_avx_vmovmskps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVmovmskps256 => (
                "x86_avx_vmovmskps256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVmovmskpd128 => (
                "x86_avx_vmovmskpd128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVmovmskpd256 => (
                "x86_avx_vmovmskpd256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVpmovmskb128 => (
                "x86_avx_vpmovmskb128",
                &[Type::Vector(crate::VectorType::I8x16)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Avx2Vpmovmskb256 => (
                "x86_avx2_vpmovmskb256",
                &[Type::Vector(crate::VectorType::I8x32)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVtestps128 => (
                "x86_avx_vtestps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVtestps256 => (
                "x86_avx_vtestps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVtestpd128 => (
                "x86_avx_vtestpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVtestpd256 => (
                "x86_avx_vtestpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVptest128 => (
                "x86_avx_vptest128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVptest256 => (
                "x86_avx_vptest256",
                &[
                    Type::Vector(crate::VectorType::I8x32),
                    Type::Vector(crate::VectorType::I8x32),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVroundps128 => (
                "x86_avx_vroundps128",
                &[Type::Vector(crate::VectorType::F32x4), Type::I32],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVroundps256 => (
                "x86_avx_vroundps256",
                &[Type::Vector(crate::VectorType::F32x8), Type::I32],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVroundpd128 => (
                "x86_avx_vroundpd128",
                &[Type::Vector(crate::VectorType::F64x2), Type::I32],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVroundpd256 => (
                "x86_avx_vroundpd256",
                &[Type::Vector(crate::VectorType::F64x4), Type::I32],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVroundss => (
                "x86_avx_vroundss",
                &[Type::F32, Type::I32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVroundsd => (
                "x86_avx_vroundsd",
                &[Type::F64, Type::I32],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVminps128 => (
                "x86_avx_vminps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVminps256 => (
                "x86_avx_vminps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVminpd128 => (
                "x86_avx_vminpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVminpd256 => (
                "x86_avx_vminpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVminss => (
                "x86_avx_vminss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVminsd => (
                "x86_avx_vminsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVmaxps128 => (
                "x86_avx_vmaxps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVmaxps256 => (
                "x86_avx_vmaxps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVmaxpd128 => (
                "x86_avx_vmaxpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVmaxpd256 => (
                "x86_avx_vmaxpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVmaxss => (
                "x86_avx_vmaxss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVmaxsd => (
                "x86_avx_vmaxsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVrcpps128 => (
                "x86_avx_vrcpps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVrcpps256 => (
                "x86_avx_vrcpps256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVrcpss => ("x86_avx_vrcpss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86AvxVrsqrtps128 => (
                "x86_avx_vrsqrtps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVrsqrtps256 => (
                "x86_avx_vrsqrtps256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVrsqrtss => ("x86_avx_vrsqrtss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86AvxVcomiss => (
                "x86_avx_vcomiss",
                &[Type::F32, Type::F32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVucomiss => (
                "x86_avx_vucomiss",
                &[Type::F32, Type::F32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVcomisd => (
                "x86_avx_vcomisd",
                &[Type::F64, Type::F64],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVucomisd => (
                "x86_avx_vucomisd",
                &[Type::F64, Type::F64],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86AvxVmaskmovdqu => (
                "x86_avx_vmaskmovdqu",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[],
                false,
            ),
            Intrinsic::X86SseReadMxcsr => ("x86_sse_read_mxcsr", &[], &[Type::I32], false),
            Intrinsic::X86SseSwapMxcsr => ("x86_sse_swap_mxcsr", &[Type::I32], &[Type::I32], false),
            Intrinsic::X86SseMmxvalueCvtpi2ps => (
                "x86_sse_mmxvalue_cvtpi2ps",
                &[Type::Vector(crate::VectorType::F32x4), Type::I64],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvtps2pi => (
                "x86_sse_mmxvalue_cvtps2pi",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvttps2pi => (
                "x86_sse_mmxvalue_cvttps2pi",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvtpi2pd => (
                "x86_sse_mmxvalue_cvtpi2pd",
                &[Type::I64],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvtpd2pi => (
                "x86_sse_mmxvalue_cvtpd2pi",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86SseMmxvalueCvttpd2pi => (
                "x86_sse_mmxvalue_cvttpd2pi",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86Ssecvtdq2pd128 => (
                "x86_sse_cvtdq2pd128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Ssecvtdq2ps128 => (
                "x86_sse_cvtdq2ps128",
                &[Type::Vector(crate::VectorType::I32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtpd2dq128 => (
                "x86_sse_cvtpd2dq128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtpd2ps128 => (
                "x86_sse_cvtpd2ps128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtps2dq128 => (
                "x86_sse_cvtps2dq128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtps2pd128 => (
                "x86_sse_cvtps2pd128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Ssecvttpd2dq128 => (
                "x86_sse_cvttpd2dq128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Ssecvttps2dq128 => (
                "x86_sse_cvttps2dq128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86Ssecvtss2si32 => {
                ("x86_sse_cvtss2si32", &[Type::F32], &[Type::I32], false)
            }
            Intrinsic::X86Ssecvttss2si32 => {
                ("x86_sse_cvttss2si32", &[Type::F32], &[Type::I32], false)
            }
            Intrinsic::X86Ssecvtss2si64 => {
                ("x86_sse_cvtss2si64", &[Type::F32], &[Type::I64], false)
            }
            Intrinsic::X86Ssecvttss2si64 => {
                ("x86_sse_cvttss2si64", &[Type::F32], &[Type::I64], false)
            }
            Intrinsic::X86Ssecvtsd2si32 => {
                ("x86_sse_cvtsd2si32", &[Type::F64], &[Type::I32], false)
            }
            Intrinsic::X86Ssecvttsd2si32 => {
                ("x86_sse_cvttsd2si32", &[Type::F64], &[Type::I32], false)
            }
            Intrinsic::X86Ssecvtsd2si64 => {
                ("x86_sse_cvtsd2si64", &[Type::F64], &[Type::I64], false)
            }
            Intrinsic::X86Ssecvttsd2si64 => {
                ("x86_sse_cvttsd2si64", &[Type::F64], &[Type::I64], false)
            }
            Intrinsic::X86Ssecvtsi2ss32 => {
                ("x86_sse_cvtsi2ss32", &[Type::I32], &[Type::F32], false)
            }
            Intrinsic::X86Ssecvtsi2ss64 => {
                ("x86_sse_cvtsi2ss64", &[Type::I64], &[Type::F32], false)
            }
            Intrinsic::X86Ssecvtsi2sd32 => {
                ("x86_sse_cvtsi2sd32", &[Type::I32], &[Type::F64], false)
            }
            Intrinsic::X86Ssecvtsi2sd64 => {
                ("x86_sse_cvtsi2sd64", &[Type::I64], &[Type::F64], false)
            }
            Intrinsic::X86Ssecvtsd2ss => ("x86_sse_cvtsd2ss", &[Type::F64], &[Type::F32], false),
            Intrinsic::X86Ssecvtss2sd => ("x86_sse_cvtss2sd", &[Type::F32], &[Type::F64], false),
            Intrinsic::X86SseMinps128 => (
                "x86_sse_minps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseMinpd128 => (
                "x86_sse_minpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseMinss => (
                "x86_sse_minss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseMinsd => (
                "x86_sse_minsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseMaxps128 => (
                "x86_sse_maxps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseMaxpd128 => (
                "x86_sse_maxpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseMaxss => (
                "x86_sse_maxss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseMaxsd => (
                "x86_sse_maxsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseRcpps128 => (
                "x86_sse_rcpps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseRcpss => ("x86_sse_rcpss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86SseRsqrtps128 => (
                "x86_sse_rsqrtps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseRsqrtss => ("x86_sse_rsqrtss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86SseComiss => (
                "x86_sse_comiss",
                &[Type::F32, Type::F32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseUcomiss => (
                "x86_sse_ucomiss",
                &[Type::F32, Type::F32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseComisd => (
                "x86_sse_comisd",
                &[Type::F64, Type::F64],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseUcomisd => (
                "x86_sse_ucomisd",
                &[Type::F64, Type::F64],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseCmpps128 => (
                "x86_sse_cmpps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I32x4)],
                false,
            ),
            Intrinsic::X86SseCmppd128 => (
                "x86_sse_cmppd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I64x2)],
                false,
            ),
            Intrinsic::X86SseCmpss => (
                "x86_sse_cmpss",
                &[Type::F32, Type::F32, Type::I32],
                &[Type::I32],
                false,
            ),
            Intrinsic::X86SseCmpsd => (
                "x86_sse_cmpsd",
                &[Type::F64, Type::F64, Type::I32],
                &[Type::I64],
                false,
            ),
            Intrinsic::X86SseAddps128 => (
                "x86_sse_addps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseAddpd128 => (
                "x86_sse_addpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseAddss => (
                "x86_sse_addss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseAddsd => (
                "x86_sse_addsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseSubps128 => (
                "x86_sse_subps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseSubpd128 => (
                "x86_sse_subpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseSubss => (
                "x86_sse_subss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseSubsd => (
                "x86_sse_subsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseMulps128 => (
                "x86_sse_mulps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseMulpd128 => (
                "x86_sse_mulpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseMulss => (
                "x86_sse_mulss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseMulsd => (
                "x86_sse_mulsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseDivps128 => (
                "x86_sse_divps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseDivpd128 => (
                "x86_sse_divpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseDivss => (
                "x86_sse_divss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86SseDivsd => (
                "x86_sse_divsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86SseSqrtps128 => (
                "x86_sse_sqrtps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86SseSqrtpd128 => (
                "x86_sse_sqrtpd128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86SseSqrtss => ("x86_sse_sqrtss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86SseSqrtsd => ("x86_sse_sqrtsd", &[Type::F64], &[Type::F64], false),
            Intrinsic::X86SseMovmskps128 => (
                "x86_sse_movmskps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86SseMovmskpd128 => (
                "x86_sse_movmskpd128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86SsePmovmskb128 => (
                "x86_sse_pmovmskb128",
                &[Type::Vector(crate::VectorType::I8x16)],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86SseMaskmovdqu => (
                "x86_sse_maskmovdqu",
                &[
                    Type::Ptr,
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[],
                false,
            ),
            Intrinsic::X86Sse3Haddps => (
                "x86_sse3_haddps",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse3Haddpd => (
                "x86_sse3_haddpd",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse3Hsubps => (
                "x86_sse3_hsubps",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse3Hsubpd => (
                "x86_sse3_hsubpd",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse3Addsubps => (
                "x86_sse3_addsubps",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse3Addsubpd => (
                "x86_sse3_addsubpd",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse41Roundps128 => (
                "x86_sse41_roundps128",
                &[Type::Vector(crate::VectorType::F32x4), Type::I32],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse41Roundpd128 => (
                "x86_sse41_roundpd128",
                &[Type::Vector(crate::VectorType::F64x2), Type::I32],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse41Roundss => (
                "x86_sse41_roundss",
                &[Type::F32, Type::I32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86Sse41Roundsd => (
                "x86_sse41_roundsd",
                &[Type::F64, Type::I32],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86Sse41Dpps => (
                "x86_sse41_dpps",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86Sse41Dppd => (
                "x86_sse41_dppd",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86Sse41Ptest128 => (
                "x86_sse41_ptest128",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Crc32b => (
                "x86_sse42_crc32b",
                &[Type::I32, Type::I8],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Crc32w => (
                "x86_sse42_crc32w",
                &[Type::I32, Type::I16],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Crc32d => (
                "x86_sse42_crc32d",
                &[Type::I32, Type::I32],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Crc32q => (
                "x86_sse42_crc32q",
                &[Type::I32, Type::I64],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42PcmpistriIndex => (
                "x86_sse42_pcmpistri_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Pcmpistri64Index => (
                "x86_sse42_pcmpistri64_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Sse42PcmpistrmMask => (
                "x86_sse42_pcmpistrm_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse42PcmpistriFlags => (
                "x86_sse42_pcmpistri_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42PcmpestriIndex => (
                "x86_sse42_pcmpestri_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42PcmpestrmMask => (
                "x86_sse42_pcmpestrm_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse42PcmpestriFlags => (
                "x86_sse42_pcmpestri_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86Sse42Pcmpestri64Index => (
                "x86_sse42_pcmpestri64_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86Sse42Pcmpestrm64Mask => (
                "x86_sse42_pcmpestrm64_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86Sse42Pcmpestri64Flags => (
                "x86_sse42_pcmpestri64_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86FmaVfmadd132pd128 => (
                "x86_fma_vfmadd132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmadd132pd256 => (
                "x86_fma_vfmadd132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd132ps128 => (
                "x86_fma_vfmadd132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd132ps256 => (
                "x86_fma_vfmadd132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmadd132sd128 => (
                "x86_fma_vfmadd132sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmadd132ss128 => (
                "x86_fma_vfmadd132ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmadd213pd128 => (
                "x86_fma_vfmadd213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmadd213pd256 => (
                "x86_fma_vfmadd213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd213ps128 => (
                "x86_fma_vfmadd213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd213ps256 => (
                "x86_fma_vfmadd213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmadd213sd128 => (
                "x86_fma_vfmadd213sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmadd213ss128 => (
                "x86_fma_vfmadd213ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmadd231pd128 => (
                "x86_fma_vfmadd231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmadd231pd256 => (
                "x86_fma_vfmadd231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd231ps128 => (
                "x86_fma_vfmadd231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmadd231ps256 => (
                "x86_fma_vfmadd231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmadd231sd128 => (
                "x86_fma_vfmadd231sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmadd231ss128 => (
                "x86_fma_vfmadd231ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub132pd128 => (
                "x86_fma_vfmaddsub132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub132pd256 => (
                "x86_fma_vfmaddsub132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub213pd128 => (
                "x86_fma_vfmaddsub213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub213pd256 => (
                "x86_fma_vfmaddsub213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub231pd128 => (
                "x86_fma_vfmaddsub231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub231pd256 => (
                "x86_fma_vfmaddsub231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub132ps128 => (
                "x86_fma_vfmaddsub132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub132ps256 => (
                "x86_fma_vfmaddsub132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub213ps128 => (
                "x86_fma_vfmaddsub213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub213ps256 => (
                "x86_fma_vfmaddsub213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub231ps128 => (
                "x86_fma_vfmaddsub231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmaddsub231ps256 => (
                "x86_fma_vfmaddsub231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd132pd128 => (
                "x86_fma_vfmsubadd132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd132pd256 => (
                "x86_fma_vfmsubadd132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd213pd128 => (
                "x86_fma_vfmsubadd213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd213pd256 => (
                "x86_fma_vfmsubadd213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd231pd128 => (
                "x86_fma_vfmsubadd231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd231pd256 => (
                "x86_fma_vfmsubadd231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd132ps128 => (
                "x86_fma_vfmsubadd132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd132ps256 => (
                "x86_fma_vfmsubadd132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd213ps128 => (
                "x86_fma_vfmsubadd213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd213ps256 => (
                "x86_fma_vfmsubadd213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd231ps128 => (
                "x86_fma_vfmsubadd231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsubadd231ps256 => (
                "x86_fma_vfmsubadd231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132pd128 => (
                "x86_fma_vfmsub132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132pd256 => (
                "x86_fma_vfmsub132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132ps128 => (
                "x86_fma_vfmsub132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132ps256 => (
                "x86_fma_vfmsub132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsub132sd128 => (
                "x86_fma_vfmsub132sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmsub132ss128 => (
                "x86_fma_vfmsub132ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmsub213pd128 => (
                "x86_fma_vfmsub213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsub213pd256 => (
                "x86_fma_vfmsub213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub213ps128 => (
                "x86_fma_vfmsub213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub213ps256 => (
                "x86_fma_vfmsub213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsub213sd128 => (
                "x86_fma_vfmsub213sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmsub213ss128 => (
                "x86_fma_vfmsub213ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfmsub231pd128 => (
                "x86_fma_vfmsub231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfmsub231pd256 => (
                "x86_fma_vfmsub231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub231ps128 => (
                "x86_fma_vfmsub231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfmsub231ps256 => (
                "x86_fma_vfmsub231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfmsub231sd128 => (
                "x86_fma_vfmsub231sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfmsub231ss128 => (
                "x86_fma_vfmsub231ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132pd128 => (
                "x86_fma_vfnmadd132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132pd256 => (
                "x86_fma_vfnmadd132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132ps128 => (
                "x86_fma_vfnmadd132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132ps256 => (
                "x86_fma_vfnmadd132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132sd128 => (
                "x86_fma_vfnmadd132sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmadd132ss128 => (
                "x86_fma_vfnmadd132ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213pd128 => (
                "x86_fma_vfnmadd213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213pd256 => (
                "x86_fma_vfnmadd213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213ps128 => (
                "x86_fma_vfnmadd213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213ps256 => (
                "x86_fma_vfnmadd213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213sd128 => (
                "x86_fma_vfnmadd213sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmadd213ss128 => (
                "x86_fma_vfnmadd213ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231pd128 => (
                "x86_fma_vfnmadd231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231pd256 => (
                "x86_fma_vfnmadd231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231ps128 => (
                "x86_fma_vfnmadd231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231ps256 => (
                "x86_fma_vfnmadd231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231sd128 => (
                "x86_fma_vfnmadd231sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmadd231ss128 => (
                "x86_fma_vfnmadd231ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132pd128 => (
                "x86_fma_vfnmsub132pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132pd256 => (
                "x86_fma_vfnmsub132pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132ps128 => (
                "x86_fma_vfnmsub132ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132ps256 => (
                "x86_fma_vfnmsub132ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132sd128 => (
                "x86_fma_vfnmsub132sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmsub132ss128 => (
                "x86_fma_vfnmsub132ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213pd128 => (
                "x86_fma_vfnmsub213pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213pd256 => (
                "x86_fma_vfnmsub213pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213ps128 => (
                "x86_fma_vfnmsub213ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213ps256 => (
                "x86_fma_vfnmsub213ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213sd128 => (
                "x86_fma_vfnmsub213sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmsub213ss128 => (
                "x86_fma_vfnmsub213ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231pd128 => (
                "x86_fma_vfnmsub231pd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231pd256 => (
                "x86_fma_vfnmsub231pd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231ps128 => (
                "x86_fma_vfnmsub231ps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231ps256 => (
                "x86_fma_vfnmsub231ps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231sd128 => (
                "x86_fma_vfnmsub231sd128",
                &[Type::F64, Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86FmaVfnmsub231ss128 => (
                "x86_fma_vfnmsub231ss128",
                &[Type::F32, Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxPcmpistriIndex => (
                "x86_avx_vpcmpistri_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxPcmpistri64Index => (
                "x86_avx_vpcmpistri64_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86AvxPcmpistrmMask => (
                "x86_avx_vpcmpistrm_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxPcmpistriFlags => (
                "x86_avx_vpcmpistri_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxPcmpestriIndex => (
                "x86_avx_vpcmpestri_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxPcmpestrmMask => (
                "x86_avx_vpcmpestrm_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxPcmpestriFlags => (
                "x86_avx_vpcmpestri_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxPcmpestri64Index => (
                "x86_avx_vpcmpestri64_index",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::I64],
                true,
            ),
            Intrinsic::X86AvxPcmpestrm64Mask => (
                "x86_avx_vpcmpestrm64_mask",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::I8x16)],
                true,
            ),
            Intrinsic::X86AvxPcmpestri64Flags => (
                "x86_avx_vpcmpestri64_flags",
                &[
                    Type::Vector(crate::VectorType::I8x16),
                    Type::Vector(crate::VectorType::I8x16),
                    Type::I64,
                    Type::I64,
                    Type::I32,
                ],
                &[Type::I32],
                true,
            ),
            Intrinsic::X86AvxVaddps128 => (
                "x86_avx_vaddps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVaddpd128 => (
                "x86_avx_vaddpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVaddss => (
                "x86_avx_vaddss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVaddsd => (
                "x86_avx_vaddsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVaddps256 => (
                "x86_avx_vaddps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVaddpd256 => (
                "x86_avx_vaddpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVsubps128 => (
                "x86_avx_vsubps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVsubpd128 => (
                "x86_avx_vsubpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVsubss => (
                "x86_avx_vsubss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVsubsd => (
                "x86_avx_vsubsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVsubps256 => (
                "x86_avx_vsubps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVsubpd256 => (
                "x86_avx_vsubpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVmulps128 => (
                "x86_avx_vmulps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVmulpd128 => (
                "x86_avx_vmulpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVmulss => (
                "x86_avx_vmulss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVmulsd => (
                "x86_avx_vmulsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVmulps256 => (
                "x86_avx_vmulps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVmulpd256 => (
                "x86_avx_vmulpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVdivps128 => (
                "x86_avx_vdivps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVdivpd128 => (
                "x86_avx_vdivpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVdivss => (
                "x86_avx_vdivss",
                &[Type::F32, Type::F32],
                &[Type::F32],
                false,
            ),
            Intrinsic::X86AvxVdivsd => (
                "x86_avx_vdivsd",
                &[Type::F64, Type::F64],
                &[Type::F64],
                false,
            ),
            Intrinsic::X86AvxVdivps256 => (
                "x86_avx_vdivps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVdivpd256 => (
                "x86_avx_vdivpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVsqrtps128 => (
                "x86_avx_vsqrtps128",
                &[Type::Vector(crate::VectorType::F32x4)],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVsqrtpd128 => (
                "x86_avx_vsqrtpd128",
                &[Type::Vector(crate::VectorType::F64x2)],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVsqrtss => ("x86_avx_vsqrtss", &[Type::F32], &[Type::F32], false),
            Intrinsic::X86AvxVsqrtsd => ("x86_avx_vsqrtsd", &[Type::F64], &[Type::F64], false),
            Intrinsic::X86AvxVsqrtps256 => (
                "x86_avx_vsqrtps256",
                &[Type::Vector(crate::VectorType::F32x8)],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVsqrtpd256 => (
                "x86_avx_vsqrtpd256",
                &[Type::Vector(crate::VectorType::F64x4)],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVhaddps128 => (
                "x86_avx_vhaddps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVhaddps256 => (
                "x86_avx_vhaddps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVhaddpd128 => (
                "x86_avx_vhaddpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVhaddpd256 => (
                "x86_avx_vhaddpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVhsubps128 => (
                "x86_avx_vhsubps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVhsubps256 => (
                "x86_avx_vhsubps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVhsubpd128 => (
                "x86_avx_vhsubpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVhsubpd256 => (
                "x86_avx_vhsubpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVaddsubps128 => (
                "x86_avx_vaddsubps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVaddsubps256 => (
                "x86_avx_vaddsubps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVaddsubpd128 => (
                "x86_avx_vaddsubpd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
            Intrinsic::X86AvxVaddsubpd256 => (
                "x86_avx_vaddsubpd256",
                &[
                    Type::Vector(crate::VectorType::F64x4),
                    Type::Vector(crate::VectorType::F64x4),
                ],
                &[Type::Vector(crate::VectorType::F64x4)],
                false,
            ),
            Intrinsic::X86AvxVdpps128 => (
                "x86_avx_vdpps128",
                &[
                    Type::Vector(crate::VectorType::F32x4),
                    Type::Vector(crate::VectorType::F32x4),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x4)],
                false,
            ),
            Intrinsic::X86AvxVdpps256 => (
                "x86_avx_vdpps256",
                &[
                    Type::Vector(crate::VectorType::F32x8),
                    Type::Vector(crate::VectorType::F32x8),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F32x8)],
                false,
            ),
            Intrinsic::X86AvxVdppd128 => (
                "x86_avx_vdppd128",
                &[
                    Type::Vector(crate::VectorType::F64x2),
                    Type::Vector(crate::VectorType::F64x2),
                    Type::I32,
                ],
                &[Type::Vector(crate::VectorType::F64x2)],
                false,
            ),
        };
        IntrinsicInfo {
            name,
            params,
            returns,
            pure,
        }
    }

    /// These target operations observe MXCSR and may set floating exception
    /// flags. They are pinned even when their SSA result is unused.
    pub fn is_x86_f16c(self) -> bool {
        matches!(
            self,
            Self::X86F16cPhToPs128
                | Self::X86F16cPhToPs256
                | Self::X86F16cPsToPh128
                | Self::X86F16cPsToPh256
        )
    }
    pub fn has_f16c_rounding(self) -> bool {
        matches!(self, Self::X86F16cPsToPh128 | Self::X86F16cPsToPh256)
    }

    pub fn is_x86_bmi(self) -> bool {
        matches!(
            self,
            Self::X86Bmi1Bextr32
                | Self::X86Bmi1Bextr64
                | Self::X86Bmi2Pdep32
                | Self::X86Bmi2Pdep64
                | Self::X86Bmi2Pext32
                | Self::X86Bmi2Pext64
        )
    }
    pub fn is_x86_cache_stream(self) -> bool {
        matches!(
            self,
            Self::X86Clflush
                | Self::X86Prefetchnta
                | Self::X86Prefetcht0
                | Self::X86Prefetcht1
                | Self::X86Prefetcht2
                | Self::X86SseMovntps128
                | Self::X86SseMovntpd128
                | Self::X86SseMovntdq128
                | Self::X86AvxVmovntps128
                | Self::X86AvxVmovntps256
                | Self::X86AvxVmovntpd128
                | Self::X86AvxVmovntpd256
                | Self::X86AvxVmovntdq128
                | Self::X86AvxVmovntdq256
                | Self::X86Movnti32
                | Self::X86Movnti64
                | Self::X86Sse41Movntdqa128
                | Self::X86Avx2Vmovntdqa256
        )
    }
    pub fn is_x86_byte_shuffle(self) -> bool {
        matches!(
            self,
            Self::X86Ssse3Pshufb128 | Self::X86AvxVpshufb128 | Self::X86Avx2Vpshufb256
        )
    }
    /// Opcode, per-lane-count form, and feature tier (SSE2=0/AVX=1/AVX2=2).
    pub fn x86_packed_shift(self) -> Option<(&'static str, bool, u8)> {
        match self {
            Self::X86Sse2Psllw128 => Some(("psllw", false, 0)),
            Self::X86Sse2Pslld128 => Some(("pslld", false, 0)),
            Self::X86Sse2Psllq128 => Some(("psllq", false, 0)),
            Self::X86Sse2Psrlw128 => Some(("psrlw", false, 0)),
            Self::X86Sse2Psrld128 => Some(("psrld", false, 0)),
            Self::X86Sse2Psrlq128 => Some(("psrlq", false, 0)),
            Self::X86Sse2Psraw128 => Some(("psraw", false, 0)),
            Self::X86Sse2Psrad128 => Some(("psrad", false, 0)),
            Self::X86AvxVpsllw128 => Some(("vpsllw", false, 1)),
            Self::X86AvxVpslld128 => Some(("vpslld", false, 1)),
            Self::X86AvxVpsllq128 => Some(("vpsllq", false, 1)),
            Self::X86AvxVpsrlw128 => Some(("vpsrlw", false, 1)),
            Self::X86AvxVpsrld128 => Some(("vpsrld", false, 1)),
            Self::X86AvxVpsrlq128 => Some(("vpsrlq", false, 1)),
            Self::X86AvxVpsraw128 => Some(("vpsraw", false, 1)),
            Self::X86AvxVpsrad128 => Some(("vpsrad", false, 1)),
            Self::X86Avx2Vpsllw256 => Some(("vpsllw", false, 2)),
            Self::X86Avx2Vpslld256 => Some(("vpslld", false, 2)),
            Self::X86Avx2Vpsllq256 => Some(("vpsllq", false, 2)),
            Self::X86Avx2Vpsrlw256 => Some(("vpsrlw", false, 2)),
            Self::X86Avx2Vpsrld256 => Some(("vpsrld", false, 2)),
            Self::X86Avx2Vpsrlq256 => Some(("vpsrlq", false, 2)),
            Self::X86Avx2Vpsraw256 => Some(("vpsraw", false, 2)),
            Self::X86Avx2Vpsrad256 => Some(("vpsrad", false, 2)),
            Self::X86Avx2Vpsllvd128 => Some(("vpsllvd", true, 2)),
            Self::X86Avx2Vpsllvq128 => Some(("vpsllvq", true, 2)),
            Self::X86Avx2Vpsrlvd128 => Some(("vpsrlvd", true, 2)),
            Self::X86Avx2Vpsrlvq128 => Some(("vpsrlvq", true, 2)),
            Self::X86Avx2Vpsravd128 => Some(("vpsravd", true, 2)),
            Self::X86Avx2Vpsllvd256 => Some(("vpsllvd", true, 2)),
            Self::X86Avx2Vpsllvq256 => Some(("vpsllvq", true, 2)),
            Self::X86Avx2Vpsrlvd256 => Some(("vpsrlvd", true, 2)),
            Self::X86Avx2Vpsrlvq256 => Some(("vpsrlvq", true, 2)),
            Self::X86Avx2Vpsravd256 => Some(("vpsravd", true, 2)),
            _ => None,
        }
    }
    /// Byte-lane shifts take a verified immediate0..255; halves stay independent.
    pub fn x86_byte_shift(self) -> Option<(&'static str, u8)> {
        match self {
            Self::X86Sse2Pslldq128 => Some(("pslldq", 0)),
            Self::X86Sse2Psrldq128 => Some(("psrldq", 0)),
            Self::X86AvxVpslldq128 => Some(("vpslldq", 1)),
            Self::X86AvxVpsrldq128 => Some(("vpsrldq", 1)),
            Self::X86Avx2Vpslldq256 => Some(("vpslldq", 2)),
            Self::X86Avx2Vpsrldq256 => Some(("vpsrldq", 2)),
            _ => None,
        }
    }
    /// Pure numerical register contract: eight sliding four-byte SADs per 128-bit
    /// half, or unsigned minimum/lowest index with zero result padding. SAD imm8
    /// uses bits 2:0 in the low half and 5:3 in the high half; other bits ignored.
    /// Returns opcode, independent feature tier (2=SSE41,3=AVX,4=AVX+AVX2), SAD.
    pub fn x86_packed_search(self) -> Option<(&'static str, u8, bool)> {
        match self {
            Self::X86Sse41Mpsadbw128 => Some(("mpsadbw", 2, true)),
            Self::X86AvxVmpsadbw128 => Some(("vmpsadbw", 3, true)),
            Self::X86Avx2Vmpsadbw256 => Some(("vmpsadbw", 4, true)),
            Self::X86Sse41Phminposuw128 => Some(("phminposuw", 2, false)),
            Self::X86AvxVphminposuw128 => Some(("vphminposuw", 3, false)),
            _ => None,
        }
    }
    /// Exact pairwise/sliced integer operation; tier SSE2/SSSE3/SSE41/AVX/AVX2.
    /// Opcode and feature tier: SSE4.1 / AVX / AVX+AVX2.
    pub fn x86_sign_blend(self) -> Option<(&'static str, u8)> {
        match self {
            Self::X86Sse41Blendvps128 => Some(("blendvps", 0)),
            Self::X86Sse41Blendvpd128 => Some(("blendvpd", 0)),
            Self::X86Sse41Pblendvb128 => Some(("pblendvb", 0)),
            Self::X86AvxVblendvps128 => Some(("vblendvps", 1)),
            Self::X86AvxVblendvpd128 => Some(("vblendvpd", 1)),
            Self::X86AvxVpblendvb128 => Some(("vpblendvb", 1)),
            Self::X86AvxVblendvps256 => Some(("vblendvps", 1)),
            Self::X86AvxVblendvpd256 => Some(("vblendvpd", 1)),
            Self::X86Avx2Vpblendvb256 => Some(("vpblendvb", 2)),
            _ => None,
        }
    }
    pub fn x86_packed_pair(self) -> Option<(&'static str, u8)> {
        match self {
            Self::X86Sse2Packsswb128 => Some(("packsswb", 0)),
            Self::X86Sse2Packssdw128 => Some(("packssdw", 0)),
            Self::X86Sse2Packuswb128 => Some(("packuswb", 0)),
            Self::X86Sse41Packusdw128 => Some(("packusdw", 2)),
            Self::X86AvxVpacksswb128 => Some(("vpacksswb", 3)),
            Self::X86AvxVpackssdw128 => Some(("vpackssdw", 3)),
            Self::X86AvxVpackuswb128 => Some(("vpackuswb", 3)),
            Self::X86AvxVpackusdw128 => Some(("vpackusdw", 3)),
            Self::X86Avx2Vpacksswb256 => Some(("vpacksswb", 4)),
            Self::X86Avx2Vpackssdw256 => Some(("vpackssdw", 4)),
            Self::X86Avx2Vpackuswb256 => Some(("vpackuswb", 4)),
            Self::X86Avx2Vpackusdw256 => Some(("vpackusdw", 4)),
            Self::X86Ssse3Phaddw128 => Some(("phaddw", 1)),
            Self::X86Ssse3Phaddd128 => Some(("phaddd", 1)),
            Self::X86Ssse3Phaddsw128 => Some(("phaddsw", 1)),
            Self::X86Ssse3Phsubw128 => Some(("phsubw", 1)),
            Self::X86Ssse3Phsubd128 => Some(("phsubd", 1)),
            Self::X86Ssse3Phsubsw128 => Some(("phsubsw", 1)),
            Self::X86Sse2Paddsb128 => Some(("paddsb", 0)),
            Self::X86Sse2Paddsw128 => Some(("paddsw", 0)),
            Self::X86Sse2Paddusb128 => Some(("paddusb", 0)),
            Self::X86Sse2Paddusw128 => Some(("paddusw", 0)),
            Self::X86Sse2Psubsb128 => Some(("psubsb", 0)),
            Self::X86Sse2Psubsw128 => Some(("psubsw", 0)),
            Self::X86Sse2Psubusb128 => Some(("psubusb", 0)),
            Self::X86Sse2Psubusw128 => Some(("psubusw", 0)),
            Self::X86Sse2Pavgb128 => Some(("pavgb", 0)),
            Self::X86Sse2Pavgw128 => Some(("pavgw", 0)),
            Self::X86Ssse3Psignb128 => Some(("psignb", 1)),
            Self::X86Ssse3Psignw128 => Some(("psignw", 1)),
            Self::X86Ssse3Psignd128 => Some(("psignd", 1)),
            Self::X86Ssse3Pabsb128 => Some(("pabsb", 1)),
            Self::X86Ssse3Pabsw128 => Some(("pabsw", 1)),
            Self::X86Ssse3Pabsd128 => Some(("pabsd", 1)),
            Self::X86AvxVphaddw128 => Some(("vphaddw", 3)),
            Self::X86AvxVphaddd128 => Some(("vphaddd", 3)),
            Self::X86AvxVphaddsw128 => Some(("vphaddsw", 3)),
            Self::X86AvxVphsubw128 => Some(("vphsubw", 3)),
            Self::X86AvxVphsubd128 => Some(("vphsubd", 3)),
            Self::X86AvxVphsubsw128 => Some(("vphsubsw", 3)),
            Self::X86AvxVpaddsb128 => Some(("vpaddsb", 3)),
            Self::X86AvxVpaddsw128 => Some(("vpaddsw", 3)),
            Self::X86AvxVpaddusb128 => Some(("vpaddusb", 3)),
            Self::X86AvxVpaddusw128 => Some(("vpaddusw", 3)),
            Self::X86AvxVpsubsb128 => Some(("vpsubsb", 3)),
            Self::X86AvxVpsubsw128 => Some(("vpsubsw", 3)),
            Self::X86AvxVpsubusb128 => Some(("vpsubusb", 3)),
            Self::X86AvxVpsubusw128 => Some(("vpsubusw", 3)),
            Self::X86AvxVpavgb128 => Some(("vpavgb", 3)),
            Self::X86AvxVpavgw128 => Some(("vpavgw", 3)),
            Self::X86AvxVpsignb128 => Some(("vpsignb", 3)),
            Self::X86AvxVpsignw128 => Some(("vpsignw", 3)),
            Self::X86AvxVpsignd128 => Some(("vpsignd", 3)),
            Self::X86AvxVpabsb128 => Some(("vpabsb", 3)),
            Self::X86AvxVpabsw128 => Some(("vpabsw", 3)),
            Self::X86AvxVpabsd128 => Some(("vpabsd", 3)),
            Self::X86Avx2Vphaddw256 => Some(("vphaddw", 4)),
            Self::X86Avx2Vphaddd256 => Some(("vphaddd", 4)),
            Self::X86Avx2Vphaddsw256 => Some(("vphaddsw", 4)),
            Self::X86Avx2Vphsubw256 => Some(("vphsubw", 4)),
            Self::X86Avx2Vphsubd256 => Some(("vphsubd", 4)),
            Self::X86Avx2Vphsubsw256 => Some(("vphsubsw", 4)),
            Self::X86Avx2Vpaddsb256 => Some(("vpaddsb", 4)),
            Self::X86Avx2Vpaddsw256 => Some(("vpaddsw", 4)),
            Self::X86Avx2Vpaddusb256 => Some(("vpaddusb", 4)),
            Self::X86Avx2Vpaddusw256 => Some(("vpaddusw", 4)),
            Self::X86Avx2Vpsubsb256 => Some(("vpsubsb", 4)),
            Self::X86Avx2Vpsubsw256 => Some(("vpsubsw", 4)),
            Self::X86Avx2Vpsubusb256 => Some(("vpsubusb", 4)),
            Self::X86Avx2Vpsubusw256 => Some(("vpsubusw", 4)),
            Self::X86Avx2Vpavgb256 => Some(("vpavgb", 4)),
            Self::X86Avx2Vpavgw256 => Some(("vpavgw", 4)),
            Self::X86Avx2Vpsignb256 => Some(("vpsignb", 4)),
            Self::X86Avx2Vpsignw256 => Some(("vpsignw", 4)),
            Self::X86Avx2Vpsignd256 => Some(("vpsignd", 4)),
            Self::X86Avx2Vpabsb256 => Some(("vpabsb", 4)),
            Self::X86Avx2Vpabsw256 => Some(("vpabsw", 4)),
            Self::X86Avx2Vpabsd256 => Some(("vpabsd", 4)),
            Self::X86Sse2Pmaddwd128 => Some(("pmaddwd", 0)),
            Self::X86Ssse3Pmaddubsw128 => Some(("pmaddubsw", 1)),
            Self::X86Sse2Pmulhw128 => Some(("pmulhw", 0)),
            Self::X86Sse2Pmulhuw128 => Some(("pmulhuw", 0)),
            Self::X86Ssse3Pmulhrsw128 => Some(("pmulhrsw", 1)),
            Self::X86Sse41Pmuldq128 => Some(("pmuldq", 2)),
            Self::X86Sse2Pmuludq128 => Some(("pmuludq", 0)),
            Self::X86Sse2Psadbw128 => Some(("psadbw", 0)),
            Self::X86AvxVpmaddwd128 => Some(("vpmaddwd", 3)),
            Self::X86AvxVpmaddubsw128 => Some(("vpmaddubsw", 3)),
            Self::X86AvxVpmulhw128 => Some(("vpmulhw", 3)),
            Self::X86AvxVpmulhuw128 => Some(("vpmulhuw", 3)),
            Self::X86AvxVpmulhrsw128 => Some(("vpmulhrsw", 3)),
            Self::X86AvxVpmuldq128 => Some(("vpmuldq", 3)),
            Self::X86AvxVpmuludq128 => Some(("vpmuludq", 3)),
            Self::X86AvxVpsadbw128 => Some(("vpsadbw", 3)),
            Self::X86Avx2Vpmaddwd256 => Some(("vpmaddwd", 4)),
            Self::X86Avx2Vpmaddubsw256 => Some(("vpmaddubsw", 4)),
            Self::X86Avx2Vpmulhw256 => Some(("vpmulhw", 4)),
            Self::X86Avx2Vpmulhuw256 => Some(("vpmulhuw", 4)),
            Self::X86Avx2Vpmulhrsw256 => Some(("vpmulhrsw", 4)),
            Self::X86Avx2Vpmuldq256 => Some(("vpmuldq", 4)),
            Self::X86Avx2Vpmuludq256 => Some(("vpmuludq", 4)),
            Self::X86Avx2Vpsadbw256 => Some(("vpsadbw", 4)),
            _ => None,
        }
    }
    /// Single-step architectural numerical operations, with raw post-op status.
    pub fn is_x86_x87_constant(self) -> bool {
        matches!(
            self,
            Self::X86X87Fldpi
                | Self::X86X87Fldl2e
                | Self::X86X87Fldl2t
                | Self::X86X87Fldlg2
                | Self::X86X87Fldln2
        )
    }

    pub fn is_x86_x87_bcd(self) -> bool {
        matches!(self, Self::X86X87Fbld | Self::X86X87Fbstp)
    }

    pub fn is_x86_x87_integer(self) -> bool {
        matches!(
            self,
            Self::X86X87Fist16
                | Self::X86X87Fist32
                | Self::X86X87Fistp16
                | Self::X86X87Fistp32
                | Self::X86X87Fistp64
                | Self::X86X87Fisttp16
                | Self::X86X87Fisttp32
                | Self::X86X87Fisttp64
                | Self::X86X87Fild16
                | Self::X86X87Fild32
                | Self::X86X87Fild64
        )
    }
    pub fn is_x86_x87_fi_arithmetic(self) -> bool {
        matches!(
            self,
            Self::X86X87Fiadd16
                | Self::X86X87Fiadd32
                | Self::X86X87Fimul16
                | Self::X86X87Fimul32
                | Self::X86X87Fisub16
                | Self::X86X87Fisub32
                | Self::X86X87Fisubr16
                | Self::X86X87Fisubr32
                | Self::X86X87Fidiv16
                | Self::X86X87Fidiv32
                | Self::X86X87Fidivr16
                | Self::X86X87Fidivr32
        )
    }
    pub fn is_x86_x87_fisttp(self) -> bool {
        matches!(
            self,
            Self::X86X87Fisttp16 | Self::X86X87Fisttp32 | Self::X86X87Fisttp64
        )
    }
    /// Pinned raw relation/flags operations, with immediate post-op status.
    pub fn is_x86_x87_compare(self) -> bool {
        matches!(
            self,
            Self::X86X87Fcom
                | Self::X86X87Fcomp
                | Self::X86X87Fcompp
                | Self::X86X87Fucom
                | Self::X86X87Fucomp
                | Self::X86X87Fucompp
                | Self::X86X87Fcomi
                | Self::X86X87Fcomip
                | Self::X86X87Fucomi
                | Self::X86X87Fucomip
        )
    }
    pub fn is_x86_x87_binary(self) -> bool {
        matches!(
            self,
            Self::X86X87FaddSt1
                | Self::X86X87FmulSt1
                | Self::X86X87FsubSt1
                | Self::X86X87FsubrSt1
                | Self::X86X87FdivSt1
                | Self::X86X87FdivrSt1
                | Self::X86X87Fadd
                | Self::X86X87Fmul
                | Self::X86X87Fsub
                | Self::X86X87Fsubr
                | Self::X86X87Fdiv
                | Self::X86X87Fdivr
                | Self::X86X87Faddp
                | Self::X86X87Fmulp
                | Self::X86X87Fsubp
                | Self::X86X87Fsubrp
                | Self::X86X87Fdivp
                | Self::X86X87Fdivrp
        )
    }
    pub fn is_x86_x87_basic(self) -> bool {
        matches!(
            self,
            Self::X86X87Fabs
                | Self::X86X87Fchs
                | Self::X86X87Fsqrt
                | Self::X86X87Frndint
                | Self::X86X87Ftst
                | Self::X86X87Fxam
        )
    }
    pub fn is_x86_x87_numerical(self) -> bool {
        matches!(
            self,
            Self::X86X87Fsincos
                | Self::X86X87Fptan
                | Self::X86X87Fxtract
                | Self::X86X87Fsin
                | Self::X86X87Fcos
                | Self::X86X87F2xm1
                | Self::X86X87Fpatan
                | Self::X86X87Fyl2x
                | Self::X86X87Fyl2xp1
                | Self::X86X87Fscale
                | Self::X86X87Fprem
                | Self::X86X87Fprem1
        ) || self.is_x86_x87_basic()
            || self.is_x86_x87_binary()
            || self.is_x86_x87_integer()
            || self.is_x86_x87_bcd()
            || self.is_x86_x87_constant()
            || self.is_x86_x87_fi_arithmetic()
            || self.is_x86_x87_compare()
    }
    /// (opcode, operand-size bits, exact memory footprint). These are pinned raw
    /// operations under docs/f80/legacy-images.md, not FXSAVE-format adapters.
    pub fn x86_legacy_x87_image(self) -> Option<(&'static str, u16, usize)> {
        Some(match self {
            Self::X86X87Fnstenv16 => ("fnstenv", 16, 14),
            Self::X86X87Fnstenv32 => ("fnstenv", 32, 28),
            Self::X86X87Fstenv16 => ("fstenv", 16, 14),
            Self::X86X87Fstenv32 => ("fstenv", 32, 28),
            Self::X86X87Fldenv16 => ("fldenv", 16, 14),
            Self::X86X87Fldenv32 => ("fldenv", 32, 28),
            Self::X86X87Fnsave16 => ("fnsave", 16, 94),
            Self::X86X87Fnsave32 => ("fnsave", 32, 108),
            Self::X86X87Fsave16 => ("fsave", 16, 94),
            Self::X86X87Fsave32 => ("fsave", 32, 108),
            Self::X86X87Frstor16 => ("frstor", 16, 94),
            Self::X86X87Frstor32 => ("frstor", 32, 108),
            _ => return None,
        })
    }

    /// Raw 2w/w architectural division, quotient then remainder. See docs/x86-raw-div.md.
    pub fn x86_raw_div(self) -> Option<(u16, bool)> {
        match self {
            Self::X86DivU8 => Some((8, false)),
            Self::X86DivU16 => Some((16, false)),
            Self::X86DivU32 => Some((32, false)),
            Self::X86DivU64 => Some((64, false)),
            Self::X86DivS8 => Some((8, true)),
            Self::X86DivS16 => Some((16, true)),
            Self::X86DivS32 => Some((32, true)),
            Self::X86DivS64 => Some((64, true)),
            _ => None,
        }
    }

    /// (output, element bits, REP, explicit owned terminal FLAGS context).
    pub fn x86_string_port_io(self) -> Option<(bool, u16, bool, bool)> {
        Some(match self {
            Self::X86Ins8 => (false, 8, false, false),
            Self::X86Ins16 => (false, 16, false, false),
            Self::X86Ins32 => (false, 32, false, false),
            Self::X86Outs8 => (true, 8, false, false),
            Self::X86Outs16 => (true, 16, false, false),
            Self::X86Outs32 => (true, 32, false, false),
            Self::X86RepIns8 => (false, 8, true, false),
            Self::X86RepIns16 => (false, 16, true, false),
            Self::X86RepIns32 => (false, 32, true, false),
            Self::X86RepOuts8 => (true, 8, true, false),
            Self::X86RepOuts16 => (true, 16, true, false),
            Self::X86RepOuts32 => (true, 32, true, false),
            Self::X86Ins8OwnedTerminal => (false, 8, false, true),
            Self::X86Ins16OwnedTerminal => (false, 16, false, true),
            Self::X86Ins32OwnedTerminal => (false, 32, false, true),
            Self::X86Outs8OwnedTerminal => (true, 8, false, true),
            Self::X86Outs16OwnedTerminal => (true, 16, false, true),
            Self::X86Outs32OwnedTerminal => (true, 32, false, true),
            Self::X86RepIns8OwnedTerminal => (false, 8, true, true),
            Self::X86RepIns16OwnedTerminal => (false, 16, true, true),
            Self::X86RepIns32OwnedTerminal => (false, 32, true, true),
            Self::X86RepOuts8OwnedTerminal => (true, 8, true, true),
            Self::X86RepOuts16OwnedTerminal => (true, 16, true, true),
            Self::X86RepOuts32OwnedTerminal => (true, 32, true, true),
            _ => return None,
        })
    }

    pub fn is_x86_target(self) -> bool {
        if matches!(self, x86_system_intrinsics!(x86_64)) {
            return true;
        }
        matches!(
            self,
            Self::X86RflagsRead
                | Self::X86RflagsSwap
                | Self::X86X87Region
                | Self::X86Xrstor64
                | Self::X86Xsave64
                | Self::X86Lahf
                | Self::X86SahfExchange
                | Self::X86Fxsave64
                | Self::X86Fxrstor64
                | Self::X86X87ReadCwNowait
                | Self::X86X87ReadSwNowait
                | Self::X86X87ReadCwWait
                | Self::X86X87ReadSwWait
                | Self::X86X87ExchangeCw
                | Self::X86X87ClexNowait
                | Self::X86X87ClexWait
                | Self::X86X87InitNowait
                | Self::X86X87InitWait
                | Self::X86X87Wait
                | Self::X86MmxEmms
                | Self::X86Xgetbv0
                | Self::X86Cpuid
                | Self::X86Rdtsc
                | Self::X86Rdpmc
                | Self::X86Lfence
                | Self::X86Sfence
                | Self::X86Mfence
                | Self::X86Pause
        ) || self.x86_system_register().is_some()
            || self.x86_system_write().is_some()
            || self.x86_descriptor_query().is_some()
            || self.x86_selector_query().is_some()
            || self.x86_port_io().is_some()
            || self.x86_string_port_io().is_some()
            || self.x86_raw_div().is_some()
            || self.x86_legacy_x87_image().is_some()
            || self.x86_owned_x87_stack().is_some()
            || self.is_x86_x87_numerical()
            || self.x86_packed_search().is_some()
            || self.x86_sign_blend().is_some()
            || self.x86_packed_pair().is_some()
            || self.x86_packed_shift().is_some()
            || self.x86_byte_shift().is_some()
            || self.is_x86_byte_shuffle()
            || self.is_x86_cache_stream()
            || self == Self::X86AvxVzeroall
            || matches!(self, Self::X86SseReadMxcsr | Self::X86SseSwapMxcsr)
            || self.is_x86_mmx_value_convert()
            || self.is_x86_raw_fma()
            || self == Self::X86Cx16Cmpxchg16b
            || self.is_x86_f16c()
            || self.is_x86_bmi()
            || self.is_x86_gather()
            || self.is_x86_masked_mem()
            || self.is_x86_raw_fp_convert()
            || self.is_x86_fp_compare()
            || self.x86_aes_opcode().is_some()
            || self.is_x86_pure_bits()
            || self.is_x86_fp_misc()
            || self == Self::X86AvxVmaskmovdqu
            || self.is_x86_legacy_raw_fp_convert()
            || self.is_x86_legacy_fp_misc()
            || self.is_x86_legacy_fp_compare()
            || self.is_x86_legacy_sign_mask()
            || self == Self::X86SseMaskmovdqu
            || self.is_x86_legacy_fp_v2()
            || self == Self::X86Pclmulqdq128
            || self.is_x86_crc32()
            || self.is_x86_legacy_ptest()
            || self.is_x86_string()
            || self.is_x86_fp_horizontal_dot()
    }

    /// Inactive sign-mask lanes do not access memory. Active lanes read exactly
    /// one scalar; faulting accesses may partially complete in hardware. SSA
    /// input vectors remain unchanged; no signal-frame register ABI is promised.
    /// QPS/QD128 gather two lanes and zero the result's upper64 bits.
    pub fn is_x86_gather(self) -> bool {
        matches!(
            self,
            Self::X86Avx2Vgatherdps128
                | Self::X86Avx2Vgatherdps256
                | Self::X86Avx2Vgatherdpd128
                | Self::X86Avx2Vgatherdpd256
                | Self::X86Avx2Vgatherqps128
                | Self::X86Avx2Vgatherqps256
                | Self::X86Avx2Vgatherqpd128
                | Self::X86Avx2Vgatherqpd256
                | Self::X86Avx2Vpgatherdd128
                | Self::X86Avx2Vpgatherdd256
                | Self::X86Avx2Vpgatherdq128
                | Self::X86Avx2Vpgatherdq256
                | Self::X86Avx2Vpgatherqd128
                | Self::X86Avx2Vpgatherqd256
                | Self::X86Avx2Vpgatherqq128
                | Self::X86Avx2Vpgatherqq256
        )
    }

    pub fn is_x86_masked_mem(self) -> bool {
        matches!(
            self,
            Self::X86AvxVmaskmovpsLoad128
                | Self::X86AvxVmaskmovpsStore128
                | Self::X86AvxVmaskmovpsLoad256
                | Self::X86AvxVmaskmovpsStore256
                | Self::X86AvxVmaskmovpdLoad128
                | Self::X86AvxVmaskmovpdStore128
                | Self::X86AvxVmaskmovpdLoad256
                | Self::X86AvxVmaskmovpdStore256
                | Self::X86Avx2VpmaskmovdLoad128
                | Self::X86Avx2VpmaskmovdStore128
                | Self::X86Avx2VpmaskmovdLoad256
                | Self::X86Avx2VpmaskmovdStore256
                | Self::X86Avx2VpmaskmovqLoad128
                | Self::X86Avx2VpmaskmovqStore128
                | Self::X86Avx2VpmaskmovqLoad256
                | Self::X86Avx2VpmaskmovqStore256
        )
    }
    pub fn is_x86_masked_store(self) -> bool {
        matches!(
            self,
            Self::X86AvxVmaskmovpsStore128
                | Self::X86AvxVmaskmovpsStore256
                | Self::X86AvxVmaskmovpdStore128
                | Self::X86AvxVmaskmovpdStore256
                | Self::X86Avx2VpmaskmovdStore128
                | Self::X86Avx2VpmaskmovdStore256
                | Self::X86Avx2VpmaskmovqStore128
                | Self::X86Avx2VpmaskmovqStore256
        )
    }

    pub fn is_x86_raw_fp_convert(self) -> bool {
        self.x86_raw_fp_opcode().is_some()
    }
    pub fn x86_raw_fp_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVcvtdq2pd128 => Some("vcvtdq2pd"),
            Self::X86AvxVcvtdq2pd256 => Some("vcvtdq2pd"),
            Self::X86AvxVcvtdq2ps128 => Some("vcvtdq2ps"),
            Self::X86AvxVcvtdq2ps256 => Some("vcvtdq2ps"),
            Self::X86AvxVcvtpd2dq128 => Some("vcvtpd2dq"),
            Self::X86AvxVcvtpd2dq256 => Some("vcvtpd2dq"),
            Self::X86AvxVcvtpd2ps128 => Some("vcvtpd2ps"),
            Self::X86AvxVcvtpd2ps256 => Some("vcvtpd2ps"),
            Self::X86AvxVcvtps2dq128 => Some("vcvtps2dq"),
            Self::X86AvxVcvtps2dq256 => Some("vcvtps2dq"),
            Self::X86AvxVcvtps2pd128 => Some("vcvtps2pd"),
            Self::X86AvxVcvtps2pd256 => Some("vcvtps2pd"),
            Self::X86AvxVcvttpd2dq128 => Some("vcvttpd2dq"),
            Self::X86AvxVcvttpd2dq256 => Some("vcvttpd2dq"),
            Self::X86AvxVcvttps2dq128 => Some("vcvttps2dq"),
            Self::X86AvxVcvttps2dq256 => Some("vcvttps2dq"),
            Self::X86AvxVcvtss2si32 => Some("vcvtss2si"),
            Self::X86AvxVcvttss2si32 => Some("vcvttss2si"),
            Self::X86AvxVcvtss2si64 => Some("vcvtss2si"),
            Self::X86AvxVcvttss2si64 => Some("vcvttss2si"),
            Self::X86AvxVcvtsd2si32 => Some("vcvtsd2si"),
            Self::X86AvxVcvttsd2si32 => Some("vcvttsd2si"),
            Self::X86AvxVcvtsd2si64 => Some("vcvtsd2si"),
            Self::X86AvxVcvttsd2si64 => Some("vcvttsd2si"),
            Self::X86AvxVcvtsi2ss32 => Some("vcvtsi2ss"),
            Self::X86AvxVcvtsi2ss64 => Some("vcvtsi2ss"),
            Self::X86AvxVcvtsi2sd32 => Some("vcvtsi2sd"),
            Self::X86AvxVcvtsi2sd64 => Some("vcvtsi2sd"),
            Self::X86AvxVcvtsd2ss => Some("vcvtsd2ss"),
            Self::X86AvxVcvtss2sd => Some("vcvtss2sd"),
            _ => None,
        }
    }

    pub fn is_x86_fp_compare(self) -> bool {
        self.x86_fp_compare_opcode().is_some()
    }
    pub fn x86_fp_compare_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVcmpps128 => Some("vcmpps"),
            Self::X86AvxVcmpps256 => Some("vcmpps"),
            Self::X86AvxVcmppd128 => Some("vcmppd"),
            Self::X86AvxVcmppd256 => Some("vcmppd"),
            Self::X86AvxVcmpss => Some("vcmpss"),
            Self::X86AvxVcmpsd => Some("vcmpsd"),
            _ => None,
        }
    }

    pub fn x86_aes_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86Aesenc128 => Some("aesenc"),
            Self::X86Aesenclast128 => Some("aesenclast"),
            Self::X86Aesdec128 => Some("aesdec"),
            Self::X86Aesdeclast128 => Some("aesdeclast"),
            Self::X86Aesimc128 => Some("aesimc"),
            Self::X86Aeskeygenassist128 => Some("aeskeygenassist"),
            _ => None,
        }
    }
    pub fn is_x86_pure_bits(self) -> bool {
        self.x86_pure_bits_opcode().is_some()
    }
    pub fn x86_pure_bits_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVpermilps128 => Some("vpermilps"),
            Self::X86AvxVpermilps256 => Some("vpermilps"),
            Self::X86AvxVpermilpd128 => Some("vpermilpd"),
            Self::X86AvxVpermilpd256 => Some("vpermilpd"),
            Self::X86Avx2Vpermd256 => Some("vpermd"),
            Self::X86Avx2Vpermps256 => Some("vpermps"),
            Self::X86AvxVmovmskps128 => Some("vmovmskps"),
            Self::X86AvxVmovmskps256 => Some("vmovmskps"),
            Self::X86AvxVmovmskpd128 => Some("vmovmskpd"),
            Self::X86AvxVmovmskpd256 => Some("vmovmskpd"),
            Self::X86AvxVpmovmskb128 => Some("vpmovmskb"),
            Self::X86Avx2Vpmovmskb256 => Some("vpmovmskb"),
            Self::X86AvxVtestps128 => Some("vtestps"),
            Self::X86AvxVtestps256 => Some("vtestps"),
            Self::X86AvxVtestpd128 => Some("vtestpd"),
            Self::X86AvxVtestpd256 => Some("vtestpd"),
            Self::X86AvxVptest128 => Some("vptest"),
            Self::X86AvxVptest256 => Some("vptest"),
            _ => None,
        }
    }

    pub fn is_x86_fp_misc(self) -> bool {
        self.x86_fp_misc_opcode().is_some()
    }
    pub fn has_x86_round_control(self) -> bool {
        self.x86_fp_misc_opcode()
            .is_some_and(|s| s.starts_with("vround"))
    }
    pub fn x86_fp_misc_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVaddps128 => Some("vaddps"),
            Self::X86AvxVaddpd128 => Some("vaddpd"),
            Self::X86AvxVaddss => Some("vaddss"),
            Self::X86AvxVaddsd => Some("vaddsd"),
            Self::X86AvxVaddps256 => Some("vaddps"),
            Self::X86AvxVaddpd256 => Some("vaddpd"),
            Self::X86AvxVsubps128 => Some("vsubps"),
            Self::X86AvxVsubpd128 => Some("vsubpd"),
            Self::X86AvxVsubss => Some("vsubss"),
            Self::X86AvxVsubsd => Some("vsubsd"),
            Self::X86AvxVsubps256 => Some("vsubps"),
            Self::X86AvxVsubpd256 => Some("vsubpd"),
            Self::X86AvxVmulps128 => Some("vmulps"),
            Self::X86AvxVmulpd128 => Some("vmulpd"),
            Self::X86AvxVmulss => Some("vmulss"),
            Self::X86AvxVmulsd => Some("vmulsd"),
            Self::X86AvxVmulps256 => Some("vmulps"),
            Self::X86AvxVmulpd256 => Some("vmulpd"),
            Self::X86AvxVdivps128 => Some("vdivps"),
            Self::X86AvxVdivpd128 => Some("vdivpd"),
            Self::X86AvxVdivss => Some("vdivss"),
            Self::X86AvxVdivsd => Some("vdivsd"),
            Self::X86AvxVdivps256 => Some("vdivps"),
            Self::X86AvxVdivpd256 => Some("vdivpd"),
            Self::X86AvxVsqrtps128 => Some("vsqrtps"),
            Self::X86AvxVsqrtpd128 => Some("vsqrtpd"),
            Self::X86AvxVsqrtss => Some("vsqrtss"),
            Self::X86AvxVsqrtsd => Some("vsqrtsd"),
            Self::X86AvxVsqrtps256 => Some("vsqrtps"),
            Self::X86AvxVsqrtpd256 => Some("vsqrtpd"),
            Self::X86AvxVroundps128 => Some("vroundps"),
            Self::X86AvxVroundps256 => Some("vroundps"),
            Self::X86AvxVroundpd128 => Some("vroundpd"),
            Self::X86AvxVroundpd256 => Some("vroundpd"),
            Self::X86AvxVroundss => Some("vroundss"),
            Self::X86AvxVroundsd => Some("vroundsd"),
            Self::X86AvxVminps128 => Some("vminps"),
            Self::X86AvxVminps256 => Some("vminps"),
            Self::X86AvxVminpd128 => Some("vminpd"),
            Self::X86AvxVminpd256 => Some("vminpd"),
            Self::X86AvxVminss => Some("vminss"),
            Self::X86AvxVminsd => Some("vminsd"),
            Self::X86AvxVmaxps128 => Some("vmaxps"),
            Self::X86AvxVmaxps256 => Some("vmaxps"),
            Self::X86AvxVmaxpd128 => Some("vmaxpd"),
            Self::X86AvxVmaxpd256 => Some("vmaxpd"),
            Self::X86AvxVmaxss => Some("vmaxss"),
            Self::X86AvxVmaxsd => Some("vmaxsd"),
            Self::X86AvxVrcpps128 => Some("vrcpps"),
            Self::X86AvxVrcpps256 => Some("vrcpps"),
            Self::X86AvxVrcpss => Some("vrcpss"),
            Self::X86AvxVrsqrtps128 => Some("vrsqrtps"),
            Self::X86AvxVrsqrtps256 => Some("vrsqrtps"),
            Self::X86AvxVrsqrtss => Some("vrsqrtss"),
            Self::X86AvxVcomiss => Some("vcomiss"),
            Self::X86AvxVucomiss => Some("vucomiss"),
            Self::X86AvxVcomisd => Some("vcomisd"),
            Self::X86AvxVucomisd => Some("vucomisd"),
            _ => None,
        }
    }

    /// Numerical/MXCSR equivalence to the six MMX conversions, without their
    /// physical MMX or x87 tag/register state changes. All values are immutable.
    pub fn is_x86_mmx_value_convert(self) -> bool {
        matches!(
            self,
            Self::X86SseMmxvalueCvtpi2ps
                | Self::X86SseMmxvalueCvtps2pi
                | Self::X86SseMmxvalueCvttps2pi
                | Self::X86SseMmxvalueCvtpi2pd
                | Self::X86SseMmxvalueCvtpd2pi
                | Self::X86SseMmxvalueCvttpd2pi
        )
    }

    pub fn is_x86_legacy_raw_fp_convert(self) -> bool {
        self.x86_legacy_raw_fp_opcode().is_some()
    }
    pub fn x86_legacy_raw_fp_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86Ssecvtdq2pd128 => Some("cvtdq2pd"),
            Self::X86Ssecvtdq2ps128 => Some("cvtdq2ps"),
            Self::X86Ssecvtpd2dq128 => Some("cvtpd2dq"),
            Self::X86Ssecvtpd2ps128 => Some("cvtpd2ps"),
            Self::X86Ssecvtps2dq128 => Some("cvtps2dq"),
            Self::X86Ssecvtps2pd128 => Some("cvtps2pd"),
            Self::X86Ssecvttpd2dq128 => Some("cvttpd2dq"),
            Self::X86Ssecvttps2dq128 => Some("cvttps2dq"),
            Self::X86Ssecvtss2si32 => Some("cvtss2si"),
            Self::X86Ssecvttss2si32 => Some("cvttss2si"),
            Self::X86Ssecvtss2si64 => Some("cvtss2si"),
            Self::X86Ssecvttss2si64 => Some("cvttss2si"),
            Self::X86Ssecvtsd2si32 => Some("cvtsd2si"),
            Self::X86Ssecvttsd2si32 => Some("cvttsd2si"),
            Self::X86Ssecvtsd2si64 => Some("cvtsd2si"),
            Self::X86Ssecvttsd2si64 => Some("cvttsd2si"),
            Self::X86Ssecvtsi2ss32 => Some("cvtsi2ss"),
            Self::X86Ssecvtsi2ss64 => Some("cvtsi2ss"),
            Self::X86Ssecvtsi2sd32 => Some("cvtsi2sd"),
            Self::X86Ssecvtsi2sd64 => Some("cvtsi2sd"),
            Self::X86Ssecvtsd2ss => Some("cvtsd2ss"),
            Self::X86Ssecvtss2sd => Some("cvtss2sd"),
            _ => None,
        }
    }

    pub fn is_x86_legacy_fp_misc(self) -> bool {
        self.x86_legacy_fp_misc_opcode().is_some()
    }
    pub fn x86_legacy_fp_misc_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86SseAddps128 => Some("addps"),
            Self::X86SseAddpd128 => Some("addpd"),
            Self::X86SseAddss => Some("addss"),
            Self::X86SseAddsd => Some("addsd"),
            Self::X86SseSubps128 => Some("subps"),
            Self::X86SseSubpd128 => Some("subpd"),
            Self::X86SseSubss => Some("subss"),
            Self::X86SseSubsd => Some("subsd"),
            Self::X86SseMulps128 => Some("mulps"),
            Self::X86SseMulpd128 => Some("mulpd"),
            Self::X86SseMulss => Some("mulss"),
            Self::X86SseMulsd => Some("mulsd"),
            Self::X86SseDivps128 => Some("divps"),
            Self::X86SseDivpd128 => Some("divpd"),
            Self::X86SseDivss => Some("divss"),
            Self::X86SseDivsd => Some("divsd"),
            Self::X86SseSqrtps128 => Some("sqrtps"),
            Self::X86SseSqrtpd128 => Some("sqrtpd"),
            Self::X86SseSqrtss => Some("sqrtss"),
            Self::X86SseSqrtsd => Some("sqrtsd"),
            Self::X86SseMinps128 => Some("minps"),
            Self::X86SseMinpd128 => Some("minpd"),
            Self::X86SseMinss => Some("minss"),
            Self::X86SseMinsd => Some("minsd"),
            Self::X86SseMaxps128 => Some("maxps"),
            Self::X86SseMaxpd128 => Some("maxpd"),
            Self::X86SseMaxss => Some("maxss"),
            Self::X86SseMaxsd => Some("maxsd"),
            Self::X86SseRcpps128 => Some("rcpps"),
            Self::X86SseRcpss => Some("rcpss"),
            Self::X86SseRsqrtps128 => Some("rsqrtps"),
            Self::X86SseRsqrtss => Some("rsqrtss"),
            Self::X86SseComiss => Some("comiss"),
            Self::X86SseUcomiss => Some("ucomiss"),
            Self::X86SseComisd => Some("comisd"),
            Self::X86SseUcomisd => Some("ucomisd"),
            _ => None,
        }
    }
    pub fn is_x86_legacy_fp_compare(self) -> bool {
        self.x86_legacy_fp_compare_opcode().is_some()
    }
    pub fn x86_legacy_fp_compare_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86SseCmpps128 => Some("cmpps"),
            Self::X86SseCmppd128 => Some("cmppd"),
            Self::X86SseCmpss => Some("cmpss"),
            Self::X86SseCmpsd => Some("cmpsd"),
            _ => None,
        }
    }

    pub fn is_x86_legacy_sign_mask(self) -> bool {
        self.x86_legacy_sign_mask_opcode().is_some()
    }
    pub fn x86_legacy_sign_mask_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86SseMovmskps128 => Some("movmskps"),
            Self::X86SseMovmskpd128 => Some("movmskpd"),
            Self::X86SsePmovmskb128 => Some("pmovmskb"),
            _ => None,
        }
    }
    pub fn is_x86_legacy_fp_v2(self) -> bool {
        self.x86_legacy_fp_v2_opcode().is_some()
    }
    pub fn x86_legacy_fp_v2_control(self) -> Option<(usize, i64)> {
        match self {
            Self::X86Sse41Roundps128 => Some((1, 16)),
            Self::X86Sse41Roundpd128 => Some((1, 16)),
            Self::X86Sse41Roundss => Some((1, 16)),
            Self::X86Sse41Roundsd => Some((1, 16)),
            Self::X86Sse41Dpps => Some((2, 256)),
            Self::X86Sse41Dppd => Some((2, 256)),
            _ => None,
        }
    }
    pub fn x86_legacy_fp_v2_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86Sse3Haddps => Some("haddps"),
            Self::X86Sse3Haddpd => Some("haddpd"),
            Self::X86Sse3Hsubps => Some("hsubps"),
            Self::X86Sse3Hsubpd => Some("hsubpd"),
            Self::X86Sse3Addsubps => Some("addsubps"),
            Self::X86Sse3Addsubpd => Some("addsubpd"),
            Self::X86Sse41Roundps128 => Some("roundps"),
            Self::X86Sse41Roundpd128 => Some("roundpd"),
            Self::X86Sse41Roundss => Some("roundss"),
            Self::X86Sse41Roundsd => Some("roundsd"),
            Self::X86Sse41Dpps => Some("dpps"),
            Self::X86Sse41Dppd => Some("dppd"),
            _ => None,
        }
    }
    pub fn is_x86_crc32(self) -> bool {
        self.x86_crc32_opcode().is_some()
    }
    pub fn is_x86_legacy_ptest(self) -> bool {
        self == Self::X86Sse41Ptest128
    }
    pub fn x86_crc32_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86Sse42Crc32b => Some("crc32"),
            Self::X86Sse42Crc32w => Some("crc32"),
            Self::X86Sse42Crc32d => Some("crc32"),
            Self::X86Sse42Crc32q => Some("crc32"),
            _ => None,
        }
    }
    pub fn is_x86_string(self) -> bool {
        self.x86_string_info().is_some()
    }
    /// Native opcode, immediate operand index, result kind (index0/mask1/flags2).
    pub fn x86_string_info(self) -> Option<(&'static str, usize, u8)> {
        match self {
            Self::X86AvxPcmpistriIndex => Some(("vpcmpistri", 2, 0)),
            Self::X86AvxPcmpistri64Index => Some(("vpcmpistri64", 2, 0)),
            Self::X86AvxPcmpistrmMask => Some(("vpcmpistrm", 2, 1)),
            Self::X86AvxPcmpistriFlags => Some(("vpcmpistri", 2, 2)),
            Self::X86AvxPcmpestriIndex => Some(("vpcmpestri", 4, 0)),
            Self::X86AvxPcmpestrmMask => Some(("vpcmpestrm", 4, 1)),
            Self::X86AvxPcmpestriFlags => Some(("vpcmpestri", 4, 2)),
            Self::X86AvxPcmpestri64Index => Some(("vpcmpestri64", 4, 0)),
            Self::X86AvxPcmpestrm64Mask => Some(("vpcmpestrm64", 4, 1)),
            Self::X86AvxPcmpestri64Flags => Some(("vpcmpestri64", 4, 2)),
            Self::X86Sse42PcmpistriIndex => Some(("pcmpistri", 2, 0)),
            Self::X86Sse42Pcmpistri64Index => Some(("pcmpistri64", 2, 0)),
            Self::X86Sse42PcmpistrmMask => Some(("pcmpistrm", 2, 1)),
            Self::X86Sse42PcmpistriFlags => Some(("pcmpistri", 2, 2)),
            Self::X86Sse42PcmpestriIndex => Some(("pcmpestri", 4, 0)),
            Self::X86Sse42PcmpestrmMask => Some(("pcmpestrm", 4, 1)),
            Self::X86Sse42PcmpestriFlags => Some(("pcmpestri", 4, 2)),
            Self::X86Sse42Pcmpestri64Index => Some(("pcmpestri64", 4, 0)),
            Self::X86Sse42Pcmpestrm64Mask => Some(("pcmpestrm64", 4, 1)),
            Self::X86Sse42Pcmpestri64Flags => Some(("pcmpestri64", 4, 2)),
            _ => None,
        }
    }
    pub fn is_x86_raw_fma(self) -> bool {
        self.x86_raw_fma_opcode().is_some()
    }
    /// Exact architectural opcode; no commuting, sign rewriting or order selection.
    pub fn x86_raw_fma_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86FmaVfmadd132pd128 => Some("vfmadd132pd"),
            Self::X86FmaVfmadd132pd256 => Some("vfmadd132pd"),
            Self::X86FmaVfmadd132ps128 => Some("vfmadd132ps"),
            Self::X86FmaVfmadd132ps256 => Some("vfmadd132ps"),
            Self::X86FmaVfmadd132sd128 => Some("vfmadd132sd"),
            Self::X86FmaVfmadd132ss128 => Some("vfmadd132ss"),
            Self::X86FmaVfmadd213pd128 => Some("vfmadd213pd"),
            Self::X86FmaVfmadd213pd256 => Some("vfmadd213pd"),
            Self::X86FmaVfmadd213ps128 => Some("vfmadd213ps"),
            Self::X86FmaVfmadd213ps256 => Some("vfmadd213ps"),
            Self::X86FmaVfmadd213sd128 => Some("vfmadd213sd"),
            Self::X86FmaVfmadd213ss128 => Some("vfmadd213ss"),
            Self::X86FmaVfmadd231pd128 => Some("vfmadd231pd"),
            Self::X86FmaVfmadd231pd256 => Some("vfmadd231pd"),
            Self::X86FmaVfmadd231ps128 => Some("vfmadd231ps"),
            Self::X86FmaVfmadd231ps256 => Some("vfmadd231ps"),
            Self::X86FmaVfmadd231sd128 => Some("vfmadd231sd"),
            Self::X86FmaVfmadd231ss128 => Some("vfmadd231ss"),
            Self::X86FmaVfmaddsub132pd128 => Some("vfmaddsub132pd"),
            Self::X86FmaVfmaddsub132pd256 => Some("vfmaddsub132pd"),
            Self::X86FmaVfmaddsub213pd128 => Some("vfmaddsub213pd"),
            Self::X86FmaVfmaddsub213pd256 => Some("vfmaddsub213pd"),
            Self::X86FmaVfmaddsub231pd128 => Some("vfmaddsub231pd"),
            Self::X86FmaVfmaddsub231pd256 => Some("vfmaddsub231pd"),
            Self::X86FmaVfmaddsub132ps128 => Some("vfmaddsub132ps"),
            Self::X86FmaVfmaddsub132ps256 => Some("vfmaddsub132ps"),
            Self::X86FmaVfmaddsub213ps128 => Some("vfmaddsub213ps"),
            Self::X86FmaVfmaddsub213ps256 => Some("vfmaddsub213ps"),
            Self::X86FmaVfmaddsub231ps128 => Some("vfmaddsub231ps"),
            Self::X86FmaVfmaddsub231ps256 => Some("vfmaddsub231ps"),
            Self::X86FmaVfmsubadd132pd128 => Some("vfmsubadd132pd"),
            Self::X86FmaVfmsubadd132pd256 => Some("vfmsubadd132pd"),
            Self::X86FmaVfmsubadd213pd128 => Some("vfmsubadd213pd"),
            Self::X86FmaVfmsubadd213pd256 => Some("vfmsubadd213pd"),
            Self::X86FmaVfmsubadd231pd128 => Some("vfmsubadd231pd"),
            Self::X86FmaVfmsubadd231pd256 => Some("vfmsubadd231pd"),
            Self::X86FmaVfmsubadd132ps128 => Some("vfmsubadd132ps"),
            Self::X86FmaVfmsubadd132ps256 => Some("vfmsubadd132ps"),
            Self::X86FmaVfmsubadd213ps128 => Some("vfmsubadd213ps"),
            Self::X86FmaVfmsubadd213ps256 => Some("vfmsubadd213ps"),
            Self::X86FmaVfmsubadd231ps128 => Some("vfmsubadd231ps"),
            Self::X86FmaVfmsubadd231ps256 => Some("vfmsubadd231ps"),
            Self::X86FmaVfmsub132pd128 => Some("vfmsub132pd"),
            Self::X86FmaVfmsub132pd256 => Some("vfmsub132pd"),
            Self::X86FmaVfmsub132ps128 => Some("vfmsub132ps"),
            Self::X86FmaVfmsub132ps256 => Some("vfmsub132ps"),
            Self::X86FmaVfmsub132sd128 => Some("vfmsub132sd"),
            Self::X86FmaVfmsub132ss128 => Some("vfmsub132ss"),
            Self::X86FmaVfmsub213pd128 => Some("vfmsub213pd"),
            Self::X86FmaVfmsub213pd256 => Some("vfmsub213pd"),
            Self::X86FmaVfmsub213ps128 => Some("vfmsub213ps"),
            Self::X86FmaVfmsub213ps256 => Some("vfmsub213ps"),
            Self::X86FmaVfmsub213sd128 => Some("vfmsub213sd"),
            Self::X86FmaVfmsub213ss128 => Some("vfmsub213ss"),
            Self::X86FmaVfmsub231pd128 => Some("vfmsub231pd"),
            Self::X86FmaVfmsub231pd256 => Some("vfmsub231pd"),
            Self::X86FmaVfmsub231ps128 => Some("vfmsub231ps"),
            Self::X86FmaVfmsub231ps256 => Some("vfmsub231ps"),
            Self::X86FmaVfmsub231sd128 => Some("vfmsub231sd"),
            Self::X86FmaVfmsub231ss128 => Some("vfmsub231ss"),
            Self::X86FmaVfnmadd132pd128 => Some("vfnmadd132pd"),
            Self::X86FmaVfnmadd132pd256 => Some("vfnmadd132pd"),
            Self::X86FmaVfnmadd132ps128 => Some("vfnmadd132ps"),
            Self::X86FmaVfnmadd132ps256 => Some("vfnmadd132ps"),
            Self::X86FmaVfnmadd132sd128 => Some("vfnmadd132sd"),
            Self::X86FmaVfnmadd132ss128 => Some("vfnmadd132ss"),
            Self::X86FmaVfnmadd213pd128 => Some("vfnmadd213pd"),
            Self::X86FmaVfnmadd213pd256 => Some("vfnmadd213pd"),
            Self::X86FmaVfnmadd213ps128 => Some("vfnmadd213ps"),
            Self::X86FmaVfnmadd213ps256 => Some("vfnmadd213ps"),
            Self::X86FmaVfnmadd213sd128 => Some("vfnmadd213sd"),
            Self::X86FmaVfnmadd213ss128 => Some("vfnmadd213ss"),
            Self::X86FmaVfnmadd231pd128 => Some("vfnmadd231pd"),
            Self::X86FmaVfnmadd231pd256 => Some("vfnmadd231pd"),
            Self::X86FmaVfnmadd231ps128 => Some("vfnmadd231ps"),
            Self::X86FmaVfnmadd231ps256 => Some("vfnmadd231ps"),
            Self::X86FmaVfnmadd231sd128 => Some("vfnmadd231sd"),
            Self::X86FmaVfnmadd231ss128 => Some("vfnmadd231ss"),
            Self::X86FmaVfnmsub132pd128 => Some("vfnmsub132pd"),
            Self::X86FmaVfnmsub132pd256 => Some("vfnmsub132pd"),
            Self::X86FmaVfnmsub132ps128 => Some("vfnmsub132ps"),
            Self::X86FmaVfnmsub132ps256 => Some("vfnmsub132ps"),
            Self::X86FmaVfnmsub132sd128 => Some("vfnmsub132sd"),
            Self::X86FmaVfnmsub132ss128 => Some("vfnmsub132ss"),
            Self::X86FmaVfnmsub213pd128 => Some("vfnmsub213pd"),
            Self::X86FmaVfnmsub213pd256 => Some("vfnmsub213pd"),
            Self::X86FmaVfnmsub213ps128 => Some("vfnmsub213ps"),
            Self::X86FmaVfnmsub213ps256 => Some("vfnmsub213ps"),
            Self::X86FmaVfnmsub213sd128 => Some("vfnmsub213sd"),
            Self::X86FmaVfnmsub213ss128 => Some("vfnmsub213ss"),
            Self::X86FmaVfnmsub231pd128 => Some("vfnmsub231pd"),
            Self::X86FmaVfnmsub231pd256 => Some("vfnmsub231pd"),
            Self::X86FmaVfnmsub231ps128 => Some("vfnmsub231ps"),
            Self::X86FmaVfnmsub231ps256 => Some("vfnmsub231ps"),
            Self::X86FmaVfnmsub231sd128 => Some("vfnmsub231sd"),
            Self::X86FmaVfnmsub231ss128 => Some("vfnmsub231ss"),
            _ => None,
        }
    }
    pub fn is_x86_fp_horizontal_dot(self) -> bool {
        self.x86_fp_horizontal_dot_opcode().is_some()
    }
    pub fn has_x86_dot_control(self) -> bool {
        matches!(
            self,
            Self::X86AvxVdpps128 | Self::X86AvxVdpps256 | Self::X86AvxVdppd128
        )
    }
    pub fn x86_fp_horizontal_dot_opcode(self) -> Option<&'static str> {
        match self {
            Self::X86AvxVhaddps128 => Some("vhaddps"),
            Self::X86AvxVhaddps256 => Some("vhaddps"),
            Self::X86AvxVhaddpd128 => Some("vhaddpd"),
            Self::X86AvxVhaddpd256 => Some("vhaddpd"),
            Self::X86AvxVhsubps128 => Some("vhsubps"),
            Self::X86AvxVhsubps256 => Some("vhsubps"),
            Self::X86AvxVhsubpd128 => Some("vhsubpd"),
            Self::X86AvxVhsubpd256 => Some("vhsubpd"),
            Self::X86AvxVaddsubps128 => Some("vaddsubps"),
            Self::X86AvxVaddsubps256 => Some("vaddsubps"),
            Self::X86AvxVaddsubpd128 => Some("vaddsubpd"),
            Self::X86AvxVaddsubpd256 => Some("vaddsubpd"),
            Self::X86AvxVdpps128 => Some("vdpps"),
            Self::X86AvxVdpps256 => Some("vdpps"),
            Self::X86AvxVdppd128 => Some("vdppd"),
            _ => None,
        }
    }
    pub fn from_name(name: &str) -> Option<Intrinsic> {
        Intrinsic::ALL.into_iter().find(|i| i.info().name == name)
    }
}

impl fmt::Display for Intrinsic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.info().name)
    }
}
