	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "sum"
	.p2align	4
	.type	"sum",@function
"sum":
.Lsum_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsum_block0:
	# v2 = iconst.i64 0
	xor	r8d, r8d
	# v39 = iconst.i64 0
	xor	r9d, r9d
	# v19 = isub v1, v2
	mov	rbx, rdi
	sub	rbx, 0
	# v20 = iconst.i64 2
	# v21 = icmp.uge v19, v20
	# brif v21, block5(v2, v2, v39), block6(v2, v2, v39)
	cmp	rbx, 2
	jb	".Lsum_inst18_else"
	mov	rbx, r9
	mov	r9, r8
	jmp	".Lsum_block5"
.Lsum_inst18_else:
	mov	rbx, r9
	mov	r9, r8
	jmp	".Lsum_block6"
.Lsum_block5:
	# v22 = iconst.i64 2
	# v23 = shl v45, v22
	# v24 = ptradd v0, v23
	# v25 = load.i32 v24
	mov	r12d, dword ptr [rsi + r8 * 4]
	# v26 = sext.i64 v25
	movsxd	r12, r12d
	# v27 = iadd v46, v26
	add	r9, r12
	# v28 = iconst.i64 1
	# v29 = iadd v45, v28
	add	r8, 1
	# v30 = iconst.i64 2
	# v31 = shl v29, v30
	# v32 = ptradd v0, v31
	# v33 = load.i32 v32
	mov	r12d, dword ptr [rsi + r8 * 4]
	# v34 = sext.i64 v33
	movsxd	r12, r12d
	# v35 = iadd v47, v34
	add	rbx, r12
	# v36 = iconst.i64 1
	# v37 = iadd v29, v36
	add	r8, 1
	# v51 = isub v1, v37
	mov	r12, rdi
	sub	r12, r8
	# v52 = iconst.i64 2
	# v53 = icmp.uge v51, v52
	# brif v53, block5(v37, v27, v35), block6(v37, v27, v35)
	cmp	r12, 2
	jae	".Lsum_block5"
.Lsum_block6:
	# v38 = iadd v49, v50
	add	r9, rbx
	# v5 = icmp.ugt v1, v48
	# brif v5, block2(v48, v38), block3(v38)
	cmp	rdi, r8
	jbe	".Lsum_block3"
.Lsum_block2:
	# v15 = iconst.i64 2
	# v7 = shl v40, v15
	# v8 = ptradd v0, v7
	# v9 = load.i32 v8
	mov	ebx, dword ptr [rsi + r8 * 4]
	# v10 = sext.i64 v9
	movsxd	rbx, ebx
	# v11 = iadd v41, v10
	add	r9, rbx
	# v12 = iconst.i64 1
	# v13 = iadd v40, v12
	add	r8, 1
	# v44 = icmp.ugt v1, v13
	# brif v44, block2(v13, v11), block3(v11)
	cmp	rdi, r8
	ja	".Lsum_block2"
.Lsum_block3:
	# return v43
	mov	rax, r9
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lsum_end:
	.cfi_endproc
	.size "sum", .-"sum"
	.section	".note.GNU-stack","",@progbits
