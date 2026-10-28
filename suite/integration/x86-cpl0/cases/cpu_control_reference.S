/* Independent reference, negative controls and a cache-omission scope control. */
.text
.code64
.global cpu_control
cpu_control:
    mov %rsi, 8(%rdi)
.ifndef KEEL_OMIT_CLTS
    clts
.endif
.ifndef KEEL_OMIT_CACHE
    wbinvd
    invd
.endif
.ifdef KEEL_ALTER_CR0
    mov %cr0, %rax
    xor $0x10000, %rax
    mov %rax, %cr0
.endif
.ifdef KEEL_CLOBBER_LIVE
    xor %esi, %esi
.endif
    mov %rsi, 16(%rdi)
    mov %rdx, 24(%rdi)
    mov %rcx, 32(%rdi)
    mov %r8, 40(%rdi)
    mov %r9, 48(%rdi)
    mov 8(%rsp), %rax
    mov %rax, 56(%rdi)
    mov 16(%rsp), %rax
    mov %rax, 64(%rdi)
    mov 24(%rsp), %rax
    mov %rax, 72(%rdi)
    ret
.section .note.GNU-stack,"",@progbits
