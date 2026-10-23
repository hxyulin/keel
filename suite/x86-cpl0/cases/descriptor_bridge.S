/* Bounded SGDT/SIDT oracle. Assemble as ELF64; no C runtime or SIMD state.
 * The seed and expected images share linker-known table addresses, never query
 * results. Tables live for the whole guest. Selectors 8/16 match boot.S. */
.section .text,"ax"
.code64
.global guest_main
.extern descriptor_observe
guest_main:
    mov %cs, %ax
    cmp $8, %ax
    jne mismatch
    mov %ds, %ax
    cmp $16, %ax
    jne mismatch
    mov %es, %ax
    cmp $16, %ax
    jne mismatch
    mov %ss, %ax
    cmp $16, %ax
    jne mismatch
    lgdt seeded_gdtr(%rip)
    lidt seeded_idtr(%rip)
    lea output(%rip), %rdi
    mov $32, %ecx
    mov $0xa5, %al
    cld
    rep stosb
    lea output(%rip), %rdi
    sub $8, %rsp
    /* Opaque, separately linked Keel object: reference bytes cannot be folded
     * into or substituted for the two observations by a bridge compiler. */
    call descriptor_observe
    add $8, %rsp
    lea output(%rip), %rsi
    lea expected(%rip), %rdi
    mov $32, %ecx
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
seeded_gdt:
    .quad 0, 0x00af9a000000ffff, 0x00cf92000000ffff, 0
seeded_gdt_end:
/* No exception resumption: invalid gates deliberately fail closed. IF is
 * already zero and remains zero. This is not an interrupt/exception fixture. */
.balign 16
seeded_idt: .zero 512
seeded_idt_end:

.section .rodata,"a"
seeded_gdtr:
    .word seeded_gdt_end - seeded_gdt - 1
    .quad seeded_gdt
seeded_idtr:
    .word seeded_idt_end - seeded_idt - 1
    .quad seeded_idt
/* Independently encoded complete output, including all twelve canary bytes. */
expected:
    .byte 0xa5
    .word 31
    .quad seeded_gdt
    .fill 6, 1, 0xa5
    .word 511
    .quad seeded_idt
    .fill 5, 1, 0xa5

.section .bss,"aw",@nobits
.balign 16
output: .skip 32
.section .note.GNU-stack,"",@progbits
