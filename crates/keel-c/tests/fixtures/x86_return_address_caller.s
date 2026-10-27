.text
.globl observe_return
.type observe_return,@function
observe_return:
    pushq %rbp
    movq %rsp,%rbp
    call *%rdi
.Lactual_continuation:
    leaq .Lactual_continuation(%rip),%rdx
    cmpq %rdx,%rax
    setne %al
    movzbl %al,%eax
    popq %rbp
    ret
.size observe_return,.-observe_return

.globl clobber_callers
.type clobber_callers,@function
clobber_callers:
    movabsq $0x1122334455667788,%rax
    movabsq $0x8877665544332211,%rcx
    movabsq $0x123456789abcdef0,%rdx
    movq %rax,%r8
    movq %rcx,%r9
    movq %rdx,%r10
    movq %rax,%r11
    ret
.size clobber_callers,.-clobber_callers
.section .note.GNU-stack,"",@progbits

.text
.globl observe_return_chain
.type observe_return_chain,@function
observe_return_chain:
    pushq %rbp
    movq %rsp,%rbp
    call .Lchain_parent
.Lchain_root_continuation:
    popq %rbp
    ret
.Lchain_parent:
    pushq %rbp
    movq %rsp,%rbp
    call .Lchain_leaf
.Lchain_parent_continuation:
    popq %rbp
    ret
.Lchain_leaf:
    pushq %rbp
    movq %rsp,%rbp
    call read_parent_return
    leaq .Lchain_parent_continuation(%rip),%rdx
    cmpq %rdx,%rax
    jne .Lchain_fail
    call read_grandparent_return
    leaq .Lchain_root_continuation(%rip),%rdx
    cmpq %rdx,%rax
    jne .Lchain_fail
    xorl %eax,%eax
    jmp .Lchain_done
.Lchain_fail:
    movl $1,%eax
.Lchain_done:
    popq %rbp
    ret
.size observe_return_chain,.-observe_return_chain
