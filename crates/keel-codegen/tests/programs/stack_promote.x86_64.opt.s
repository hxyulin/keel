	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "private_acc"
	.p2align	4
	.type	"private_acc",@function
"private_acc":
.Lprivate_acc_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lprivate_acc_block0:
	# v3 = iconst.i32 0
	xor	r8d, r8d
	# v5 = iconst.i64 0
	xor	r9d, r9d
	# v12 = icmp.ugt v1, v5
	# brif v12, block2(v5, v5, v5, v3), block3(v5, v5, v3)
	cmp	rdi, 0
	jbe	".Lprivate_acc_inst14_else"
	mov	rbx, r9
	mov	r12, r8
	mov	r8, r9
	jmp	".Lprivate_acc_block2"
.Lprivate_acc_inst14_else:
	mov	rsi, r9
	jmp	".Lprivate_acc_block3"
.Lprivate_acc_block2:
	# v13 = iconst.i64 3
	# v14 = shl v75, v13
	# v15 = ptradd v0, v14
	# v16 = load.i64 v15
	mov	r13, qword ptr [rsi + r9 * 8]
	# v54 = iconst.i64 0
	# v18 = icmp.slt v16, v54
	# brif v18, block5, block4
	cmp	r13, 0
	jl	".Lprivate_acc_block5"
.Lprivate_acc_block4:
	# v36 = iadd v16, v77
	add	r13, rbx
	# jump block6(v76, v36)
	jmp	".Lprivate_acc_block6"
.Lprivate_acc_block5:
	# v39 = iadd v16, v76
	add	r13, r8
	# jump block6(v39, v77)
	mov	r8, r13
	mov	r13, rbx
.Lprivate_acc_block6:
	# v44 = iconst.i32 1
	# v45 = iadd v78, v44
	add	r12d, 1
	# v46 = iconst.i64 1
	# v47 = iadd v75, v46
	add	r9, 1
	# v83 = icmp.ugt v1, v47
	# brif v83, block2(v47, v62, v68, v45), block3(v62, v68, v45)
	cmp	rdi, r9
	jbe	".Lprivate_acc_inst57_else"
	mov	rbx, r13
	jmp	".Lprivate_acc_block2"
.Lprivate_acc_inst57_else:
	mov	r9, r8
	mov	rsi, r13
	mov	r8, r12
.Lprivate_acc_block3:
	# v21 = iconst.i64 3
	# v22 = imul v80, v21
	imul	r9, r9, 3
	# v26 = iadd v22, v81
	add	r9, rsi
	# v30 = uext.i64 v82
	mov	esi, r8d
	# v31 = iadd v26, v30
	add	r9, rsi
	# return v31
	mov	rax, r9
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lprivate_acc_end:
	.cfi_endproc
	.size "private_acc", .-"private_acc"
	.section	".note.GNU-stack","",@progbits
