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
	sub	rsp, 5080
.Lmain_block0:
	# v1 = iconst.i64 0
	xor	esi, esi
	# jump block1(v1)
.Lmain_block1:
	# v3 = iconst.i64 10
	# v4 = icmp.ult v2, v3
	cmp	rsi, 10
	setb	dil
	# v94 = iconst.i64 0
	xor	r8d, r8d
	# brif v4, block2, block3
	test	dil, dil
	jne	".Lmain_block2"
.Lmain_block3:
	# jump block4(v94, v94)
	mov	rsi, r8
	mov	rdi, r8
.Lmain_block4:
	# v86 = iconst.i64 2
	# v18 = shl v95, v86
	# v89 = stack_addr ss1
	lea	r8, [rbp + -88]
	# v19 = ptradd v89, v18
	# v20 = load.i32 v19
	mov	r8d, dword ptr [r8 + rsi * 4]
	# v21 = sext.i64 v20
	movsxd	r8, r8d
	# v22 = iadd v96, v21
	add	rdi, r8
	# v23 = iconst.i64 1
	# v24 = iadd v95, v23
	add	rsi, 1
	# v99 = iconst.i64 10
	# v100 = icmp.ult v24, v99
	# brif v100, block4(v24, v22), block5(v22)
	cmp	rsi, 10
	jb	".Lmain_block4"
	mov	rsi, rdi
.Lmain_block5:
	# v26 = iconst.i64 135
	# v27 = icmp.ne v98, v26
	cmp	rsi, 135
	setne	sil
	# v29 = uext.i32 v27
	movzx	esi, sil
	# v30 = stack_addr ss3
	lea	rdi, [rbp + -48]
	# v31 = stack_addr ss4
	lea	r8, [rbp + -32]
	# v33 = ptrtoint.i64 v30
	mov	r9, rdi
	# v34 = iconst.i64 7
	# v35 = and v33, v34
	and	r9, 7
	# v37 = iconst.i32 2
	mov	ebx, 2
	# v38 = select v35, v37, v29
	test	r9, r9
	mov	rax, rsi
	cmovne	rax, rbx
	mov	rsi, rax
	# v39 = ptrtoint.i64 v31
	mov	r9, r8
	# v40 = iconst.i64 15
	# v41 = and v39, v40
	and	r9, 15
	# v43 = iconst.i32 3
	mov	ebx, 3
	# v44 = select v41, v43, v38
	test	r9, r9
	mov	rax, rsi
	cmovne	rax, rbx
	mov	rsi, rax
	# v45 = iconst.i32 287454020
	# store notrap v45, v30
	mov	dword ptr [rbp + -48], 287454020
	# v46 = load.i8 notrap v30
	movzx	r9d, byte ptr [rbp + -48]
	# v47 = iconst.i8 68
	# v48 = icmp.ne v46, v47
	cmp	r9b, 68
	setne	r9b
	# v49 = iconst.i32 4
	mov	ebx, 4
	# v50 = select v48, v49, v44
	test	r9b, r9b
	mov	rax, rsi
	cmovne	rax, rbx
	mov	rsi, rax
	# v51 = iconst.i64 2
	# v52 = ptradd v30, v51
	# v53 = load.i16 notrap v52
	movzx	edi, word ptr [rdi + 2]
	# v54 = iconst.i16 4386
	# v55 = icmp.ne v53, v54
	cmp	di, 4386
	setne	dil
	# v56 = iconst.i32 5
	mov	r9d, 5
	# v57 = select v55, v56, v50
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r9
	mov	rsi, rax
	# v60 = iconst.i64 -1
	# store notrap v60, v30
	mov	qword ptr [rbp + -48], -1
	# store notrap v60, v31
	mov	qword ptr [rbp + -32], -1
	# v62 = iconst.i64 8
	# v63 = ptradd v31, v62
	# store notrap v60, v63
	mov	qword ptr [r8 + 8], -1
	# v68 = stack_addr ss0
	lea	rdi, [rbp + -5088]
	# v69 = iconst.i64 4992
	# v70 = ptradd v68, v69
	# v71 = iconst.i64 123456789
	# store notrap v71, v70
	mov	qword ptr [rdi + 4992], 123456789
	# v78 = ptrtoint.i64 v68
	# v0 = stack_addr ss1
	lea	r8, [rbp + -88]
	# v79 = ptrtoint.i64 v0
	# v80 = isub v79, v78
	sub	r8, rdi
	# v81 = iconst.i64 5000
	# v82 = icmp.ult v80, v81
	cmp	r8, 5000
	setb	dil
	# v83 = iconst.i32 8
	mov	r8d, 8
	# v84 = select v82, v83, v57
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# return v84
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lmain_block2:
	# v85 = iconst.i64 2
	# v6 = shl v2, v85
	# v90 = stack_addr ss1
	lea	rdi, [rbp + -88]
	# v7 = ptradd v90, v6
	# v8 = iconst.i64 3
	# v9 = imul v2, v8
	imul	r8, rsi, 3
	# v10 = trunc.i32 v9
	# store v10, v7
	mov	dword ptr [rdi + rsi * 4], r8d
	# v11 = iconst.i64 1
	# v12 = iadd v2, v11
	add	rsi, 1
	# jump block1(v12)
	jmp	".Lmain_block1"
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
