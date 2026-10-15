// Correctness fixture, not a tuned microkernel. Ordinary PCS; ZA inactive
// on entry. packed A[kblock][M][1], B[kblock][N][1], row-major C[M][N].
// x0=A, x1=B, x2=C, x3=M, x4=N, x5=number of K blocks.
// Every packed row/column group is four bytes; incomplete K groups are zero padded.
.text
.globl _matmul_fp32
_matmul_fp32:
    sub sp, sp, #64
    stp d8, d9, [sp]
    stp d10, d11, [sp, #16]
    stp d12, d13, [sp, #32]
    stp d14, d15, [sp, #48]
    cbz x3, .Lfp32_done
    cbz x4, .Lfp32_done
    smstart
    cntw x6
    lsl x16, x3, #2
    lsl x17, x4, #2
    mov x7, #0
.Lfp32_rows:
    mov x8, #0
.Lfp32_cols:
    lsl x9, x3, #0
    lsl x10, x7, #0
    whilelo p0.s, x10, x9
    lsl x9, x4, #0
    lsl x10, x8, #0
    whilelo p1.s, x10, x9
    whilelo p2.s, x8, x4
    add x9, x0, x7, lsl #2
    add x10, x1, x8, lsl #2
    mov x11, x5
    zero {za}
    cbz x11, .Lfp32_output
.Lfp32_inner:
    ld1w {z0.s}, p0/z, [x9]
    ld1w {z1.s}, p1/z, [x10]
    fmopa za0.s, p0/m, p1/m, z0.s, z1.s
    add x9, x9, x16
    add x10, x10, x17
    subs x11, x11, #1
    b.ne .Lfp32_inner
.Lfp32_output:
    madd x13, x7, x4, x8
    add x13, x2, x13, lsl #2
    mov x12, #0
    mov x15, x7
.Lfp32_store:
    cmp x15, x3
    b.hs .Lfp32_next
    cmp x12, x6
    b.hs .Lfp32_next
    mova z2.s, p2/m, za0h.s[w12, 0]
    st1w {z2.s}, p2, [x13]
    add x13, x13, x17
    add x12, x12, #1
    add x15, x15, #1
    b .Lfp32_store
.Lfp32_next:
    add x8, x8, x6
    cmp x8, x4
    b.lo .Lfp32_cols
    add x7, x7, x6
    cmp x7, x3
    b.lo .Lfp32_rows
    smstop
.Lfp32_done:
    ldp d8, d9, [sp]
    ldp d10, d11, [sp, #16]
    ldp d12, d13, [sp, #32]
    ldp d14, d15, [sp, #48]
    add sp, sp, #64
    ret
