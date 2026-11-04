	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "constant_loop"
	.p2align	4
	.type	"constant_loop",@function
"constant_loop":
.Lconstant_loop_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lconstant_loop_block0:
	# v1 = iconst.i64 0
	xor	edi, edi
	# jump block1(v1, v1)
	mov	r8, rdi
.Lconstant_loop_block1:
	# v6 = icmp.ult v3, v0
	# brif v6, block2, block3
	cmp	rdi, rsi
	jb	".Lconstant_loop_block2"
.Lconstant_loop_block3:
	# return v4
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Lconstant_loop_block2:
	# v7 = iconst.i64 49
	# v8 = iadd v4, v7
	add	r8, 49
	# v9 = iconst.i64 1
	# v10 = iadd v3, v9
	add	rdi, 1
	# jump block1(v10, v8)
	jmp	".Lconstant_loop_block1"
.Lconstant_loop_end:
	.cfi_endproc
	.size "constant_loop", .-"constant_loop"
	.section	".note.GNU-stack","",@progbits
