	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "checked_fold"
	.p2align	4
	.type	"checked_fold",@function
"checked_fold":
.Lchecked_fold_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lchecked_fold_block0:
	# v0 = iconst.i64 9223372036854775807
	movabs	rsi, 9223372036854775807
	# v1 = iconst.i64 1
	mov	edi, 1
	# v4, v5 = sadd_overflow v0, v1
	mov	rax, rsi
	add	rax, rdi
	seto	cl
	mov	rsi, rax
	mov	rdi, rcx
	# v6 = uext.i64 v5
	movzx	edi, dil
	# v7 = xor v4, v6
	xor	rsi, rdi
	# return v7
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lchecked_fold_end:
	.cfi_endproc
	.size "checked_fold", .-"checked_fold"
	.section	".note.GNU-stack","",@progbits
