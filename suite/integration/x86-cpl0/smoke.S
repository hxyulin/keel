/* One finite CPL0 observation, with BSS and call alignment controls. */
.section .text,"ax"
.code64
.global guest_main
guest_main:
    mov %cs, %ax
    test $3, %al
    jnz 1f
    smsw %eax
    test $1, %al
    jz 1f
    cmpq $0, zero(%rip)
    jne 1f
    mov %rsp, %rax
    and $15, %eax
    cmp $8, %eax
    jne 1f
    xor %eax, %eax
    ret
1:  mov $1, %eax
    ret
.section .bss,"aw",@nobits
.balign 8
zero: .skip 8
.section .note.GNU-stack,"",@progbits
