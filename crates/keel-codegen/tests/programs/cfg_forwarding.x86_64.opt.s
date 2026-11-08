	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "forward"
	.p2align	4
	.type	"forward",@function
"forward":
.Lforward_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lforward_block0:
	# v11 = select v2, v1, v0
	test	r8b, r8b
	mov	rax, rsi
	cmovne	rax, rdi
	mov	rdi, rax
	# v9 = imul v11, v11
	imul	rdi, rdi
	# v10 = iadd v0, v9
	add	rsi, rdi
	# return v10
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lforward_end:
	.cfi_endproc
	.size "forward", .-"forward"
	.section	".note.GNU-stack","",@progbits
