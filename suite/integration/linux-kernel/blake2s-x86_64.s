# SPDX-License-Identifier: MIT
# Scalar x86-64 reference for the Linux v6.12 BLAKE2s compression ABI.
# Round permutations follow lib/crypto/blake2s-generic.c, whose table is
# Copyright (C) 2015-2019 Jason A. Donenfeld <Jason@zx2c4.com>.

.text
.p2align 4
.globl blake2s_compress_generic
.type blake2s_compress_generic,@function
# The first eight state words stay in r8d-r15d. The remaining words use
# 32 bytes of stack, and message words are read directly from the input.
.macro g a,b,c,d,x,y
    movl \c(%rsp), %eax
    movl \d(%rsp), %ecx
    addl \b, \a
    addl \x(%rbx), \a
    xorl \a, %ecx
    rorl $16, %ecx
    addl %ecx, %eax
    xorl %eax, \b
    rorl $12, \b
    addl \b, \a
    addl \y(%rbx), \a
    xorl \a, %ecx
    rorl $8, %ecx
    addl %ecx, %eax
    xorl %eax, \b
    rorl $7, \b
    movl %eax, \c(%rsp)
    movl %ecx, \d(%rsp)
.endm
blake2s_compress_generic:
    testq %rdx, %rdx
    je .Lempty
    pushq %rbp
    movq %rsp, %rbp
    pushq %rbx
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    subq $32, %rsp
    movq %rsi, %rbx
    movl %ecx, %esi
