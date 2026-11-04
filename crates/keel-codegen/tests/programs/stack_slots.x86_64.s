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
	sub	rsp, 5072
.Lmain_block0:
	# v0 = stack_addr ss1
	lea	rsi, [rbp + -88]
	# v1 = iconst.i64 0
	xor	edi, edi
	# jump block1(v1)
	mov	r8, rdi
.Lmain_block1:
	# v3 = iconst.i64 10
	# v4 = icmp.ult v2, v3
	cmp	r8, 10
	setb	r9b
	# brif v4, block2, block3(v1, v1)
	test	r9b, r9b
	jne	".Lmain_block2"
	mov	r8, rdi
.Lmain_block3:
	# v15 = iconst.i64 10
	# v16 = icmp.ult v13, v15
	cmp	rdi, 10
	setb	r9b
	# brif v16, block4, block5
	test	r9b, r9b
	jne	".Lmain_block4"
.Lmain_block5:
	# v25 = iconst.i32 0
	xor	esi, esi
	# v26 = iconst.i64 135
	# v27 = icmp.ne v14, v26
	cmp	r8, 135
	setne	dil
	# v28 = iconst.i32 1
	mov	r8d, 1
	# v29 = select v27, v28, v25
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v30 = stack_addr ss3
	lea	rdi, [rbp + -40]
	# v31 = stack_addr ss4
	lea	r8, [rbp + -32]
	# v32 = iconst.i64 0
	# v33 = ptrtoint.i64 v30
	mov	rbx, rdi
	# v34 = iconst.i64 7
	# v35 = and v33, v34
	and	rbx, 7
	# v36 = icmp.ne v35, v32
	cmp	rbx, 0
	setne	bl
	# v37 = iconst.i32 2
	mov	r12d, 2
	# v38 = select v36, v37, v29
	test	bl, bl
	mov	rax, rsi
	cmovne	rax, r12
	mov	rsi, rax
	# v39 = ptrtoint.i64 v31
	# v40 = iconst.i64 15
	# v41 = and v39, v40
	and	r8, 15
	# v42 = icmp.ne v41, v32
	cmp	r8, 0
	setne	r8b
	# v43 = iconst.i32 3
	mov	r9d, 3
	# v44 = select v42, v43, v38
	test	r8b, r8b
	mov	rax, rsi
	cmovne	rax, r9
	mov	rsi, rax
	# v45 = iconst.i32 287454020
	# store v45, v30
	mov	dword ptr [rbp + -40], 287454020
	# v46 = load.i8 v30
	movzx	r8d, byte ptr [rbp + -40]
	# v47 = iconst.i8 68
	# v48 = icmp.ne v46, v47
	cmp	r8b, 68
	setne	r8b
	# v49 = iconst.i32 4
	mov	r9d, 4
	# v50 = select v48, v49, v44
	test	r8b, r8b
	mov	rax, rsi
	cmovne	rax, r9
	mov	rsi, rax
	# v51 = iconst.i64 2
	# v52 = ptradd v30, v51
	lea	r8, [rdi + 2]
	# v53 = load.i16 v52
	movzx	r8d, word ptr [r8]
	# v54 = iconst.i16 4386
	# v55 = icmp.ne v53, v54
	cmp	r8w, 4386
	setne	r8b
	# v56 = iconst.i32 5
	mov	r9d, 5
	# v57 = select v55, v56, v50
	test	r8b, r8b
	mov	rax, rsi
	cmovne	rax, r9
	mov	rsi, rax
	# v58 = iconst.i8 127
	# v59 = stack_addr ss2
	# store v58, v59
	mov	byte ptr [rbp + -48], 127
	# v60 = iconst.i64 -1
	# store v60, v30
	mov	qword ptr [rbp + -40], -1
	# v61 = stack_addr ss4
	lea	rdi, [rbp + -32]
	# store v60, v61
	mov	qword ptr [rbp + -32], -1
	# v62 = iconst.i64 8
	# v63 = ptradd v61, v62
	add	rdi, 8
	# store v60, v63
	mov	qword ptr [rdi], -1
	# v64 = load.i8 v59
	movzx	edi, byte ptr [rbp + -48]
	# v65 = icmp.ne v64, v58
	cmp	dil, 127
	setne	dil
	# v66 = iconst.i32 6
	mov	r8d, 6
	# v67 = select v65, v66, v57
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v68 = stack_addr ss0
	lea	rdi, [rbp + -5088]
	# v69 = iconst.i64 4992
	# v70 = ptradd v68, v69
	add	rdi, 4992
	# v71 = iconst.i64 123456789
	# store v71, v70
	mov	qword ptr [rdi], 123456789
	# v72 = load.i64 v70
	mov	rdi, qword ptr [rdi]
	# v73 = icmp.ne v72, v71
	cmp	rdi, 123456789
	setne	dil
	# v74 = iconst.i32 7
	mov	r8d, 7
	# v75 = select v73, v74, v67
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v76 = stack_addr ss0
	lea	rdi, [rbp + -5088]
	# v77 = stack_addr ss1
	lea	r8, [rbp + -88]
	# v78 = ptrtoint.i64 v76
	# v79 = ptrtoint.i64 v77
	# v80 = isub v79, v78
	mov	rax, r8
	sub	rax, rdi
	mov	rdi, rax
	# v81 = iconst.i64 5000
	# v82 = icmp.ult v80, v81
	cmp	rdi, 5000
	setb	dil
	# v83 = iconst.i32 8
	mov	r8d, 8
	# v84 = select v82, v83, v75
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# return v84
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lmain_block4:
	# v17 = iconst.i64 4
	# v18 = imul v13, v17
	imul	r9, rdi, 4
	# v19 = ptradd v0, v18
	add	r9, rsi
	# v20 = load.i32 v19
	mov	r9d, dword ptr [r9]
	# v21 = sext.i64 v20
	movsxd	r9, r9d
	# v22 = iadd v14, v21
	add	r8, r9
	# v23 = iconst.i64 1
	# v24 = iadd v13, v23
	add	rdi, 1
	# jump block3(v24, v22)
	jmp	".Lmain_block3"
.Lmain_block2:
	# v5 = iconst.i64 4
	# v6 = imul v2, v5
	imul	r9, r8, 4
	# v7 = ptradd v0, v6
	add	r9, rsi
	# v8 = iconst.i64 3
	# v9 = imul v2, v8
	imul	rbx, r8, 3
	# v10 = trunc.i32 v9
	# store v10, v7
	mov	dword ptr [r9], ebx
	# v11 = iconst.i64 1
	# v12 = iadd v2, v11
	add	r8, 1
	# jump block1(v12)
	jmp	".Lmain_block1"
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
