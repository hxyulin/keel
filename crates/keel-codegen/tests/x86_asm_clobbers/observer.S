.text
.globl check_saved
.type check_saved,@function
check_saved:
    push %rbp
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    sub $8, %rsp
    mov %rdi, %rax
    mov %rsi, %rdi
    mov %rdx, %rsi
    mov $0x1234, %ebx
    mov $0x2345, %r12d
    mov $0x3456, %r13d
    mov $0x4567, %r14d
    mov $0x5678, %r15d
    call *%rax
    xor %eax, %eax
    cmp $0x1234, %rbx
    jne 1f
    cmp $0x2345, %r12
    jne 1f
    cmp $0x3456, %r13
    jne 1f
    cmp $0x4567, %r14
    jne 1f
    cmp $0x5678, %r15
    jne 1f
    mov $1, %eax
1:
    add $8, %rsp
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    pop %rbp
    ret
.size check_saved, .-check_saved
.section .note.GNU-stack,"",@progbits
