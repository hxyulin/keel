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
	push	r15
	.cfi_offset 15, -56
	sub	rsp, 56
.Lmain_block0:
	# v0 = iconst.i32 0
	xor	ebx, ebx
	# v1 = stack_addr ss0
	lea	r12, [rbp + -80]
	# v2 = stack_addr ss1
	lea	r13, [rbp + -64]
	# v3 = iconst.i64 8
	mov	eax, 8
	mov	qword ptr [rbp + -96], rax
	# v4 = ptradd v1, v3
	lea	rax, [r12 + 8]
	mov	qword ptr [rbp + -88], rax
	# v5 = iconst.i64 16
	mov	r15d, 16
	# v6 = iconst.i8 7
	mov	esi, 7
	# intrinsic memset(v1, v6, v5)
	mov	rdi, r12
	mov	rdx, r15
	call	"memset"@PLT
	# v7 = iconst.i8 1
	mov	esi, 1
	# intrinsic memset(v1, v7, v3)
	mov	rdi, r12
	mov	rdx, qword ptr [rbp + -96]
	call	"memset"@PLT
	# v8 = load.i64 v1
	mov	rsi, qword ptr [rbp + -80]
	# v9 = iconst.i64 72340172838076673
	movabs	rdi, 72340172838076673
	# v10 = icmp.ne v8, v9
	cmp	rsi, rdi
	setne	sil
	# v11 = iconst.i32 1
	mov	edi, 1
	# v12 = select v10, v11, v0
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rsi, rax
	# v13 = load.i64 v4
	mov	rax, qword ptr [rbp + -88]
	mov	rdi, qword ptr [rax]
	# v14 = iconst.i64 506381209866536711
	movabs	rbx, 506381209866536711
	# v15 = icmp.ne v13, v14
	cmp	rdi, rbx
	setne	dil
	# v16 = iconst.i32 2
	mov	r8d, 2
	# v17 = select v15, v16, v12
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	r14, rax
	# intrinsic memcpy(v2, v1, v5)
	mov	rdi, r13
	mov	rsi, r12
	mov	rdx, r15
	call	"memcpy"@PLT
	# v18 = ptradd v2, v3
	lea	rsi, [r13 + 8]
	# v19 = load.i64 v18
	mov	rsi, qword ptr [rsi]
	# v20 = icmp.ne v19, v14
	cmp	rsi, rbx
	setne	sil
	# v21 = iconst.i32 3
	mov	edi, 3
	# v22 = select v20, v21, v17
	test	sil, sil
	mov	rax, r14
	cmovne	rax, rdi
	mov	rbx, rax
	# v23 = iconst.i64 506097522914230528
	movabs	rsi, 506097522914230528
	# store v23, v1
	mov	qword ptr [rbp + -80], rsi
	# v24 = iconst.i64 1084818905618843912
	movabs	rsi, 1084818905618843912
	# store v24, v4
	mov	rcx, qword ptr [rbp + -88]
	mov	qword ptr [rcx], rsi
	# v25 = iconst.i64 1
	# v26 = ptradd v1, v25
	lea	rsi, [r12 + 1]
	# v27 = iconst.i64 15
	mov	edi, 15
	# intrinsic memmove(v26, v1, v27)
	mov	rdx, rdi
	mov	rdi, rsi
	mov	rsi, r12
	call	"memmove"@PLT
	# v28 = load.i64 v1
	mov	rsi, qword ptr [rbp + -80]
	# v29 = iconst.i64 433757350076153856
	movabs	rdi, 433757350076153856
	# v30 = icmp.ne v28, v29
	cmp	rsi, rdi
	setne	sil
	# v31 = iconst.i32 4
	mov	edi, 4
	# v32 = select v30, v31, v22
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rsi, rax
	# v33 = load.i64 v4
	mov	rax, qword ptr [rbp + -88]
	mov	rdi, qword ptr [rax]
	# v34 = iconst.i64 1012478732780767239
	movabs	r8, 1012478732780767239
	# v35 = icmp.ne v33, v34
	cmp	rdi, r8
	setne	dil
	# v36 = iconst.i32 5
	mov	r9d, 5
	# v37 = select v35, v36, v32
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r9
	mov	rsi, rax
	# v38 = ptrtoint.i64 v1
	mov	rdi, r12
	# v39 = ptrtoint.i64 v4
	mov	rax, qword ptr [rbp + -88]
	mov	r9, rax
	# v40 = isub v39, v38
	sub	r9, rdi
	# v41 = icmp.ne v40, v3
	cmp	r9, 8
	setne	r9b
	# v42 = iconst.i32 6
	mov	ebx, 6
	# v43 = select v41, v42, v37
	test	r9b, r9b
	mov	rax, rsi
	cmovne	rax, rbx
	mov	rsi, rax
	# v44 = iadd v38, v3
	add	rdi, 8
	# v45 = inttoptr.ptr v44
	# v46 = load.i64 v45
	mov	rdi, qword ptr [rdi]
	# v47 = icmp.ne v46, v34
	cmp	rdi, r8
	setne	dil
	# v48 = iconst.i32 7
	mov	r8d, 7
	# v49 = select v47, v48, v43
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# return v49
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
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
