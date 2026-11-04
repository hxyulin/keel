	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "folding_duplicate"
	.p2align	4
	.type	"folding_duplicate",@function
"folding_duplicate":
.Lfolding_duplicate_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lfolding_duplicate_block0:
	# v0 = iconst.i64 7
	mov	esi, 7
	# v1 = iadd v0, v0
	add	rsi, 7
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lfolding_duplicate_end:
	.cfi_endproc
	.size "folding_duplicate", .-"folding_duplicate"
	.section	".note.GNU-stack","",@progbits
