/* Independent ABI reference and finite, normally returning broken controls. */
.text
.code64
.macro payload name, reg
.global \name
\name:
    mov %\reg, %rax
.ifndef KEEL_OMIT_WRITE
    mov %rsi, %\reg
.endif
    mov %\reg, %r10
    mov %\reg, %r11
.ifdef KEEL_NARROW_READ
    mov %eax, %eax
    mov %r10d, %r10d
.endif
    mov %rax, 8(%rdi)
    mov %r10, 16(%rdi)
    mov %rsi, 24(%rdi)
    mov %rdx, 32(%rdi)
    mov %rcx, 40(%rdi)
    mov %r8, 48(%rdi)
    mov %r9, 56(%rdi)
    mov 8(%rsp), %rax
    mov %rax, 64(%rdi)
    mov 16(%rsp), %rax
    mov %rax, 72(%rdi)
    mov 24(%rsp), %rax
    mov %rax, 80(%rdi)
    mov 32(%rsp), %rax
    mov %rax, 88(%rdi)
.ifdef KEEL_CLOBBER_LIVE
    movq $0, 32(%rdi)
.endif
    ret
.endm
payload system_cr0, cr0
payload system_cr2, cr2
payload system_cr3, cr3
payload system_cr4, cr4
payload system_cr8, cr8
.ifdef KEEL_WRONG_REGISTER
payload system_dr0, dr1
.else
payload system_dr0, dr0
.endif
payload system_dr1, dr1
payload system_dr2, dr2
payload system_dr3, dr3
payload system_dr4, dr4
payload system_dr5, dr5
payload system_dr6, dr6
payload system_dr7, dr7
.global system_constant
system_constant:
    movabs $0x000056789abcdef0, %rax
    mov %rax, %dr0
    mov %dr0, %rax
    ret
.section .note.GNU-stack,"",@progbits
