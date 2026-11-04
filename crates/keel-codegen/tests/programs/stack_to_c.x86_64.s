	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "main"
	.p2align	4
	.type	"main",@function
"main":
.Lmain_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	push	r12
	.cfi_offset 12, -32
	push	r13
	.cfi_offset 13, -40
	sub	rsp, 56
.Lmain_block0:
	# v0 = stack_addr ss0
	lea	rbx, [rbp + -80]
	# v1 = iconst.i64 10
	mov	r12d, 10
	# call @fill_squares(v0, v1)
	mov	rdi, rbx
	mov	rsi, r12
	call	"fill_squares"@PLT
	# v2 = iconst.i64 0
	xor	esi, esi
	# jump block1(v2, v2)
	mov	r13, rsi
.Lmain_block1:
	# v5 = icmp.ult v3, v1
	cmp	rsi, 10
	setb	dil
	# brif v5, block2, block3
	test	dil, dil
	jne	".Lmain_block2"
.Lmain_block3:
	# v14 = stack_addr ss1
	lea	rsi, [rbp + -40]
	# v15 = iconst.i8 111
	# store v15, v14
	mov	byte ptr [rbp + -40], 111
	# v16 = iconst.i64 1
	# v17 = ptradd v14, v16
	lea	rdi, [rsi + 1]
	# v18 = iconst.i8 107
	# store v18, v17
	mov	byte ptr [rdi], 107
	# v19 = iconst.i64 2
	# v20 = ptradd v14, v19
	lea	rdi, [rsi + 2]
	# v21 = iconst.i8 0
	# store v21, v20
	mov	byte ptr [rdi], 0
	# v22 = call @puts(v14)
	mov	rdi, rsi
	call	"puts"@PLT
	mov	rsi, rax
	# v23 = iconst.i64 285
	# v24 = icmp.ne v4, v23
	cmp	r13, 285
	setne	sil
	# v25 = iconst.i32 1
	mov	edi, 1
	# v26 = iconst.i32 0
	xor	r8d, r8d
	# v27 = select v24, v25, v26
	test	sil, sil
	mov	rax, r8
	cmovne	rax, rdi
	mov	rsi, rax
	# return v27
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lmain_block2:
	# v6 = iconst.i64 4
	# v7 = imul v3, v6
	imul	rdi, rsi, 4
	# v8 = ptradd v0, v7
	add	rdi, rbx
	# v9 = load.i32 v8
	mov	edi, dword ptr [rdi]
	# v10 = sext.i64 v9
	movsxd	rdi, edi
	# v11 = iadd v4, v10
	add	rdi, r13
	# v12 = iconst.i64 1
	# v13 = iadd v3, v12
	add	rsi, 1
	# jump block1(v13, v11)
	mov	r13, rdi
	jmp	".Lmain_block1"
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
