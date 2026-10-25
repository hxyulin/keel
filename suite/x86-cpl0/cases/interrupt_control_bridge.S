/* Private CPL0 IRQ context. Generated payloads only access output/SSA args. */
.text
.code64
.global guest_main
guest_main:
    pushq $2
    popfq
    push %rbx
    sub $32, %rsp
    /* Preserve boot CS/SS meanings; add a private TSS with an IST1 stack. */
    lea tss(%rip), %rax
    mov %ax, gdt+26(%rip)
    shr $16, %rax
    mov %al, gdt+28(%rip)
    mov %ah, gdt+31(%rip)
    shr $16, %rax
    mov %eax, gdt+32(%rip)
    lea irq_stack_top(%rip), %rax
    mov %rax, tss+36(%rip)
    lgdt gdtr(%rip)
    mov $24, %eax
    ltr %ax
    lea irq_handler(%rip), %rax
    mov %ax, idt+512(%rip)
    movw $8, idt+514(%rip)
    movb $1, idt+516(%rip)
    movb $0x8e, idt+517(%rip)
    shr $16, %rax
    mov %ax, idt+518(%rip)
    shr $16, %rax
    mov %eax, idt+520(%rip)
    lidt idtr(%rip)
    call setup_pic

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
.macro output_ok
    lea output(%rip), %rsi
    lea expected(%rip), %rdi
    mov $128, %ecx
    cld
    repe cmpsb
    jne mismatch
.endm
.macro irq_ok slot
    test $0x200, %eax
    jz mismatch
    cmpl $1, irq_count(%rip)
    jne mismatch
    cmpl $1, irq_stack_ok(%rip)
    jne mismatch
    testq $0x200, irq_saved_flags(%rip)
    jz mismatch
    testq $0x200, irq_handler_flags(%rip)
    jnz mismatch
    mov irq_rip(%rip), %rax
    mov %rax, \slot(%rip)
    output_ok
.endm
    prepare
    sti
    call interrupt_disable
    pushfq
    pop %rax
    cli
    test $0x200, %eax
    jnz mismatch
    output_ok
    prepare
    call interrupt_enable
    pushfq
    pop %rax
    cli
    test $0x200, %eax
    jz mismatch
    output_ok
    prepare
    call arm_timer
    sti
    call interrupt_halt
    pushfq
    pop %rax
    cli
    irq_ok raw_resume
    prepare
    call arm_timer
    call await_pending
    jc mismatch
    /* IF is clear and IRQ0 is now pending before entering the pair. */
    call interrupt_enable_halt
    pushfq
    pop %rax
    cli
    irq_ok pair_resume
    lea raw_label(%rip), %rsi
    call print_string
    mov raw_resume(%rip), %rax
    call print_hex
    lea pair_label(%rip), %rsi
    call print_string
    mov pair_resume(%rip), %rax
    call print_hex
    xor %ebx, %ebx
    jmp cleanup
mismatch:
    mov $1, %ebx
cleanup:
    cli
    mov $0xff, %al
    outb %al, $0x21
    outb %al, $0xa1
    mov %ebx, %eax
    add $32, %rsp
    pop %rbx
    ret
setup_pic:
    /* Reinitialize both PICs; only IRQ0 will be unmasked for each wait. */
    mov $0x11, %al
    outb %al, $0x20
    outb %al, $0xa0
    mov $0x20, %al
    outb %al, $0x21
    mov $0x28, %al
    outb %al, $0xa1
    mov $4, %al
    outb %al, $0x21
    mov $2, %al
    outb %al, $0xa1
    mov $1, %al
    outb %al, $0x21
    outb %al, $0xa1
    mov $0xff, %al
    outb %al, $0x21
    outb %al, $0xa1
    ret
arm_timer:
    movl $0, irq_count(%rip)
    movl $0, irq_stack_ok(%rip)
    movq $0, irq_rip(%rip)
    movq $0, irq_saved_flags(%rip)
    movq $0, irq_handler_flags(%rip)
    mov $0xff, %al
    outb %al, $0x21
    /* PIT channel 0, one-shot mode 0, count 65535 (~55 ms). */
    mov $0x30, %al
    outb %al, $0x43
    mov $0xff, %al
    outb %al, $0x40
    outb %al, $0x40
    /* Clear any previously latched edge after forcing the PIT output low. */
    call setup_pic
    mov $0xfe, %al
    outb %al, $0x21
    ret
await_pending:
    mov $0x0a, %al
    outb %al, $0x20
    mov $100000000, %r10d
1:  inb $0x20, %al
    test $1, %al
    jnz 2f
    dec %r10d
    jnz 1b
    stc
    ret
2:  clc
    ret
irq_handler:
    push %rax
    push %rcx
    push %rdx
    push %rbx
    push %rbp
    push %rsi
    push %rdi
    push %r8
    push %r9
    push %r10
    push %r11
    push %r12
    push %r13
    push %r14
    push %r15
    /* Observe the active IST stack and the distinct interrupted stack. */
    mov %rsp, %rax
    lea irq_stack(%rip), %r10
    cmp %r10, %rax
    jb 1f
    lea irq_stack_top(%rip), %r10
    cmp %r10, %rax
    jae 1f
    mov 144(%rsp), %rax
    lea irq_stack(%rip), %r10
    cmp %r10, %rax
    jb 2f
    lea irq_stack_top(%rip), %r10
    cmp %r10, %rax
    jb 1f
2:  movl $1, irq_stack_ok(%rip)
1:  incl irq_count(%rip)
    mov 120(%rsp), %rax
    mov %rax, irq_rip(%rip)
    mov 136(%rsp), %rax
    mov %rax, irq_saved_flags(%rip)
    pushfq
    pop %rax
    mov %rax, irq_handler_flags(%rip)
    mov $0xff, %al
    outb %al, $0x21
    mov $0x20, %al
    outb %al, $0x20
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %r11
    pop %r10
    pop %r9
    pop %r8
    pop %rdi
    pop %rsi
    pop %rbp
    pop %rbx
    pop %rdx
    pop %rcx
    pop %rax
    iretq
print_string:
    lodsb
    test %al, %al
    jz 1f
    outb %al, $0xf1
    jmp print_string
1:  ret
print_hex:
    mov %rax, %rdx
    mov $16, %ecx
1:  rol $4, %rdx
    mov %edx, %eax
    and $15, %eax
    cmp $10, %al
    jb 2f
    add $87, %al
    jmp 3f
2:  add $48, %al
3:  outb %al, $0xf1
    loop 1b
    mov $10, %al
    outb %al, $0xf1
    ret
.data
.balign 16
gdt: .quad 0, 0x00af9a000000ffff, 0x00cf92000000ffff
    .quad 0x0000890000000067, 0
gdtr: .word 39
    .quad gdt
tss: .zero 102
    .word 104
idtr: .word 4095
    .quad idt
.section .rodata,"a"
raw_label: .asciz "KEEL IRQ HLT "
pair_label: .asciz "KEEL IRQ STIHLT "
expected:
    .fill 8,1,0xa5
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 56,1,0xa5
.bss
.balign 16
output: .skip 128
irq_count: .skip 8
irq_stack_ok: .skip 8
irq_rip: .skip 8
irq_saved_flags: .skip 8
irq_handler_flags: .skip 8
raw_resume: .skip 8
pair_resume: .skip 8
.balign 16
idt: .skip 4096
.balign 16
irq_stack: .skip 16384
irq_stack_top:
.section .note.GNU-stack,"",@progbits
