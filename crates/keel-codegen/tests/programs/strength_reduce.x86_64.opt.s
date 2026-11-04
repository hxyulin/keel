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
	# v11 = iconst.i64 -8
	# v8 = and v0, v11
	mov	rdi, rsi
	and	rdi, -8
	# v3 = isub v0, v8
	sub	rsi, rdi
	# v5 = iadd v3, v8
	add	rsi, rdi
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lstrength_end:
	.cfi_endproc
	.size "strength", .-"strength"
	.section	".note.GNU-stack","",@progbits
