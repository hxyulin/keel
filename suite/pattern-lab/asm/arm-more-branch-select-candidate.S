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
L_loop:
    lsr x4, x1, #8
    and x4, x4, x3
    ldr x4, [x2, x4, lsl #3]
    tst x4, #1
    add x5, x1, #3
    add x6, x1, #5
    csel x1, x5, x6, ne
    subs x0, x0, #1
    b.ne L_loop
L_done:
    mov x0, x1
    ret
.cfi_endproc
#ifndef __APPLE__
.size KERNEL, .-KERNEL
.section .note.GNU-stack,"",%progbits
#endif
