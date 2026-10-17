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
    movz x6, #0x7c15
    movk x6, #0x7f4a, lsl #16
    movk x6, #0x79b9, lsl #32
    movk x6, #0x9e37, lsl #48
L_loop:
    lsr x4, x1, #13
    lsl x5, x1, #51
    orr x1, x4, x5
    eor x1, x1, x6
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
