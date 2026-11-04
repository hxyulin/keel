	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "compare"
	.p2align	4
	.type	"compare",@function
"compare":
.Lcompare_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lcompare_block0:
	# v2 = icmp.slt v0, v1
	cmp	rsi, rdi
	setl	r8b
	# v4 = icmp.ult v0, v1
	cmp	rsi, rdi
	setb	sil
	# v6 = uext.i64 v2
	movzx	edi, r8b
	# v8 = uext.i64 v4
	movzx	esi, sil
	# v10 = iadd v6, v6
	add	rdi, rdi
	# v11 = iadd v8, v8
	add	rsi, rsi
	# v12 = iadd v11, v11
	add	rsi, rsi
	# v13 = iadd v10, v12
	add	rdi, rsi
	# return v13
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lcompare_end:
	.cfi_endproc
	.size "compare", .-"compare"
	.section	".note.GNU-stack","",@progbits
