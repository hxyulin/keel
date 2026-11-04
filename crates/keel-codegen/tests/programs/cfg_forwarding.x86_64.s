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
	# brif v2, block1(v0, v1), block2(v0, v1)
	test	r8b, r8b
	jne	".Lforward_block1"
.Lforward_block2:
	# jump block3(v5, v5)
	mov	rdi, rsi
	jmp	".Lforward_block3"
.Lforward_block1:
	# jump block3(v4, v3)
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lforward_block3:
	# v9 = imul v7, v7
	imul	rsi, rsi
	# v10 = iadd v9, v8
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
