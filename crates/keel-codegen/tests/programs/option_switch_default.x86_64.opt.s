	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "option_code"
	.p2align	4
	.type	"option_code",@function
"option_code":
.Loption_code_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Loption_code_block0:
	# switch v0, block2, [1: block1(v1)]
	cmp	rsi, 1
	je	".Loption_code_block1"
.Loption_code_block2:
	# brif v0, block3, block4
	test	rsi, rsi
	jne	".Loption_code_block3"
.Loption_code_block4:
	# v3 = iconst.i64 -1
	mov	rsi, -1
	# jump block1(v3)
	mov	rdi, rsi
.Loption_code_block1:
	# return v2
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Loption_code_block3:
	# trap
	ud2
.Loption_code_end:
	.cfi_endproc
	.size "option_code", .-"option_code"
	.section	".note.GNU-stack","",@progbits
