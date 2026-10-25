/* Trusted setup and literal oracle for twelve SLDT/STR/SMSW forms.
 * All execution stays at CPL0 in long mode with IF=0. No fault recovery. */
.section .text,"ax"
.code64
.global guest_main
.extern descriptor_status
guest_main:
    /* Establish IF=DF=AC=TF=NT=0 rather than inheriting boot flag values. */
    pushq $2
    popfq
    push %rbx
    /* The low-memory linker places the entire image below 32 MiB. Build the
     * two 16-byte system descriptors from linker-known addresses. */
    lea ldt(%rip), %rax
    mov %ax, gdt+26(%rip)
    shr $16, %rax
    mov %al, gdt+28(%rip)
    mov %ah, gdt+31(%rip)
    lea tss(%rip), %rax
    mov %ax, gdt+42(%rip)
    shr $16, %rax
    mov %al, gdt+44(%rip)
    mov %ah, gdt+47(%rip)
    lgdt gdtr(%rip)
    mov $0x18, %eax
    lldt %ax
    mov $0x28, %eax
    ltr %ax
    /* PG/WP/NE/ET/MP/PE; preserve the existing long-mode page tables. */
    mov $0x80010033, %eax
    mov %rax, %cr0
    lea expected_a(%rip), %rbx
    movabs $0xdeadbeefcafe7654, %rsi
    call check_case
    test %eax, %eax
    jnz done
    /* A null LDTR, AM and TS exercise different status and selector values.
     * Neither the generated payload nor the bridge uses FP/SIMD or AC=1. */
    xor %eax, %eax
    lldt %ax
    mov $0x8005003b, %eax
    mov %rax, %cr0
    lea expected_b(%rip), %rbx
    movabs $0x0123456789abfedc, %rsi
    call check_case
done:
    pop %rbx
    ret

check_case:
    lea output(%rip), %rdi
    mov $128, %ecx
    mov $0xa5, %al
    cld
    rep stosb
    lea output(%rip), %rdi
    sub $8, %rsp
    call descriptor_status
    add $8, %rsp
    lea output(%rip), %rsi
    mov %rbx, %rdi
    mov $128, %ecx
    cld
    repe cmpsb
    jne mismatch
    xor %eax, %eax
    ret
mismatch:
    mov $1, %eax
    ret

.section .data,"aw"
.balign 16
gdt:
    .quad 0, 0x00af9a000000ffff, 0x00cf92000000ffff
    /* Present LDT (type 2), limit 15; base filled above, upper base zero. */
    .quad 0x000082000000000f, 0
    /* Available 64-bit TSS (type 9), limit 103; LTR sets its busy bit. */
    .quad 0x0000890000000067, 0
gdt_end:
.balign 16
ldt: .zero 16
.balign 16
tss:
    .zero 102
    .word 104

.section .rodata,"a"
gdtr:
    .word gdt_end - gdt - 1
    .quad gdt
/* Expectations use setup constants, never values read by a status query. */
.macro expected name, ldtr, cr0, old
\name:
    .byte 0xa5
    .word \ldtr
    .fill 2, 1, 0xa5
    .word 0x28
    .fill 2, 1, 0xa5
    .word (\cr0 & 0xffff)
    .fill 5, 1, 0xa5
    .quad (\old & ~0xffff) | \ldtr, \ldtr, \ldtr
    .quad (\old & ~0xffff) | 0x28, 0x28, 0x28
    .quad (\old & ~0xffff) | (\cr0 & 0xffff), \cr0, \cr0
    .fill 8, 1, 0xa5
    .quad \old
    .fill 24, 1, 0xa5
.endm
expected expected_a, 0x18, 0x80010033, 0xdeadbeefcafe7654
expected expected_b, 0, 0x8005003b, 0x0123456789abfedc

.section .bss,"aw",@nobits
.balign 16
output: .skip 128
.section .note.GNU-stack,"",@progbits
