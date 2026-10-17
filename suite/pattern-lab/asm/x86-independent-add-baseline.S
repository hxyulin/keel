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
    movq %rsi, %r8
    leaq 1(%rsi), %r9
    leaq 2(%rsi), %r10
    leaq 3(%rsi), %r11
    leaq 6(%rsi,%rsi,2), %rax
L_loop:
    addq $1, %r8
    addq $1, %r9
    addq $1, %r10
    addq $1, %r11
    addq $1, %r8
    addq $1, %r9
    addq $1, %r10
    addq $1, %r11
    subq $1, %rdi
    jnz L_loop
    addq %r9, %r8
    addq %r10, %r8
    addq %r11, %r8
    subq %rax, %r8
    movq %r8, %rsi
L_done:
    movq %rsi, %rax
    ret
.cfi_endproc
#ifndef __APPLE__
.size KERNEL, .-KERNEL
.section .note.GNU-stack,"",@progbits
#endif
