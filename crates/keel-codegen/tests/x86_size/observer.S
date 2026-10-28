.text
.globl side
.type side,@function
side:
    mov %rsp, %rax
    and $15, %eax
    cmp $8, %eax
    jne .Lfail
    mov %rdi, %rax
    and $1, %eax
    ret
.size side,.-side
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
    mov $101, %ebp
    mov $102, %ebx
    mov $103, %r12d
    mov $104, %r13d
    mov $105, %r14d
    mov $106, %r15d
    call choose
    cmp $101, %rbp
    jne .Lfail
    cmp $102, %rbx
    jne .Lfail
    cmp $103, %r12
    jne .Lfail
    cmp $104, %r13
    jne .Lfail
    cmp $105, %r14
    jne .Lfail
    cmp $106, %r15
    jne .Lfail
    add $8, %rsp
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    pop %rbp
    ret
.Lfail:
    ud2
.size check_saved,.-check_saved
.section .note.GNU-stack,"",@progbits
