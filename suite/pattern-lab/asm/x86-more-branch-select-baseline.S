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
    testq $1, %r8
    jz L_even
    leaq 3(%rsi), %rsi
    jmp L_selected
L_even:
    leaq 5(%rsi), %rsi
L_selected:
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
