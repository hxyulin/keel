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
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lcompare_block0:
	# v2 = icmp.slt v0, v1
	cmp	rsi, rdi
	setl	r8b
	# v3 = icmp.sgt v1, v0
	cmp	rdi, rsi
	setg	r9b
	# v4 = icmp.ult v0, v1
	cmp	rsi, rdi
	setb	bl
	# v5 = icmp.ugt v1, v0
	cmp	rdi, rsi
	seta	sil
	# v6 = uext.i64 v2
	movzx	edi, r8b
	# v7 = uext.i64 v3
	movzx	r8d, r9b
	# v8 = uext.i64 v4
	movzx	r9d, bl
	# v9 = uext.i64 v5
	movzx	esi, sil
	# v10 = iadd v6, v7
	add	rdi, r8
	# v11 = iadd v8, v9
	add	rsi, r9
	# v12 = iadd v11, v11
	add	rsi, rsi
	# v13 = iadd v10, v12
	add	rsi, rdi
	# return v13
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lcompare_end:
	.cfi_endproc
	.size "compare", .-"compare"
	.section	".note.GNU-stack","",@progbits
