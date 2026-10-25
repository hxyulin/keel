/* Private long-mode GDT/TSS/IDT. CPL3 is observable, not an isolation test. */
.text
.code64
.global guest_main
guest_main:
    cli
    cld
    push %rbx
    push %rbp
    push %r12
    push %r13
    push %r14
    push %r15
    sub $40,%rsp
    mov %rsp,saved_rsp(%rip)
    /* Temporarily allow user access to the first 2 MiB and a high alias. */
    mov %cr3,%rax
    mov %rax,original_cr3(%rip)
    mov (%rax),%rdx
    mov %rdx,original_pml4(%rip)
    mov 2048(%rax),%r8
    mov %r8,original_alias(%rip)
    mov %rdx,%r8
    or $4,%r8
    mov %r8,2048(%rax)
    orq $4,(%rax)
    and $-4096,%rdx
    mov %rdx,pdpt_address(%rip)
    mov (%rdx),%rcx
    mov %rcx,original_pdpt(%rip)
    orq $4,(%rdx)
    and $-4096,%rcx
    mov %rcx,pd_address(%rip)
    mov (%rcx),%r8
    mov %r8,original_pd(%rip)
    orq $4,(%rcx)
    mov %rax,%cr3
    /* Keep the boot selector meanings and add DPL3 code/data plus a TSS. */
    lea tss(%rip),%rax
    mov %ax,gdt+42(%rip)
    shr $16,%rax
    mov %al,gdt+44(%rip)
    mov %ah,gdt+47(%rip)
    shr $16,%rax
    mov %eax,gdt+48(%rip)
    lea kernel_stack_top(%rip),%rax
    mov %rax,tss+4(%rip)
    lgdt gdtr(%rip)
    mov $40,%eax
    ltr %ax
    lea return_gate(%rip),%rax
    mov %ax,idt+2048(%rip)
    movw $8,idt+2050(%rip)
    movb $0,idt+2052(%rip)
    movb $0xee,idt+2053(%rip)
    shr $16,%rax
    mov %ax,idt+2054(%rip)
    shr $16,%rax
    mov %eax,idt+2056(%rip)
    lidt idtr(%rip)
    movq $0,phase(%rip)
prepare:
    lea output(%rip),%rdi
    mov $128,%ecx
    mov $0xa5,%al
    cld
    rep stosb
    movabs $0x0000000100000002,%rax
    mov %rax,0(%rsp)
    movabs $0xffffffff00000000,%rax
    mov %rax,8(%rsp)
    movabs $0x7fffffffffffffff,%rax
    mov %rax,16(%rsp)
    movabs $0x8000000000000000,%rax
    mov %rax,24(%rsp)
    lea image(%rip),%rdi
    lea output(%rip),%rsi
    movabs $0x0123456789abcdef,%rdx
    movabs $0xfedcba9876543210,%rcx
    movabs $0x8877665544332211,%r8
    movabs $0xdeadbeef11223344,%r9
    call resume_owned
    /* Returning through the compiled caller can never count as a transfer. */
    jmp mismatch
landing:
    mov %rax,captured+0(%rip)
    mov %rcx,captured+8(%rip)
    mov %rdx,captured+16(%rip)
    mov %rbx,captured+24(%rip)
    mov %rbp,captured+32(%rip)
    mov %rsi,captured+40(%rip)
    mov %rdi,captured+48(%rip)
    mov %r8,captured+56(%rip)
    mov %r9,captured+64(%rip)
    mov %r10,captured+72(%rip)
    mov %r11,captured+80(%rip)
    mov %r12,captured+88(%rip)
    mov %r13,captured+96(%rip)
    mov %r14,captured+104(%rip)
    mov %r15,captured+112(%rip)
    mov %rsp,captured_sp(%rip)
    pushfq
    pop %rax
    mov %rax,captured_flags(%rip)
    xor %eax,%eax
    mov %cs,%ax
    mov %rax,captured_cs(%rip)
    mov %ss,%ax
    mov %rax,captured_ss(%rip)
    lea landing(%rip),%rax
    mov %rax,captured_rip(%rip)
    mov %cs,%ax
    test $3,%al
    jnz user_return
    /* A broken return may land at CPL0 through the high alias. Verify and
     * remove that alias only while executing the trusted low mapping. */
    movabs $verify,%rax
    jmp *%rax
user_return:
    /* This DPL3 gate is the only permitted return to the trusted verifier. */
    int $0x80
user_fallthrough:
    ud2
return_gate:
    /* Independently check the privilege/stack frame pushed by the processor. */
    lea kernel_stack_top-40(%rip),%rax
    cmp %rax,%rsp
    jne mismatch
    lea user_fallthrough(%rip),%rax
    movabs $0xffff800000000000,%rdx
    or %rdx,%rax
    cmp %rax,0(%rsp)
    jne mismatch
    cmpq $27,8(%rsp)
    jne mismatch
    mov captured_sp(%rip),%rax
    cmp %rax,24(%rsp)
    jne mismatch
    cmpq $35,32(%rsp)
    jne mismatch
    /* Current flags differ after comparisons; captured flags predate them. */
verify:
    cli
    lea captured(%rip),%rsi
    lea expected_gprs(%rip),%rdi
    mov $120,%ecx
    cld
    repe cmpsb
    jne mismatch
    lea landing(%rip),%rax
    cmpq $0,phase(%rip)
    je expected_rip
    movabs $0xffff800000000000,%rdx
    or %rdx,%rax
