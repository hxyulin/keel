/* Separately assembled positive reference and targeted negative controls. */
.text
.code64
.global msr_observe
msr_observe:
    mov %rdx, %r10
    mov %rcx, %r11
    mov %esi, %ecx
.ifdef KEEL_WRONG_INDEX
    xor $3, %ecx
.endif
    rdmsr
.ifndef KEEL_NARROW_READ
    shl $32, %rdx
    or %rdx, %rax
.endif
    mov %rax, 8(%rdi)
    mov %r10, %rax
    mov %rax, %rdx
    shr $32, %rdx
.ifndef KEEL_OMIT_WRITE
    wrmsr
.endif
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    mov %rax, 16(%rdi)
    rdmsr
    mov %r10, 24(%rdi)
    mov %esi, %eax
    mov %rax, 32(%rdi)
.ifdef KEEL_CLOBBER_LIVE
    xor %r11, %r11
.endif
    mov %r11, 40(%rdi)
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
    mov 40(%rsp), %rax
    mov %rax, 96(%rdi)
    ret
.global msr_constant
msr_constant:
    mov $0xc0000102, %ecx
    movabs $0x000056789abcdef0, %rax
    mov %rax, %rdx
    shr $32, %rdx
    wrmsr
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    ret
.section .note.GNU-stack,"",@progbits
