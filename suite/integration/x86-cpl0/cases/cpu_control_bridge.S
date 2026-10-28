/* Finite CR0 and data checks. TCG execution is not a cache-coherency oracle. */
.text
.code64
.global guest_main
.extern cpu_control
guest_main:
    pushq $2
    popfq
    push %rbx
    sub $32, %rsp
    mov %cr0, %rbx

.macro check seed, after
    mov $\seed, %eax
    mov %rax, %cr0
    lea output(%rip), %rdi
    mov $128, %ecx
    mov $0xa5, %al
    cld
    rep stosb
    movabs $0xffffffff00000000, %rax
    mov %rax, 0(%rsp)
    movabs $0x7fffffffffffffff, %rax
    mov %rax, 8(%rsp)
    movabs $0x8000000000000000, %rax
    mov %rax, 16(%rsp)
    lea output(%rip), %rdi
    movabs $0x0123456789abcdef, %rsi
    movabs $0xfedcba9876543210, %rdx
    movabs $0x8877665544332211, %rcx
    movabs $0xdeadbeef11223344, %r8
    movabs $0x0000000100000002, %r9
    call cpu_control
    /* Literal seed-derived expectation, not an SMSW result from the payload. */
    mov %cr0, %rax
    mov $\after, %ecx
    cmp %rcx, %rax
    jne mismatch
    lea output(%rip), %rsi
    lea expected(%rip), %rdi
    mov $128, %ecx
    cld
    repe cmpsb
    jne mismatch
.endm
    check 0x8001003b, 0x80010033
    check 0x8005003b, 0x80050033
    check 0x80010033, 0x80010033
    xor %eax, %eax
    jmp restore
mismatch:
    mov $1, %eax
restore:
    mov %rbx, %cr0
    add $32, %rsp
    pop %rbx
    ret

.section .rodata,"a"
expected:
    .fill 8, 1, 0xa5
    .quad 0x0123456789abcdef
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211
    .quad 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000
    .quad 0x7fffffffffffffff, 0x8000000000000000
    .fill 48, 1, 0xa5
.bss
.balign 16
output: .skip 128
.section .note.GNU-stack,"",@progbits
