.text
# run(fn,a,b,out,cw,after[3],top): empty tags, arbitrary physical TOP.
.globl run
.type run,@function
run:
 pushq %r12
 subq $528,%rsp
 movq %r9,%r12
 movq %rdi,%rax
 fnstcw 2(%rsp)
 fninit
 movw %r8w,0(%rsp)
 fldcw 0(%rsp)
 movq %rsi,%rdi
 movq %rdx,%rsi
 movq %rcx,%rdx
 # Seventh SysV argument: entry RSP+8, now RSP+544 after push/sub.
 movl 544(%rsp),%ecx
 testl %ecx,%ecx
 jz .Ltop_ready
.Lseed_top:
 fincstp
 decl %ecx
 jnz .Lseed_top
.Ltop_ready:
 call *%rax
 fnstsw 4(%rsp)
 fnstcw 6(%rsp)
 movl 4(%rsp),%ecx
 movl %ecx,(%r12)
 fxsave64 16(%rsp)
 movzbl 20(%rsp),%ecx
 movw %cx,4(%r12)
 fnclex
 fldcw 2(%rsp)
 addq $528,%rsp
 popq %r12
 ret
.size run,.-run

# Reference deliberately uses native assembler mnemonics, independently of Keel.
.macro oracle op,binary,keep,mode,store,status
.globl ref_\op\()_\mode
.type ref_\op\()_\mode,@function
ref_\op\()_\mode:
.if \binary
 fldt (%rsi)
.endif
 fldt (%rdi)
 \op
 fnstsw %ax
.if \store
 fstpt (%rdx)
.else
 fstp %st(0)
.endif
.if \keep
 fstp %st(0)
.endif
.if \status
 movzwl %ax,%eax
.else
 xorl %eax,%eax
.endif
 ret
.size ref_\op\()_\mode,.-ref_\op\()_\mode
.endm
.macro modes op,binary,keep
 oracle \op,\binary,\keep,both,1,1
 oracle \op,\binary,\keep,value,1,0
 oracle \op,\binary,\keep,status,0,1
 oracle \op,\binary,\keep,neither,0,0
.endm
modes fsin,0,0
modes fcos,0,0
modes f2xm1,0,0
modes fpatan,1,0
modes fyl2x,1,0
modes fyl2xp1,1,0
modes fscale,1,1
modes fprem,1,1
modes fprem1,1,1
.section .note.GNU-stack,"",@progbits
