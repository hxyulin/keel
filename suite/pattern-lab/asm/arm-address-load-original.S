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
    ubfx w4, w1, #16, #8
    and w4, w4, w3
    sxtw x4, w4
    lsl x4, x4, #4
    add x4, x2, x4
    ldr x1, [x4]
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
