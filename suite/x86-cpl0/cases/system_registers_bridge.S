/* Trusted state and value oracle; no privileged execution on the host. */
.text
.code64
.global guest_main
guest_main:
    pushq $2
    popfq
    push %rbx
    sub $32, %rsp
    mov %cr0, %rax
    mov %rax, saved+0(%rip)
    mov %cr2, %rax
    mov %rax, saved+8(%rip)
    mov %cr3, %rax
    mov %rax, saved+16(%rip)
    mov %cr4, %rax
    mov %rax, saved+24(%rip)
    mov %cr8, %rax
    mov %rax, saved+32(%rip)
    mov %dr0, %rax
    mov %rax, saved+40(%rip)
    mov %dr1, %rax
    mov %rax, saved+48(%rip)
    mov %dr2, %rax
    mov %rax, saved+56(%rip)
    mov %dr3, %rax
    mov %rax, saved+64(%rip)
    mov %dr6, %rax
    mov %rax, saved+72(%rip)
    mov %dr7, %rax
    mov %rax, saved+80(%rip)
    mov $0x400, %eax
    mov %rax, %dr7
    mov $0x20, %eax
    mov %rax, %cr4
    mov %cr3, %rsi
    lea page_a(%rip), %rdi
    mov $512, %ecx
    cld
    rep movsq
    mov %cr3, %rsi
    lea page_b(%rip), %rdi
    mov $512, %ecx
    rep movsq

.macro check reg, before, after
    movabs $\before, %rax
    mov %rax, %\reg
    lea output(%rip), %rdi
    mov $128, %ecx
    mov $0xa5, %al
    rep stosb
    movabs $0x0000000100000002, %rax
    mov %rax, 0(%rsp)
    movabs $0xffffffff00000000, %rax
    mov %rax, 8(%rsp)
    movabs $0x7fffffffffffffff, %rax
    mov %rax, 16(%rsp)
    movabs $0x8000000000000000, %rax
    mov %rax, 24(%rsp)
    lea output(%rip), %rdi
    movabs $\after, %rsi
    movabs $0x0123456789abcdef, %rdx
    movabs $0xfedcba9876543210, %rcx
    movabs $0x8877665544332211, %r8
    movabs $0xdeadbeef11223344, %r9
    call system_\reg
    mov %\reg, %rax
    movabs $\after, %rcx
    cmp %rcx, %rax
    jne mismatch
    lea output(%rip), %rsi
    lea expected_\reg(%rip), %rdi
    mov $128, %ecx
    cld
    repe cmpsb
    jne mismatch
.endm
    check cr0, 0x80010033, 0x80050033
    check cr2, 0xfedcba9876543210, 0x0123456789abcdef
    check cr3, page_a, page_b
    check cr4, 0x20, 0xa0
    check cr8, 2, 9
    check dr0, 0xffff800012345670, 0x00001234abcde000
    check dr1, 0xffff800012345671, 0x00001234abcde001
    check dr2, 0xffff800012345672, 0x00001234abcde002
    check dr3, 0xffff800012345673, 0x00001234abcde003
    check dr4, 0xffff0ff0, 0xffff4ff1
    check dr5, 0x400, 0x10400
    check dr6, 0xffff0ff0, 0xffff4ff2
    check dr7, 0x400, 0x50400
    call system_constant
    movabs $0x000056789abcdef0, %rcx
    cmp %rcx, %rax
    jne mismatch
    mov %dr0, %rax
    cmp %rcx, %rax
    jne mismatch
    xor %ebx, %ebx
    jmp restore
mismatch:
    mov $1, %ebx
restore:
    mov $0x400, %eax
    mov %rax, %dr7
    mov saved+0(%rip), %rax
    mov %rax, %cr0
    mov saved+8(%rip), %rax
    mov %rax, %cr2
    mov saved+16(%rip), %rax
    mov %rax, %cr3
    mov saved+24(%rip), %rax
    mov %rax, %cr4
    mov saved+32(%rip), %rax
    mov %rax, %cr8
    mov saved+40(%rip), %rax
    mov %rax, %dr0
    mov saved+48(%rip), %rax
    mov %rax, %dr1
    mov saved+56(%rip), %rax
    mov %rax, %dr2
    mov saved+64(%rip), %rax
    mov %rax, %dr3
    mov saved+72(%rip), %rax
    mov %rax, %dr6
    mov saved+80(%rip), %rax
    mov %rax, %dr7
    mov %ebx, %eax
    add $32, %rsp
    pop %rbx
    ret
.section .rodata,"a"
expected_cr0:
    .fill 8, 1, 0xa5
    .quad 0x80010033, 0x80050033, 0x80050033
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_cr2:
    .fill 8, 1, 0xa5
    .quad 0xfedcba9876543210, 0x0123456789abcdef, 0x0123456789abcdef
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_cr3:
    .fill 8, 1, 0xa5
    .quad page_a, page_b, page_b
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_cr4:
    .fill 8, 1, 0xa5
    .quad 0x20, 0xa0, 0xa0
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_cr8:
    .fill 8, 1, 0xa5
    .quad 2, 9, 9
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_dr0:
    .fill 8, 1, 0xa5
    .quad 0xffff800012345670, 0x00001234abcde000, 0x00001234abcde000
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_dr1:
    .fill 8, 1, 0xa5
    .quad 0xffff800012345671, 0x00001234abcde001, 0x00001234abcde001
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_dr2:
    .fill 8, 1, 0xa5
    .quad 0xffff800012345672, 0x00001234abcde002, 0x00001234abcde002
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_dr3:
    .fill 8, 1, 0xa5
    .quad 0xffff800012345673, 0x00001234abcde003, 0x00001234abcde003
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_dr4:
    .fill 8, 1, 0xa5
    .quad 0xffff0ff0, 0xffff4ff1, 0xffff4ff1
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_dr5:
    .fill 8, 1, 0xa5
    .quad 0x400, 0x10400, 0x10400
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_dr6:
    .fill 8, 1, 0xa5
    .quad 0xffff0ff0, 0xffff4ff2, 0xffff4ff2
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
expected_dr7:
    .fill 8, 1, 0xa5
    .quad 0x400, 0x50400, 0x50400
    .quad 0x0123456789abcdef, 0xfedcba9876543210, 0x8877665544332211, 0xdeadbeef11223344, 0x0000000100000002, 0xffffffff00000000, 0x7fffffffffffffff, 0x8000000000000000
    .fill 32, 1, 0xa5
.bss
.balign 16
output: .skip 128
saved: .skip 88
.balign 4096
page_a: .skip 4096
page_b: .skip 4096
.section .note.GNU-stack,"",@progbits
