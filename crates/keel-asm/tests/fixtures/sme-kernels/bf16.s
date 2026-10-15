// Correctness fixture, not a tuned microkernel. Ordinary PCS; ZA inactive
// on entry. packed A[kblock][M][2], B[kblock][N][2], row-major C[M][N].
// x0=A, x1=B, x2=C, x3=M, x4=N, x5=number of K blocks.
// Every packed row/column group is four bytes; incomplete K groups are zero padded.
.text
.globl _matmul_bf16
_matmul_bf16:
    sub sp, sp, #64
    stp d8, d9, [sp]
    stp d10, d11, [sp, #16]
    stp d12, d13, [sp, #32]
    stp d14, d15, [sp, #48]
    cbz x3, .Lbf16_done
    cbz x4, .Lbf16_done
    smstart
    cntw x6
    lsl x16, x3, #2
    lsl x17, x4, #2
    mov x7, #0
.Lbf16_rows:
    mov x8, #0
.Lbf16_cols:
    lsl x9, x3, #1
    lsl x10, x7, #1
    whilelo p0.h, x10, x9
    lsl x9, x4, #1
    lsl x10, x8, #1
    whilelo p1.h, x10, x9
    whilelo p2.s, x8, x4
    add x9, x0, x7, lsl #2
    add x10, x1, x8, lsl #2
    mov x11, x5
    zero {za}
    cbz x11, .Lbf16_output
.Lbf16_inner:
    ld1h {z0.h}, p0/z, [x9]
    ld1h {z1.h}, p1/z, [x10]
    bfmopa za0.s, p0/m, p1/m, z0.h, z1.h
    add x9, x9, x16
    add x10, x10, x17
    subs x11, x11, #1
    b.ne .Lbf16_inner
.Lbf16_output:
    madd x13, x7, x4, x8
    add x13, x2, x13, lsl #2
    mov x12, #0
    mov x15, x7
.Lbf16_store:
    cmp x15, x3
    b.hs .Lbf16_next
    cmp x12, x6
    b.hs .Lbf16_next
    mova z2.s, p2/m, za0h.s[w12, 0]
    st1w {z2.s}, p2, [x13]
    add x13, x13, x17
    add x12, x12, #1
    add x15, x15, #1
    b .Lbf16_store
.Lbf16_next:
    add x8, x8, x6
    cmp x8, x4
    b.lo .Lbf16_cols
    add x7, x7, x6
    cmp x7, x3
    b.lo .Lbf16_rows
    smstop
.Lbf16_done:
    ldp d8, d9, [sp]
    ldp d10, d11, [sp, #16]
    ldp d12, d13, [sp, #32]
    ldp d14, d15, [sp, #48]
    add sp, sp, #64
    ret
