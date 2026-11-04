	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "strength"
	.p2align	4
	.type	"strength",@function
"strength":
.Lstrength_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lstrength_block0:
	# v1 = iconst.i64 8
	# v2 = udiv v0, v1
	mov	rax, rsi
	mov	r11, rax
	shr	rax, 3
	mov	r8, rax
	# v3 = urem v0, v1
	mov	rax, rsi
	and	rax, 7
	mov	rsi, rax
	# v4 = imul v2, v1
	imul	rdi, r8, 8
	# v5 = iadd v4, v3
	add	rsi, rdi
	# v6 = iconst.i64 0
	# v7 = iadd v5, v6
	add	rsi, 0
	# return v7
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lstrength_end:
	.cfi_endproc
	.size "strength", .-"strength"
	.section	".note.GNU-stack","",@progbits
