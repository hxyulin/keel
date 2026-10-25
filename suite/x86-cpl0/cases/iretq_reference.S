/* Independent GNU-assembled transport, caller and normally failing controls. */
.text
.code64
.global resume_owned
resume_owned:
    mov %rdx,8(%rsi)
    mov %rcx,16(%rsi)
    mov %r8,24(%rsi)
    mov %r9,32(%rsi)
    mov 8(%rsp),%rax
    mov %rax,40(%rsi)
    mov 16(%rsp),%rax
    mov %rax,48(%rsi)
    mov 24(%rsp),%rax
    mov %rax,56(%rsi)
    mov 32(%rsp),%rax
    mov %rax,64(%rsi)
.ifdef KEEL_CLOBBER_LIVE
    movq $0,8(%rsi)
.endif
.ifdef KEEL_WRONG_FLAGS
    xorq $1,136(%rdi)
.endif
.ifdef KEEL_CLOBBER_IMAGE
    movb $0,160(%rdi)
.endif
.ifdef KEEL_SKIP_PRIVILEGE
    movq $8,128(%rdi)
    movq $16,152(%rdi)
.endif
    mov %rdi,%r10
    mov 0(%r10),%rax
    mov 8(%r10),%rcx
    mov 16(%r10),%rdx
    mov 24(%r10),%rbx
    mov 32(%r10),%rbp
    mov 40(%r10),%rsi
    mov 48(%r10),%rdi
    mov 56(%r10),%r8
    mov 64(%r10),%r9
    mov 80(%r10),%r11
    mov 88(%r10),%r12
    mov 96(%r10),%r13
    mov 104(%r10),%r14
    mov 112(%r10),%r15
    lea 120(%r10),%rsp
.ifndef KEEL_OMIT_R10
    mov 72(%r10),%r10
.endif
.ifdef KEEL_USE_RET
    ret
.else
    iretq
.endif
.section .note.GNU-stack,"",@progbits
