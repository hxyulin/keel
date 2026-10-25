.text
.code64
.global system_reg
.global system_mem
system_reg:
.ifndef KEEL_OMIT_LLDT
    lldt %si
.endif
.ifndef KEEL_OMIT_LTR
    ltr %dx
.endif
.ifndef KEEL_OMIT_LMSW
    lmsw %cx
.endif
    mov %si, 1(%rdi)
    mov %dx, 5(%rdi)
    mov %cx, 9(%rdi)
.ifdef KEEL_CLOBBER_LIVE
    xor %r8d, %r8d
.endif
    mov %r8, 16(%rdi)
    ret
system_mem:
.ifndef KEEL_OMIT_LLDT
    lldt (%rsi)
.endif
.ifndef KEEL_OMIT_LTR
    ltr (%rdx)
.endif
.ifndef KEEL_OMIT_LMSW
    lmsw (%rcx)
.endif
    mov (%rsi), %ax
    mov %ax, 1(%rdi)
    mov (%rdx), %ax
    mov %ax, 5(%rdi)
    mov (%rcx), %ax
    mov %ax, 9(%rdi)
    mov %r8, 16(%rdi)
    ret
.section .note.GNU-stack,"",@progbits
