#ifdef __APPLE__
#define KERNEL _pattern_kernel
#else
#define KERNEL pattern_kernel
#endif
.text
.p2align 4
.globl KERNEL
#ifndef __APPLE__
.type KERNEL, @function
#endif
KERNEL:
.cfi_startproc
    testq %rdi, %rdi
    jz L_done
    movabsq $0x9e3779b97f4a7c15, %r8
    movabsq $0xcccccccccccccccd, %r9
L_loop:
    movq %rsi, %rax
    mulq %r9
    shrq $3, %rdx
    movq %rdx, %rsi
    addq %r8, %rsi
    subq $1, %rdi
    jnz L_loop
L_done:
    movq %rsi, %rax
    ret
.cfi_endproc
#ifndef __APPLE__
.size KERNEL, .-KERNEL
.section .note.GNU-stack,"",@progbits
#endif
