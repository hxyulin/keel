/* Independent MSR, selector and effective-address observations. IF stays clear. */
.text
.code64
.global guest_main
guest_main:
    pushq $2
    popfq
    push %rbx
    push %r12
    push %r13
    sub $32, %rsp
    xor %r13d, %r13d
    mov %gs, %ax
    mov %ax, saved_selector(%rip)
    mov $0xc0000101, %ecx
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    mov %rax, %rbx
    mov $0xc0000102, %ecx
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    mov %rax, %r12
.macro write_msr index, value
    mov $\index, %ecx
    movabs $\value, %rax
    mov %rax, %rdx
    shr $32, %rdx
    wrmsr
.endm
.macro prepare
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
.endm
.macro verify active, inactive, selector, image
    mov $0xc0000101, %ecx
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    movabs $\active, %rdx
    cmp %rdx, %rax
    jne mismatch
    mov $0xc0000102, %ecx
    rdmsr
    shl $32, %rdx
    or %rdx, %rax
    movabs $\inactive, %rdx
    cmp %rdx, %rax
    jne mismatch
    mov %gs, %ax
    cmp $\selector, %ax
    jne mismatch
    lea output(%rip), %rsi
    lea \image(%rip), %rdi
    mov $128, %ecx
    cld
    repe cmpsb
    jne mismatch
.endm
.macro effective first, second
    mov %gs:0, %rax
    movabs $\first, %rdx
    cmp %rdx, %rax
    jne mismatch
    mov %gs:8, %rax
    movabs $\second, %rdx
    cmp %rdx, %rax
    jne mismatch
.endm
.macro mapped selector, active, inactive, first_image, second_image, pair_image, active0, active1, inactive0, inactive1
    mov $\selector, %eax
    mov %ax, %gs
    write_msr 0xc0000101, \active
    write_msr 0xc0000102, \inactive
    prepare
    call gs_exchange
    verify \inactive, \active, \selector, \first_image
    effective \inactive0, \inactive1
    prepare
    call gs_exchange
    verify \active, \inactive, \selector, \second_image
    effective \active0, \active1
    prepare
    call gs_roundtrip
    verify \active, \inactive, \selector, \pair_image
    effective \active0, \active1
.endm
    mapped 0, gs_a, gs_b, expected_b, expected_a, expected_ba, 0xa11ce00d12345678, 0xa11ce00d87654321, 0xbadc0ffe11223344, 0xbadc0ffe55667788
    mapped 16, gs_b, gs_a, expected_a, expected_b, expected_ab, 0xbadc0ffe11223344, 0xbadc0ffe55667788, 0xa11ce00d12345678, 0xa11ce00d87654321
    /* Wide canonical bases are not dereferenced in this phase. */
    write_msr 0xc0000101, 0xffff800012345000
    write_msr 0xc0000102, 0x000023456789a000
    prepare
    call gs_exchange_raw
    verify 0x000023456789a000, 0xffff800012345000, 16, expected_raw
    prepare
    call gs_exchange_raw
    verify 0xffff800012345000, 0x000023456789a000, 16, expected_raw
    jmp restore
mismatch:
    mov $1, %r13d
restore:
    /* A selector reload may affect the base, so restore it before both MSRs. */
    mov saved_selector(%rip), %ax
    mov %ax, %gs
    mov $0xc0000101, %ecx
    mov %rbx, %rax
    mov %rax, %rdx
    shr $32, %rdx
    wrmsr
    mov $0xc0000102, %ecx
    mov %r12, %rax
    mov %rax, %rdx
    shr $32, %rdx
    wrmsr
    mov %r13d, %eax
    add $32, %rsp
    pop %r13
    pop %r12
    pop %rbx
    ret
/* The generated code cannot manufacture this effective-address observation. */
.global gs_observe
.type gs_observe,@function
gs_observe:
    mov %gs:0, %rax
    ret
.size gs_observe,.-gs_observe
.section .rodata,"a"
gs_a: .quad 0xa11ce00d12345678, 0xa11ce00d87654321
gs_b: .quad 0xbadc0ffe11223344, 0xbadc0ffe55667788
expected_a:
    .fill 8,1,0xa5
    .quad 0xa11ce00d12345678, 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 48,1,0xa5
expected_b:
    .fill 8,1,0xa5
    .quad 0xbadc0ffe11223344, 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 48,1,0xa5
expected_ab:
    .fill 8,1,0xa5
    .quad 0xa11ce00d12345678, 0xbadc0ffe11223344, 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 40,1,0xa5
expected_ba:
    .fill 8,1,0xa5
    .quad 0xbadc0ffe11223344, 0xa11ce00d12345678, 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 40,1,0xa5
expected_raw:
    .fill 8,1,0xa5
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 56,1,0xa5
.bss
.balign 16
output: .skip 128
saved_selector: .skip 2
.section .note.GNU-stack,"",@progbits
