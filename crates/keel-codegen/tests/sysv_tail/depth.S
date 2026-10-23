.text
.globl alias_entry
alias_entry:
 sub $40,%rsp
 mov %rdi,24(%rsp)
 movq $0,0(%rsp)
 movq $0,8(%rsp)
 movq $0,16(%rsp)
 call sret_alias
 cmp 24(%rsp),%rax
 sete %al
 movzbl %al,%eax
 add $40,%rsp
 ret
.globl depth_entry
depth_entry:
 mov %rsp, depth_sp(%rip)
 jmp tail_depth
.globl depth_target
depth_target:
 xor %eax,%eax
 cmp depth_sp(%rip),%rsp
 sete %al
 ret
.bss
.p2align 3
depth_sp: .zero 8
.section .note.GNU-stack,"",@progbits
