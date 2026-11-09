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
	push	r14
	.cfi_offset 14, -48
	sub	rsp, 16
.Lmain_block0:
	# v1 = stack_addr ss0
	lea	rsi, [rbp + -48]
	# v3 = iconst.i64 8
	# v4 = ptradd v1, v3
	lea	rdi, [rsi + 8]
	# v52 = iconst.i64 506381209866536711
	movabs	r8, 506381209866536711
	# store notrap v52, v1
	mov	qword ptr [rbp + -48], r8
	# store notrap v52, v4
	mov	qword ptr [rdi], r8
	# v57 = iconst.i64 72340172838076673
	movabs	r8, 72340172838076673
	# store notrap v57, v1
	mov	qword ptr [rbp + -48], r8
	# v12 = iconst.i32 0
	xor	r8d, r8d
	# v23 = iconst.i64 506097522914230528
	movabs	r9, 506097522914230528
	# store notrap v23, v1
	mov	qword ptr [rbp + -48], r9
	# v24 = iconst.i64 1084818905618843912
	movabs	rbx, 1084818905618843912
	# store notrap v24, v4
	mov	qword ptr [rdi], rbx
	# v25 = iconst.i64 1
	# v26 = ptradd v1, v25
	lea	rbx, [rsi + 1]
	# v67 = load.i32 notrap v4
	mov	r12d, dword ptr [rdi]
	# v68 = iconst.i64 12
	# v69 = ptradd v1, v68
	# v70 = load.i16 notrap v69
	movzx	r13d, word ptr [rsi + 12]
	# v71 = iconst.i64 14
	# v72 = ptradd v1, v71
	# v73 = load.i8 notrap v72
	movzx	r14d, byte ptr [rsi + 14]
	# store notrap v23, v26
	mov	qword ptr [rbx], r9
	# v75 = ptradd v26, v3
	# store notrap v67, v75
	mov	dword ptr [rbx + 8], r12d
	# v77 = ptradd v26, v68
	# store notrap v70, v77
	mov	word ptr [rbx + 12], r13w
	# v79 = ptradd v26, v71
	# store notrap v73, v79
	mov	byte ptr [rbx + 14], r14b
	# v28 = load.i64 notrap v1
	mov	r9, qword ptr [rbp + -48]
	# v29 = iconst.i64 433757350076153856
	movabs	rbx, 433757350076153856
	# v30 = icmp.ne v28, v29
	cmp	r9, rbx
	setne	r9b
	# v31 = iconst.i32 4
	mov	ebx, 4
	# v32 = select v30, v31, v12
	test	r9b, r9b
	mov	rax, r8
	cmovne	rax, rbx
	mov	r8, rax
	# v33 = load.i64 notrap v4
	mov	r9, qword ptr [rdi]
	# v34 = iconst.i64 1012478732780767239
	movabs	rbx, 1012478732780767239
	# v35 = icmp.ne v33, v34
	cmp	r9, rbx
	setne	r9b
	# v36 = iconst.i32 5
	mov	r12d, 5
	# v37 = select v35, v36, v32
	test	r9b, r9b
	mov	rax, r8
	cmovne	rax, r12
	mov	r8, rax
	# v38 = ptrtoint.i64 v1
	# v39 = ptrtoint.i64 v4
	# v40 = isub v39, v38
	sub	rdi, rsi
	# v41 = icmp.ne v40, v3
	cmp	rdi, 8
	setne	dil
	# v42 = iconst.i32 6
	mov	r9d, 6
	# v43 = select v41, v42, v37
	test	dil, dil
	mov	rax, r8
	cmovne	rax, r9
	mov	rdi, rax
	# v44 = iadd v38, v3
	add	rsi, 8
	# v45 = inttoptr.ptr v44
	# v46 = load.i64 v45
	mov	rsi, qword ptr [rsi]
	# v47 = icmp.ne v46, v34
	cmp	rsi, rbx
	setne	sil
	# v48 = iconst.i32 7
	mov	r8d, 7
	# v49 = select v47, v48, v43
	test	sil, sil
	mov	rax, rdi
	cmovne	rax, r8
	mov	rsi, rax
	# return v49
	mov	rax, rsi
	lea	rsp, [rbp + -32]
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
