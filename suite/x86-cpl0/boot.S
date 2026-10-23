/* Flat multiboot entry is 32-bit even though the linked container is ELF64. */
.section .multiboot,"a"
.balign 4
.long 0x1badb002, 0x10003, -(0x1badb002 + 0x10003)
.long mb_header, image_start, load_end, bss_end, entry32
.set mb_header, image_start

.section .text.boot,"ax"
.code32
.global entry32
entry32:
    cli
    cld
    /* No stack or C until all BSS, including page tables, is initialized. */
    mov $bss_start, %edi
    mov $bss_end, %ecx
    sub %edi, %ecx
    xor %eax, %eax
    rep stosb
    mov $stack_top, %esp
    mov $(pdpt + 3), %eax
    mov %eax, pml4
    mov $(pd + 3), %eax
    mov %eax, pdpt
    xor %ecx, %ecx
1:
    mov %ecx, %eax
    shl $21, %eax
    or $0x83, %eax
    mov %eax, pd(,%ecx,8)
    inc %ecx
    cmp $512, %ecx
    jne 1b
    lgdt boot_gdtr
    mov %cr4, %eax
    or $0x20, %eax
    mov %eax, %cr4
    mov $pml4, %eax
    mov %eax, %cr3
    mov $0xc0000080, %ecx
    rdmsr
    or $0x100, %eax
    wrmsr
    mov %cr0, %eax
    or $0x80000001, %eax
    mov %eax, %cr0
    ljmp $8, $entry64
.code64
entry64:
    mov $16, %eax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss
    xor %eax, %eax
    mov %ax, %fs
    mov %ax, %gs
    mov $stack_top, %rsp
    xor %ebp, %ebp
    call harness_main
2:  cli
    hlt
    jmp 2b

.section .rodata,"a"
.balign 8
boot_gdt:
    .quad 0, 0x00af9a000000ffff, 0x00cf92000000ffff
boot_gdtr:
    .word 23
    .long boot_gdt
.section .bss,"aw",@nobits
.balign 4096
pml4: .skip 4096
pdpt: .skip 4096
pd: .skip 4096
.balign 16
    .skip 65536
stack_top:
.section .note.GNU-stack,"",@progbits
