/* Debugcon F1 avoids pc-testdev's E8--EB ports. No host I/O executes here. */
.section .text,"ax"
.code64
.global harness_main
harness_main:
    mov %cs, %ax
    test $3, %al
    jnz fail
    mov $0xc0000080, %ecx
    rdmsr
    test $0x400, %eax
    jz fail
    lea ready(%rip), %rsi
    call puts
    /* Entry RSP is 8 mod 16; align before calling the external payload. */
    sub $8, %rsp
    call guest_main
    add $8, %rsp
    test %eax, %eax
    jnz fail
    lea passed(%rip), %rsi
    call puts
    mov $0x10, %eax
    jmp exit
fail:
    lea failed(%rip), %rsi
    call puts
    mov $0x11, %eax
exit:
    outl %eax, $0xf4
1:  cli
    hlt
    jmp 1b
puts:
    cld
2:  lodsb
    test %al, %al
    jz 3f
    outb %al, $0xf1
    jmp 2b
3:  ret
.section .rodata,"a"
ready: .asciz "KEEL CPL0 LONG64\n"
passed: .asciz "KEEL PASS\n"
failed: .asciz "KEEL FAIL\n"
.section .note.GNU-stack,"",@progbits
