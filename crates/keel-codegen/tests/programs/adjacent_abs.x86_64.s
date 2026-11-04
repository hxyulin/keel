	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "rk_adjacent"
	.p2align	4
	.type	"rk_adjacent",@function
"rk_adjacent":
.Lrk_adjacent_begin:
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
	sub	rsp, 8
	mov	rsi, rdi
	mov	rdi, rcx
.Lrk_adjacent_block0:
	# v4 = iconst.i64 0
	xor	r8d, r8d
	# v5 = iconst.i64 1
	mov	r9d, 1
	# jump block1
.Lrk_adjacent_block1:
	# v6 = icmp.ugt v3, v5
	cmp	rdi, 1
	seta	bl
	# brif v6, block2(v5, v4), block3(v4)
	test	bl, bl
	je	".Lrk_adjacent_inst4_else"
	mov	r11, r8
	mov	r8, r9
	mov	r9, r11
	jmp	".Lrk_adjacent_block2"
.Lrk_adjacent_inst4_else:
	mov	rsi, r8
	jmp	".Lrk_adjacent_block3"
.Lrk_adjacent_block2:
	# v9 = iconst.i64 2
	# v10 = shl v7, v9
	mov	r12, r8
	shl	r12, 2
	# v11 = ptradd v0, v10
	add	r12, rsi
	# v12 = load.i32 v11
	mov	r12d, dword ptr [r12]
	# v13 = sext.i64 v12
	movsxd	r12, r12d
	# v14 = iconst.i64 1
	# v15 = isub v7, v14
	mov	r13, r8
	sub	r13, 1
	# v16 = shl v15, v9
	mov	rbx, r13
	shl	rbx, 2
	# v17 = ptradd v0, v16
	add	rbx, rsi
	# v18 = load.i32 v17
	mov	ebx, dword ptr [rbx]
	# v19 = sext.i64 v18
	movsxd	rbx, ebx
	# v20 = isub v13, v19
	mov	rax, r12
	sub	rax, rbx
	mov	rbx, rax
	# v21 = iconst.i64 0
	# v22 = icmp.slt v20, v21
	cmp	rbx, 0
	setl	r12b
	# brif v22, block4, block5(v20)
	test	r12b, r12b
	je	".Lrk_adjacent_block5"
.Lrk_adjacent_block4:
	# v24 = ineg v20
	neg	rbx
	# jump block5(v24)
.Lrk_adjacent_block5:
	# v26 = iadd v8, v25
	add	r9, rbx
	# v27 = iconst.i64 1
	# v28 = iadd v7, v27
	add	r8, 1
	# v29 = icmp.ugt v3, v28
	cmp	rdi, r8
	seta	bl
	# brif v29, block2(v28, v26), block3(v26)
	test	bl, bl
	jne	".Lrk_adjacent_block2"
	mov	rsi, r9
.Lrk_adjacent_block3:
	# return v23
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lrk_adjacent_end:
	.cfi_endproc
	.size "rk_adjacent", .-"rk_adjacent"
	.section	".note.GNU-stack","",@progbits
