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
	mov	rsi, rdi
	mov	rdi, rcx
.Lrk_adjacent_block0:
	# v4 = iconst.i64 0
	xor	r8d, r8d
	# v5 = iconst.i64 1
	mov	r9d, 1
	# v6 = icmp.ugt v3, v5
	# brif v6, block2(v5, v4), block3(v4)
	cmp	rdi, 1
	jbe	".Lrk_adjacent_block3"
.Lrk_adjacent_block2:
	# v9 = iconst.i64 2
	# v10 = shl v7, v9
	# v11 = ptradd v0, v10
	# v12 = load.i32 v11
	mov	ebx, dword ptr [rsi + r9 * 4]
	# v13 = sext.i64 v12
	movsxd	rbx, ebx
	# v36 = iconst.i64 1
	# v15 = isub v7, v36
	mov	r12, r9
	sub	r12, 1
	# v16 = shl v15, v9
	# v17 = ptradd v0, v16
	# v18 = load.i32 v17
	mov	r12d, dword ptr [rsi + r12 * 4]
	# v19 = sext.i64 v18
	movsxd	r12, r12d
	# v20 = isub v13, v19
	sub	rbx, r12
	# v37 = iconst.i64 0
	# v22 = icmp.slt v20, v37
	# v24 = ineg v20
	mov	r12, rbx
	neg	r12
	# v33 = select v22, v24, v20
	mov	rdx, rbx
	mov	r11, r12
	cmp	rbx, 0
	cmovl	rdx, r11
	mov	rbx, rdx
	# v26 = iadd v8, v33
	add	r8, rbx
	# v28 = iadd v7, v36
	add	r9, 1
	# v29 = icmp.ugt v3, v28
	# brif v29, block2(v28, v26), block3(v26)
	cmp	rdi, r9
	ja	".Lrk_adjacent_block2"
.Lrk_adjacent_block3:
	# return v23
	mov	rax, r8
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lrk_adjacent_end:
	.cfi_endproc
	.size "rk_adjacent", .-"rk_adjacent"
	.section	".note.GNU-stack","",@progbits