.Lblock:
    addl %esi, 32(%rdi)
    adcl $0, 36(%rdi)
    movl 0(%rdi), %r8d
    movl 4(%rdi), %r9d
    movl 8(%rdi), %r10d
    movl 12(%rdi), %r11d
    movl 16(%rdi), %r12d
    movl 20(%rdi), %r13d
    movl 24(%rdi), %r14d
    movl 28(%rdi), %r15d
    movl $0x6a09e667, 0(%rsp)
    movl $0xbb67ae85, 4(%rsp)
    movl $0x3c6ef372, 8(%rsp)
    movl $0xa54ff53a, 12(%rsp)
    movl 32(%rdi), %eax
    xorl $0x510e527f, %eax
    movl %eax, 16(%rsp)
    movl 36(%rdi), %eax
    xorl $0x9b05688c, %eax
    movl %eax, 20(%rsp)
    movl 40(%rdi), %eax
    xorl $0x1f83d9ab, %eax
    movl %eax, 24(%rsp)
    movl 44(%rdi), %eax
    xorl $0x5be0cd19, %eax
    movl %eax, 28(%rsp)
    # Round 0
    g %r8d, %r12d, 0, 16, 0, 4
    g %r9d, %r13d, 4, 20, 8, 12
    g %r10d, %r14d, 8, 24, 16, 20
    g %r11d, %r15d, 12, 28, 24, 28
    g %r8d, %r13d, 8, 28, 32, 36
    g %r9d, %r14d, 12, 16, 40, 44
    g %r10d, %r15d, 0, 20, 48, 52
    g %r11d, %r12d, 4, 24, 56, 60
    # Round 1
    g %r8d, %r12d, 0, 16, 56, 40
    g %r9d, %r13d, 4, 20, 16, 32
    g %r10d, %r14d, 8, 24, 36, 60
    g %r11d, %r15d, 12, 28, 52, 24
    g %r8d, %r13d, 8, 28, 4, 48
    g %r9d, %r14d, 12, 16, 0, 8
    g %r10d, %r15d, 0, 20, 44, 28
    g %r11d, %r12d, 4, 24, 20, 12
    # Round 2
    g %r8d, %r12d, 0, 16, 44, 32
    g %r9d, %r13d, 4, 20, 48, 0
    g %r10d, %r14d, 8, 24, 20, 8
    g %r11d, %r15d, 12, 28, 60, 52
    g %r8d, %r13d, 8, 28, 40, 56
    g %r9d, %r14d, 12, 16, 12, 24
    g %r10d, %r15d, 0, 20, 28, 4
    g %r11d, %r12d, 4, 24, 36, 16
    # Round 3
    g %r8d, %r12d, 0, 16, 28, 36
    g %r9d, %r13d, 4, 20, 12, 4
    g %r10d, %r14d, 8, 24, 52, 48
    g %r11d, %r15d, 12, 28, 44, 56
    g %r8d, %r13d, 8, 28, 8, 24
    g %r9d, %r14d, 12, 16, 20, 40
    g %r10d, %r15d, 0, 20, 16, 0
    g %r11d, %r12d, 4, 24, 60, 32
    # Round 4
    g %r8d, %r12d, 0, 16, 36, 0
    g %r9d, %r13d, 4, 20, 20, 28
    g %r10d, %r14d, 8, 24, 8, 16
    g %r11d, %r15d, 12, 28, 40, 60
    g %r8d, %r13d, 8, 28, 56, 4
    g %r9d, %r14d, 12, 16, 44, 48
    g %r10d, %r15d, 0, 20, 24, 32
    g %r11d, %r12d, 4, 24, 12, 52
    # Round 5
    g %r8d, %r12d, 0, 16, 8, 48
    g %r9d, %r13d, 4, 20, 24, 40
    g %r10d, %r14d, 8, 24, 0, 44
    g %r11d, %r15d, 12, 28, 32, 12
    g %r8d, %r13d, 8, 28, 16, 52
    g %r9d, %r14d, 12, 16, 28, 20
    g %r10d, %r15d, 0, 20, 60, 56
    g %r11d, %r12d, 4, 24, 4, 36
    # Round 6
    g %r8d, %r12d, 0, 16, 48, 20
    g %r9d, %r13d, 4, 20, 4, 60
    g %r10d, %r14d, 8, 24, 56, 52
    g %r11d, %r15d, 12, 28, 16, 40
    g %r8d, %r13d, 8, 28, 0, 28
    g %r9d, %r14d, 12, 16, 24, 12
    g %r10d, %r15d, 0, 20, 36, 8
    g %r11d, %r12d, 4, 24, 32, 44
    # Round 7
    g %r8d, %r12d, 0, 16, 52, 44
    g %r9d, %r13d, 4, 20, 28, 56
    g %r10d, %r14d, 8, 24, 48, 4
    g %r11d, %r15d, 12, 28, 12, 36
    g %r8d, %r13d, 8, 28, 20, 0
    g %r9d, %r14d, 12, 16, 60, 16
    g %r10d, %r15d, 0, 20, 32, 24
    g %r11d, %r12d, 4, 24, 8, 40
    # Round 8
    g %r8d, %r12d, 0, 16, 24, 60
    g %r9d, %r13d, 4, 20, 56, 36
    g %r10d, %r14d, 8, 24, 44, 12
    g %r11d, %r15d, 12, 28, 0, 32
    g %r8d, %r13d, 8, 28, 48, 8
    g %r9d, %r14d, 12, 16, 52, 28
    g %r10d, %r15d, 0, 20, 4, 16
    g %r11d, %r12d, 4, 24, 40, 20
    # Round 9
    g %r8d, %r12d, 0, 16, 40, 8
    g %r9d, %r13d, 4, 20, 32, 16
    g %r10d, %r14d, 8, 24, 28, 24
    g %r11d, %r15d, 12, 28, 4, 20
    g %r8d, %r13d, 8, 28, 60, 44
    g %r9d, %r14d, 12, 16, 36, 56
    g %r10d, %r15d, 0, 20, 12, 48
    g %r11d, %r12d, 4, 24, 52, 0
    xorl 0(%rsp), %r8d
    xorl %r8d, 0(%rdi)
    xorl 4(%rsp), %r9d
    xorl %r9d, 4(%rdi)
    xorl 8(%rsp), %r10d
    xorl %r10d, 8(%rdi)
    xorl 12(%rsp), %r11d
    xorl %r11d, 12(%rdi)
    xorl 16(%rsp), %r12d
    xorl %r12d, 16(%rdi)
    xorl 20(%rsp), %r13d
    xorl %r13d, 20(%rdi)
    xorl 24(%rsp), %r14d
    xorl %r14d, 24(%rdi)
    xorl 28(%rsp), %r15d
    xorl %r15d, 28(%rdi)

    addq $64, %rbx
    decq %rdx
    jne .Lblock
    addq $32, %rsp
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %rbx
    popq %rbp
.Lempty:
    ret
.size blake2s_compress_generic, .-blake2s_compress_generic
.section .note.GNU-stack,"",@progbits
