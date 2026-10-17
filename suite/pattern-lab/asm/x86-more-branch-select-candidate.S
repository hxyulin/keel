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
L_loop:
    movq %rsi, %rax
    shrq $8, %rax
    andq %rcx, %rax
    movq (%rdx,%rax,8), %r8
    leaq 3(%rsi), %r9
    leaq 5(%rsi), %rsi
    testq $1, %r8
    cmovnzq %r9, %rsi
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
