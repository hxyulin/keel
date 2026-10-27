.text
.globl stack_probe
.type stack_probe,@function
stack_probe:
    movq %rsp,stack_entry(%rip)
    pushq %rbp
    movq %rsp,%rbp
    subq $64,%rsp
    movabsq $0x1122334455667788,%rax
    movq %rax,(%rsp)
    movq %rax,56(%rsp)
    call stack_nested
    movabsq $0x1122334455667788,%rax
    cmpq %rax,(%rsp)
    jne .Lbad_stack
    cmpq %rax,56(%rsp)
    jne .Lbad_stack
    jmp .Lstack_done
.Lbad_stack:
    movq $1,stack_probe_failed(%rip)
.Lstack_done:
    leave
    ret
.size stack_probe,.-stack_probe

.type stack_nested,@function
stack_nested:
    movq %rsp,stack_nested_entry(%rip)
    pxor %xmm0,%xmm0
    pxor %xmm1,%xmm1
    pxor %xmm2,%xmm2
    pxor %xmm3,%xmm3
    pxor %xmm4,%xmm4
    pxor %xmm5,%xmm5
    pxor %xmm6,%xmm6
    pxor %xmm7,%xmm7
    pxor %xmm8,%xmm8
    pxor %xmm9,%xmm9
    pxor %xmm10,%xmm10
    pxor %xmm11,%xmm11
    pxor %xmm12,%xmm12
    pxor %xmm13,%xmm13
    pxor %xmm14,%xmm14
    pxor %xmm15,%xmm15
    movabsq $0xfedcba9876543210,%rax
    movq %rax,%rcx
    movq %rax,%rdx
    movq %rax,%rsi
    movq %rax,%rdi
    movq %rax,%r8
    movq %rax,%r9
    movq %rax,%r10
    movq %rax,%r11
    ret
.size stack_nested,.-stack_nested

.globl stack_eight
.type stack_eight,@function
stack_eight:
    movq %rsp,stack_entry(%rip)
    leaq (%rdi,%rsi),%rax
    addq %rdx,%rax
    addq %rcx,%rax
    addq %r8,%rax
    addq %r9,%rax
    addq 8(%rsp),%rax
    addq 16(%rsp),%rax
    ret
.size stack_eight,.-stack_eight

.globl clear_page_probe
.type clear_page_probe,@function
clear_page_probe:
    movl $512,%ecx
    xorl %eax,%eax
    rep stosq
    ret
.size clear_page_probe,.-clear_page_probe

.globl clear_page_probe_alternative
.type clear_page_probe_alternative,@function
clear_page_probe_alternative:
    jmp clear_page_probe
.size clear_page_probe_alternative,.-clear_page_probe_alternative
.section .note.GNU-stack,"",@progbits
