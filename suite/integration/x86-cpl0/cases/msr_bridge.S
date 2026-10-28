/* Two canonical GS-base MSRs, with independent state/output checks. The
 * fixture and generated code use no GS-relative accesses or SWAPGS. */
.text
.code64
.global guest_main
.extern msr_observe
.extern msr_constant
guest_main:
    pushq $2
    popfq
    push %rbx
    push %r12
    push %r13
    sub $48, %rsp
    xor %r13d, %r13d
    mov $0xc0000102, %ecx
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    mov %rax, %rbx
    mov $0xc0000101, %ecx
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    mov %rax, %r12

.macro check index, before, after, expected
    mov $\index, %ecx
    movabs $\before, %rax
    mov %rax, %rdx
    shr $32, %rdx
    wrmsr
    lea output(%rip), %rdi
    mov $128, %ecx
    mov $0xa5, %al
    cld
    rep stosb
    movabs $0xdeadbeef11223344, %rax
    mov %rax, 0(%rsp)
    movabs $0x0000000100000002, %rax
    mov %rax, 8(%rsp)
    movabs $0xffffffff00000000, %rax
    mov %rax, 16(%rsp)
    movabs $0x7fffffffffffffff, %rax
    mov %rax, 24(%rsp)
    movabs $0x8000000000000000, %rax
    mov %rax, 32(%rsp)
    lea output(%rip), %rdi
    mov $\index, %esi
    movabs $\after, %rdx
    movabs $0x0123456789abcdef, %rcx
    movabs $0xfedcba9876543210, %r8
    movabs $0x8877665544332211, %r9
    call msr_observe
    /* Read the actual target independently of any output supplied by Keel. */
    mov $\index, %ecx
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    movabs $\after, %rdx
    cmp %rdx, %rax
    jne mismatch
    lea output(%rip), %rsi
    lea \expected(%rip), %rdi
    mov $128, %ecx
    cld
    repe cmpsb
    jne mismatch
.endm
    check 0xc0000102, 0x00001234deadbeef, 0xffffabcd01234567, expected_a
    check 0xc0000101, 0xfffffeba89abcdef, 0x0000456789abcdef, expected_b
    /* Constant operands exercise literal lowering independently of ABI
     * argument homes. Check both the returned value and the actual state. */
    call msr_constant
    movabs $0x000056789abcdef0, %rdx
    cmp %rdx, %rax
    jne mismatch
    mov $0xc0000102, %ecx
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    movabs $0x000056789abcdef0, %rdx
    cmp %rdx, %rax
    jne mismatch
    jmp restore
mismatch:
    mov $1, %r13d
restore:
    mov $0xc0000102, %ecx
    mov %rbx, %rax
    mov %rax, %rdx
    shr $32, %rdx
    wrmsr
    mov $0xc0000101, %ecx
    mov %r12, %rax
    mov %rax, %rdx
    shr $32, %rdx
    wrmsr
    mov %r13d, %eax
    add $48, %rsp
    pop %r13
    pop %r12
    pop %rbx
    ret

.section .rodata,"a"
.macro expected name, index, before, after
\name:
    .fill 8, 1, 0xa5
    .quad \before, \after, \after, \index
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211
    .quad 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000
    .quad 0x7fffffffffffffff, 0x8000000000000000
    .fill 24, 1, 0xa5
.endm
expected expected_a, 0xc0000102, 0x00001234deadbeef, 0xffffabcd01234567
expected expected_b, 0xc0000101, 0xfffffeba89abcdef, 0x0000456789abcdef

.bss
.balign 16
output: .skip 128
.section .note.GNU-stack,"",@progbits
