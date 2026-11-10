/* AArch64 Mach-O ordered reductions. PACKED chooses NEON loads/absolute
 * values; zero chooses scalar pair loads. Both unroll by eight and keep
 * every addition in input order. Only caller-saved SIMD registers are used.
 * No read passes n, including the scalar tail. No fast-math assumptions. */
.text
.macro ORDERED name, wide, absolute
.globl \name
.p2align 4
\name:
.if \wide
    fmov d0, xzr
.else
    fmov s0, wzr
.endif
    cmp x1, #8
    b.lo 2f
1:
#if PACKED
.if \wide
    ldp q1, q2, [x0], #32
    ldp q3, q4, [x0], #32
.else
    ldp q1, q2, [x0], #32
.endif
.if \absolute
.if \wide
    fabs v1.2d, v1.2d
    fabs v2.2d, v2.2d
    fabs v3.2d, v3.2d
    fabs v4.2d, v4.2d
.else
    fabs v1.4s, v1.4s
    fabs v2.4s, v2.4s
.endif
.endif
.if \wide
    fadd d0, d0, d1
    mov d5, v1.d[1]
    fadd d0, d0, d5
    fadd d0, d0, d2
    mov d5, v2.d[1]
    fadd d0, d0, d5
    fadd d0, d0, d3
    mov d5, v3.d[1]
    fadd d0, d0, d5
    fadd d0, d0, d4
    mov d5, v4.d[1]
    fadd d0, d0, d5
.else
    fadd s0, s0, s1
    mov s5, v1.s[1]
    fadd s0, s0, s5
    mov s5, v1.s[2]
    fadd s0, s0, s5
    mov s5, v1.s[3]
    fadd s0, s0, s5
    fadd s0, s0, s2
    mov s5, v2.s[1]
    fadd s0, s0, s5
    mov s5, v2.s[2]
    fadd s0, s0, s5
    mov s5, v2.s[3]
    fadd s0, s0, s5
.endif
#else
.if \wide
    ldp d1, d2, [x0], #16
    ldp d3, d4, [x0], #16
    ldp d5, d6, [x0], #16
    ldp d7, d16, [x0], #16
.if \absolute
    fabs d1, d1
    fabs d2, d2
    fabs d3, d3
    fabs d4, d4
    fabs d5, d5
    fabs d6, d6
    fabs d7, d7
    fabs d16, d16
.endif
    fadd d0, d0, d1
    fadd d0, d0, d2
    fadd d0, d0, d3
    fadd d0, d0, d4
    fadd d0, d0, d5
    fadd d0, d0, d6
    fadd d0, d0, d7
    fadd d0, d0, d16
.else
    ldp s1, s2, [x0], #8
    ldp s3, s4, [x0], #8
    ldp s5, s6, [x0], #8
    ldp s7, s16, [x0], #8
.if \absolute
    fabs s1, s1
    fabs s2, s2
    fabs s3, s3
    fabs s4, s4
    fabs s5, s5
    fabs s6, s6
    fabs s7, s7
    fabs s16, s16
.endif
    fadd s0, s0, s1
    fadd s0, s0, s2
    fadd s0, s0, s3
    fadd s0, s0, s4
    fadd s0, s0, s5
    fadd s0, s0, s6
    fadd s0, s0, s7
    fadd s0, s0, s16
.endif
#endif
    sub x1, x1, #8
    cmp x1, #8
    b.hs 1b
2:
    cbz x1, 4f
3:
.if \wide
    ldr d1, [x0], #8
.if \absolute
    fabs d1, d1
.endif
    fadd d0, d0, d1
.else
    ldr s1, [x0], #4
.if \absolute
    fabs s1, s1
.endif
    fadd s0, s0, s1
.endif
    subs x1, x1, #1
    b.ne 3b
4:
    ret
.endm
ORDERED _sum64, 1, 0
ORDERED _abs64, 1, 1
ORDERED _sum32, 0, 0
ORDERED _abs32, 0, 1
