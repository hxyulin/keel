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
	sub	rsp, 32
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lprivate_acc_block0:
	# v2 = stack_addr ss0
	lea	r8, [rbp + -48]
	# v3 = iconst.i32 0
	# v4 = iconst.i64 24
	# v5 = iconst.i64 0
	# store v5, v2
	mov	qword ptr [rbp + -48], 0
	# v6 = iconst.i64 8
	# v7 = ptradd v2, v6
	lea	r12, [r8 + 8]
	# store v5, v7
	mov	qword ptr [r12], 0
	# v8 = iconst.i64 16
	# v9 = ptradd v2, v8
	add	r8, 16
	# store v3, v9
	mov	dword ptr [r8], 0
	# v10 = iconst.i64 0
	xor	r8d, r8d
	# jump block1(v10)
.Lprivate_acc_block1:
	# v12 = icmp.ugt v1, v11
	# brif v12, block2, block3
	cmp	rdi, r8
	jbe	".Lprivate_acc_block3"
.Lprivate_acc_block2:
	# v13 = iconst.i64 3
	# v14 = shl v11, v13
	mov	r9, r8
	shl	r9, 3
	# v15 = ptradd v0, v14
	add	r9, rsi
	# v16 = load.i64 v15
	mov	r9, qword ptr [r9]
	# v17 = iconst.i64 0
	# v18 = icmp.slt v16, v17
	# brif v18, block5, block4
	cmp	r9, 0
	jl	".Lprivate_acc_block5"
.Lprivate_acc_block4:
	# v32 = iconst.i64 8
	# v33 = stack_addr ss0
	lea	r12, [rbp + -48]
	# v34 = ptradd v33, v32
	lea	rbx, [r12 + 8]
	# v35 = load.i64 v34
	mov	r12, qword ptr [rbx]
	# v36 = iadd v16, v35
	add	r9, r12
	# store v36, v34
	mov	qword ptr [rbx], r9
	# jump block6
	jmp	".Lprivate_acc_block6"
.Lprivate_acc_block5:
	# v37 = stack_addr ss0
	# v38 = load.i64 v37
	mov	r12, qword ptr [rbp + -48]
	# v39 = iadd v16, v38
	add	r9, r12
	# store v39, v37
	mov	qword ptr [rbp + -48], r9
	# jump block6
.Lprivate_acc_block6:
	# v40 = iconst.i64 16
	# v41 = stack_addr ss0
	lea	rbx, [rbp + -48]
	# v42 = ptradd v41, v40
	lea	r9, [rbx + 16]
	# v43 = load.i32 v42
	mov	ebx, dword ptr [r9]
	# v44 = iconst.i32 1
	# v45 = iadd v43, v44
	add	ebx, 1
	# store v45, v42
	mov	dword ptr [r9], ebx
	# v46 = iconst.i64 1
	# v47 = iadd v11, v46
	add	r8, 1
	# jump block1(v47)
	jmp	".Lprivate_acc_block1"
.Lprivate_acc_block3:
	# v19 = stack_addr ss0
	lea	rsi, [rbp + -48]
	# v20 = load.i64 align 8 v19
	mov	rdi, qword ptr [rbp + -48]
	# v21 = iconst.i64 3
	# v22 = imul v20, v21
	imul	rdi, rdi, 3
	# v23 = iconst.i64 8
	# v24 = ptradd v19, v23
	lea	r8, [rsi + 8]
	# v25 = load.i64 align 8 v24
	mov	r8, qword ptr [r8]
	# v26 = iadd v22, v25
	add	rdi, r8
	# v27 = iconst.i64 16
	# v28 = ptradd v19, v27
	add	rsi, 16
	# v29 = load.i32 align 8 v28
	mov	esi, dword ptr [rsi]
	# v30 = uext.i64 v29
	# v31 = iadd v26, v30
	add	rsi, rdi
	# return v31
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lprivate_acc_end:
	.cfi_endproc
	.size "private_acc", .-"private_acc"
	.section	".note.GNU-stack","",@progbits
