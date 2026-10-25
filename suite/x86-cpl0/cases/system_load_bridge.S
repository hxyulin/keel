/* Trusted setup/observations for generated LLDT/LTR/LMSW reg and mem forms. */
.text
.code64
.global guest_main
.extern system_reg
.extern system_mem
guest_main:
    pushq $2
    popfq
    push %rbx
    mov %cr0, %rbx
.macro descriptor table, offset
    lea \table(%rip), %rax
    mov %ax, gdt+\offset+2(%rip)
    shr $16, %rax
    mov %al, gdt+\offset+4(%rip)
    mov %ah, gdt+\offset+7(%rip)
.endm
    descriptor ldt, 24
    descriptor tss_a, 40
    descriptor tss_b, 56
    lgdt gdtr(%rip)
    xor %eax, %eax
    lldt %ax
    mov $0x38, %eax
    ltr %ax
.macro prepare cr0
    mov $\cr0, %eax
    mov %rax, %cr0
    lea output(%rip), %rdi
    mov $32, %ecx
    mov $0xa5, %al
    cld
    rep stosb
    lea output(%rip), %rdi
.endm
.macro verify ldtr, tr, cr0, image
    sldt %ax
    cmp $\ldtr, %ax
    jne mismatch
    str %ax
    cmp $\tr, %ax
    jne mismatch
    mov %cr0, %rax
    mov $\cr0, %ecx
    cmp %rcx, %rax
    jne mismatch
    lea output(%rip), %rsi
    lea \image(%rip), %rdi
    mov $32, %ecx
    cld
    repe cmpsb
    jne mismatch
.endm
    prepare 0x80010033
    mov $0x18, %esi
    mov $0x28, %edx
    mov $0xfffe, %ecx
    movabs $0x0123456789abcdef, %r8
    call system_reg
    verify 0x18, 0x28, 0x8001003f, expected_reg
    cmpb $0x8b, gdt+45(%rip)
    jne mismatch
    /* TR now names TSS A; make TSS B available again for the second LTR. */
    movb $0x89, gdt+61(%rip)
    prepare 0x8005003b
    lea inputs+1(%rip), %rsi
    lea inputs+5(%rip), %rdx
    lea inputs+9(%rip), %rcx
    movabs $0xfedcba9876543210, %r8
    call system_mem
    verify 0, 0x38, 0x80050031, expected_mem
    cmpb $0x8b, gdt+45(%rip)
    jne mismatch
    cmpb $0x8b, gdt+61(%rip)
    jne mismatch
    lea inputs(%rip), %rsi
    lea expected_inputs(%rip), %rdi
    mov $16, %ecx
    cld
    repe cmpsb
    jne mismatch
    xor %eax, %eax
    jmp restore
mismatch:
    mov $1, %eax
restore:
    mov %rbx, %cr0
    pop %rbx
    ret

.data
.balign 16
gdt:
    .quad 0, 0x00af9a000000ffff, 0x00cf92000000ffff
    .quad 0x000082000000000f, 0
    .quad 0x0000890000000067, 0
    .quad 0x0000890000000067, 0
gdt_end:
.balign 16
ldt: .zero 16
tss_a: .zero 102
    .word 104
tss_b: .zero 102
    .word 104
inputs:
    .byte 0xa5
    .word 0
    .fill 2, 1, 0xa5
    .word 0x38
    .fill 2, 1, 0xa5
    .word 0xfff0
    .fill 5, 1, 0xa5
.section .rodata,"a"
gdtr:
    .word gdt_end - gdt - 1
    .quad gdt
.macro image name, ldtr, tr, msw, live
\name:
    .byte 0xa5
    .word \ldtr
    .fill 2, 1, 0xa5
    .word \tr
    .fill 2, 1, 0xa5
    .word \msw
    .fill 5, 1, 0xa5
    .quad \live
    .fill 8, 1, 0xa5
.endm
image expected_reg, 0x18, 0x28, 0xfffe, 0x0123456789abcdef
image expected_mem, 0, 0x38, 0xfff0, 0xfedcba9876543210
expected_inputs:
    .byte 0xa5
    .word 0
    .fill 2, 1, 0xa5
    .word 0x38
    .fill 2, 1, 0xa5
    .word 0xfff0
    .fill 5, 1, 0xa5
.bss
.balign 16
output: .skip 32
.section .note.GNU-stack,"",@progbits
