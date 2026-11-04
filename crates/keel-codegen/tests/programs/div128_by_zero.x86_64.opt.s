	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "main"
	.p2align	4
	.type	"main",@function
"main":
.Lmain_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lmain_block0:
	# v0 = iconst.i128 7
	mov	eax, 7
	xor	edx, edx
	mov	rsi, rax
	mov	rdi, rdx
	# v1 = iconst.i128 0
	xor	eax, eax
	xor	edx, edx
	mov	r8, rax
	mov	r9, rdx
	# v2 = udiv v0, v1
	mov	rax, r8
	or	rax, r9
	je	".Lmain_trap"
	mov	rdx, r8
	mov	rcx, r9
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"__udivti3"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# v3 = trunc.i32 v2
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmain_trap:
	ud2
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