expected_rip:
    cmp captured_rip(%rip),%rax
    jne mismatch
    cmp image+120(%rip),%rax
    jne mismatch
    cmpq $0,phase(%rip)
    jne verify_user
    cmpq $2,captured_flags(%rip)
    jne mismatch
    cmpq $2,image+136(%rip)
    jne mismatch
    cmpq $8,image+128(%rip)
    jne mismatch
    cmpq $16,image+152(%rip)
    jne mismatch
    cmpq $8,captured_cs(%rip)
    jne mismatch
    cmpq $16,captured_ss(%rip)
    jne mismatch
    lea target_stack0_top(%rip),%rax
    jmp verify_stack
verify_user:
    cmpq $0x43,captured_flags(%rip)
    jne mismatch
    cmpq $0x43,image+136(%rip)
    jne mismatch
    cmpq $27,image+128(%rip)
    jne mismatch
    cmpq $35,image+152(%rip)
    jne mismatch
    cmpq $27,captured_cs(%rip)
    jne mismatch
    cmpq $35,captured_ss(%rip)
    jne mismatch
    lea target_stack1_top(%rip),%rax
    movabs $0xffff800000000000,%rdx
    or %rdx,%rax
verify_stack:
    cmp captured_sp(%rip),%rax
    jne mismatch
    cmp image+144(%rip),%rax
    jne mismatch
    lea output(%rip),%rsi
    lea expected_output(%rip),%rdi
    mov $128,%ecx
    repe cmpsb
    jne mismatch
    movabs $0xa5a5a5a5a5a5a5a5,%rax
    cmp image_before(%rip),%rax
    jne mismatch
    cmp image_after(%rip),%rax
    jne mismatch
    /* The adapter must not mutate any target GPR/frame word. */
    lea image(%rip),%rsi
    lea expected_gprs(%rip),%rdi
    mov $120,%ecx
    repe cmpsb
    jne mismatch
    cmpq $0,phase(%rip)
    jne success
    movq $1,phase(%rip)
    movq $27,image+128(%rip)
    movq $35,image+152(%rip)
    movq $0x43,image+136(%rip)
    lea landing(%rip),%rax
    movabs $0xffff800000000000,%rdx
    or %rdx,%rax
    mov %rax,image+120(%rip)
    lea target_stack1_top(%rip),%rax
    or %rdx,%rax
    mov %rax,image+144(%rip)
    mov saved_rsp(%rip),%rsp
    jmp prepare
success:
    xor %ebx,%ebx
    jmp cleanup
mismatch:
    mov $1,%ebx
cleanup:
    cli
    mov original_cr3(%rip),%rax
    mov original_pml4(%rip),%rdx
    mov %rdx,(%rax)
    mov original_alias(%rip),%rdx
    mov %rdx,2048(%rax)
    mov pdpt_address(%rip),%rcx
    mov original_pdpt(%rip),%rdx
    mov %rdx,(%rcx)
    mov pd_address(%rip),%rcx
    mov original_pd(%rip),%rdx
    mov %rdx,(%rcx)
    mov %rax,%cr3
    mov %ebx,%eax
    mov saved_rsp(%rip),%rsp
    add $40,%rsp
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbp
    pop %rbx
    ret
.data
.balign 16
gdt:
    .quad 0,0x00af9a000000ffff,0x00cf92000000ffff
    .quad 0x00affa000000ffff,0x00cff2000000ffff
    .quad 0x0000890000000067,0
gdtr: .word 55
    .quad gdt
tss: .zero 102
    .word 104
idtr: .word 4095
    .quad idt
.balign 8
image_before: .quad 0xa5a5a5a5a5a5a5a5
image:
    .quad 0x1234000000000100
    .quad 0x1234000101010201
    .quad 0x1234000202020302
    .quad 0x1234000303030403
    .quad 0x1234000404040504
    .quad 0x1234000505050605
    .quad 0x1234000606060706
    .quad 0x1234000707070807
    .quad 0x1234000808080908
    .quad 0x1234000909090a09
    .quad 0x1234000a0a0a0b0a
    .quad 0x1234000b0b0b0c0b
    .quad 0x1234000c0c0c0d0c
    .quad 0x1234000d0d0d0e0d
    .quad 0x1234000e0e0e0f0e
    .quad landing,8,2,target_stack0_top,16
image_after: .quad 0xa5a5a5a5a5a5a5a5
.section .rodata,"a"
expected_gprs:
    .quad 0x1234000000000100
    .quad 0x1234000101010201
    .quad 0x1234000202020302
    .quad 0x1234000303030403
    .quad 0x1234000404040504
    .quad 0x1234000505050605
    .quad 0x1234000606060706
    .quad 0x1234000707070807
    .quad 0x1234000808080908
    .quad 0x1234000909090a09
    .quad 0x1234000a0a0a0b0a
    .quad 0x1234000b0b0b0c0b
    .quad 0x1234000c0c0c0d0c
    .quad 0x1234000d0d0d0e0d
    .quad 0x1234000e0e0e0f0e
expected_output:
    .fill 8,1,0xa5
    .quad 0x0123456789abcdef,0xfedcba9876543210,0x8877665544332211,0xdeadbeef11223344
    .quad 0x0000000100000002,0xffffffff00000000,0x7fffffffffffffff,0x8000000000000000
    .fill 56,1,0xa5
.bss
.balign 16
output: .skip 128
captured: .skip 120
captured_sp: .skip 8
captured_flags: .skip 8
captured_cs: .skip 8
captured_ss: .skip 8
captured_rip: .skip 8
saved_rsp: .skip 8
phase: .skip 8
original_cr3: .skip 8
original_pml4: .skip 8
original_alias: .skip 8
original_pdpt: .skip 8
original_pd: .skip 8
pdpt_address: .skip 8
pd_address: .skip 8
.balign 16
idt: .skip 4096
kernel_stack: .skip 16384
kernel_stack_top:
target_stack0: .skip 16384
target_stack0_top:
target_stack1: .skip 16384
target_stack1_top:
.section .note.GNU-stack,"",@progbits
