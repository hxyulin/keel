/* Independent SysV payloads and normally returning broken controls. */
.text
.code64
.macro payload name, kind
.global \name
.type \name,@function
\name:
.if \kind == 0
.ifndef KEEL_OMIT_CLI
    cli
.endif
.elseif \kind == 1
.ifndef KEEL_OMIT_STI
    sti
.endif
.elseif \kind == 2
.ifndef KEEL_OMIT_HLT
    hlt
.endif
.else
    sti
    hlt
.endif
    mov %rsi, 8(%rdi)
    mov %rdx, 16(%rdi)
    mov %rcx, 24(%rdi)
    mov %r8, 32(%rdi)
    mov %r9, 40(%rdi)
    mov 8(%rsp), %rax
    mov %rax, 48(%rdi)
    mov 16(%rsp), %rax
    mov %rax, 56(%rdi)
    mov 24(%rsp), %rax
    mov %rax, 64(%rdi)
.ifdef KEEL_CLOBBER_LIVE
    movq $0, 8(%rdi)
.endif
    ret
.size \name,.-\name
.endm
payload interrupt_disable,0
payload interrupt_enable,1
payload interrupt_halt,2
payload interrupt_enable_halt,3
.section .note.GNU-stack,"",@progbits
