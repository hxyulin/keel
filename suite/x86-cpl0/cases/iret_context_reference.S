/* Experimental GPR/IRETQ image witness; not a stable compiler API. */
.text
.code64
.global guest_main
guest_main:
    push %rbx
    push %rbp
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rsp,saved_rsp(%rip)
    lea image(%rip),%rdi
    call transfer
    mov $1,%eax
    jmp cleanup
landing:
    mov %rax, captured+0(%rip)
    mov %rcx, captured+8(%rip)
    mov %rdx, captured+16(%rip)
    mov %rbx, captured+24(%rip)
    mov %rbp, captured+32(%rip)
    mov %rsi, captured+40(%rip)
    mov %rdi, captured+48(%rip)
    mov %r8, captured+56(%rip)
    mov %r9, captured+64(%rip)
    mov %r10, captured+72(%rip)
    mov %r11, captured+80(%rip)
    mov %r12, captured+88(%rip)
    mov %r13, captured+96(%rip)
    mov %r14, captured+104(%rip)
    mov %r15, captured+112(%rip)
    mov %rsp,captured_sp(%rip)
    pushfq
    pop %rax
    mov %rax,captured_flags(%rip)
    lea captured(%rip),%rsi
    lea expected(%rip),%rdi
    mov $120,%ecx
    cld
    repe cmpsb
    jne bad
    lea target_stack_top(%rip),%rax
    cmp captured_sp(%rip),%rax
    jne bad
    cmpq $2,captured_flags(%rip)
    jne bad
    xor %eax,%eax
    jmp cleanup
bad:
    mov $1,%eax
cleanup:
    mov saved_rsp(%rip),%rsp
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbp
    pop %rbx
    ret
/* 15 GPR words followed by RIP, CS, RFLAGS, RSP and SS.
 * R10 keeps the image address until the final load. No ordinary epilogue. */
transfer:
    mov %rdi,%r10
    lea 120(%r10),%rsp
    mov 0(%r10),%rax
    mov 8(%r10),%rcx
    mov 16(%r10),%rdx
    mov 24(%r10),%rbx
    mov 32(%r10),%rbp
    mov 40(%r10),%rsi
    mov 48(%r10),%rdi
    mov 56(%r10),%r8
    mov 64(%r10),%r9
    mov 80(%r10),%r11
    mov 88(%r10),%r12
    mov 96(%r10),%r13
    mov 104(%r10),%r14
    mov 112(%r10),%r15
.ifndef KEEL_OMIT_R10
    mov 72(%r10),%r10
.endif
.ifdef KEEL_USE_RET
    ret
.else
    iretq
.endif
.data
.balign 8
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
.ifdef KEEL_WRONG_FLAGS
    .quad landing,8,3,target_stack_top,16
.else
    .quad landing,8,2,target_stack_top,16
.endif
.section .rodata,"a"
expected:
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
.bss
.balign 16
captured: .skip 120
captured_sp: .skip 8
captured_flags: .skip 8
saved_rsp: .skip 8
.balign 16
target_stack: .skip 16384
target_stack_top:
.section .note.GNU-stack,"",@progbits
