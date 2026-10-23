.text
.globl probe
.type probe,@function
probe:
 pushq %rbx
 pushq %r12
 pushq %r13
 pushq %r14
 pushq %r15
 movq %rdi,%rbx
 movq %rsi,%r12
 movq %rdx,%r13
 movq %r8,%r14
 movq %r9,%r15
 movl $7,%eax
 xorl %edx,%edx
 xrstor64 (%rcx)
 xsave64 (%r14)
 movq %r12,%rdi
 movq %r13,%rsi
 call *%rbx
 movl $7,%eax
 xorl %edx,%edx
 xsave64 (%r15)
 fninit
 pushq $0x1f80
 ldmxcsr (%rsp)
 addq $8,%rsp
 vzeroupper
 popq %r15
 popq %r14
 popq %r13
 popq %r12
 popq %rbx
 ret
.size probe,.-probe
.globl oracle_save
.type oracle_save,@function
oracle_save:
 movq %rsi,%rax
 movq %rsi,%rdx
 shrq $32,%rdx
 xsave64 (%rdi)
 ret
.size oracle_save,.-oracle_save
.globl oracle_restore
.type oracle_restore,@function
oracle_restore:
 movq %rsi,%rax
 movq %rsi,%rdx
 shrq $32,%rdx
 xrstor64 (%rdi)
 ret
.size oracle_restore,.-oracle_restore
.section .note.GNU-stack,"",@progbits
