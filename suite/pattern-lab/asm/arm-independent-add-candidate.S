#ifdef __APPLE__
#define KERNEL _pattern_kernel
#else
#define KERNEL pattern_kernel
#endif
.text
.p2align 2
.globl KERNEL
#ifndef __APPLE__
.type KERNEL, %function
#endif
KERNEL:
.cfi_startproc
    cbz x0, L_done
    mov x4, x1
    add x5, x1, #1
    add x6, x1, #2
    add x7, x1, #3
    add x8, x1, x1, lsl #1
    add x8, x8, #6
L_loop:
    add x4, x4, #2
    add x5, x5, #2
    add x6, x6, #2
    add x7, x7, #2
    subs x0, x0, #1
    b.ne L_loop
    add x1, x4, x5
    add x1, x1, x6
    add x1, x1, x7
    sub x1, x1, x8
L_done:
    mov x0, x1
    ret
.cfi_endproc
#ifndef __APPLE__
.size KERNEL, .-KERNEL
.section .note.GNU-stack,"",%progbits
#endif
