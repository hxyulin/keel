/* Independent payload and normally returning broken controls. */
.text
.code64
.macro exchange
.ifndef KEEL_OMIT_SWAPGS
    swapgs
.ifdef KEEL_DOUBLE_SWAPGS
    swapgs
.endif
.endif
.ifdef KEEL_CLOBBER_SELECTOR
    push %rax
    push %rcx
    push %rdx
    mov $0xc0000101, %ecx
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    mov %rax, %r11
    mov $16, %eax
    mov %ax, %gs
    mov %r11, %rax
    mov %rax, %rdx
    shr $32, %rdx
    wrmsr
    pop %rdx
    pop %rcx
    pop %rax
.endif
.endm
.macro live offset
    mov %rsi, \offset(%rdi)
    mov %rdx, \offset+8(%rdi)
    mov %rcx, \offset+16(%rdi)
    mov %r8, \offset+24(%rdi)
    mov %r9, \offset+32(%rdi)
    mov 8(%rsp), %rax
    mov %rax, \offset+40(%rdi)
    mov 16(%rsp), %rax
    mov %rax, \offset+48(%rdi)
    mov 24(%rsp), %rax
    mov %rax, \offset+56(%rdi)
.ifdef KEEL_CLOBBER_LIVE
    movq $0, \offset(%rdi)
.endif
.endm
.global gs_exchange
gs_exchange:
    sub $8, %rsp
    exchange
    call gs_observe
    add $8, %rsp
    mov %rax, 8(%rdi)
    live 16
    ret
.global gs_roundtrip
gs_roundtrip:
    sub $24, %rsp
    exchange
    call gs_observe
    mov %rax, (%rsp)
    exchange
    call gs_observe
    mov %rax, 16(%rdi)
    mov (%rsp), %rax
    mov %rax, 8(%rdi)
    add $24, %rsp
    live 24
    ret
.global gs_exchange_raw
gs_exchange_raw:
    exchange
.ifdef KEEL_NARROW_BASE
    push %rax
    push %rcx
    push %rdx
    mov $0xc0000101, %ecx
    rdmsr
    xor %edx, %edx
    wrmsr
    pop %rdx
    pop %rcx
    pop %rax
.endif
    live 8
    ret
.section .note.GNU-stack,"",@progbits
