/* Independent implementation and narrowly broken harness controls. */
.text
.code64
.global descriptor_status
descriptor_status:
    sldt 1(%rdi)
    str 5(%rdi)
    smsw 9(%rdi)
.ifdef KEEL_WIDE_MEMORY
    /* Simulate an m16 operation incorrectly widened into its canaries. */
    movw $0, 3(%rdi)
.endif
    mov %rsi, %rax
    sldt %ax
.ifdef KEEL_NORMALIZE_R16
    movzwl %ax, %eax
.endif
    mov %rax, 16(%rdi)
    mov %rsi, %rax
    sldt %eax
    mov %rax, 24(%rdi)
    mov %rsi, %rax
    sldt %rax
    mov %rax, 32(%rdi)
    mov %rsi, %rax
    str %ax
    mov %rax, 40(%rdi)
    mov %rsi, %rax
    str %eax
    mov %rax, 48(%rdi)
    mov %rsi, %rax
    str %rax
    mov %rax, 56(%rdi)
    mov %rsi, %rax
    smsw %ax
    mov %rax, 64(%rdi)
    mov %rsi, %rax
    smsw %eax
.ifdef KEEL_NARROW_SMSW
    movzwl %ax, %eax
.endif
    mov %rax, 72(%rdi)
    mov %rsi, %rax
    smsw %rax
.ifdef KEEL_NARROW_SMSW
    movzwl %ax, %eax
.endif
    mov %rax, 80(%rdi)
.ifdef KEEL_CLOBBER_OLD
    xor %esi, %esi
.endif
    mov %rsi, 96(%rdi)
    ret
.section .note.GNU-stack,"",@progbits
