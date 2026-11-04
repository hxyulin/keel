	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "rotate3"
	.p2align	4
	.type	"rotate3",@function
"rotate3":
.Lrotate3_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lrotate3_block0:
	# jump block1(v0, v1, v2, v3)
.Lrotate3_block1:
	# v8 = iconst.i64 0
	# v9 = icmp.eq v7, v8
	cmp	r9, 0
	sete	bl
	# v10 = iconst.i64 1
	# v11 = isub v7, v10
	sub	r9, 1
	# brif v9, block2, block1(v5, v6, v4, v11)
	test	bl, bl
	jne	".Lrotate3_block2"
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r8
	mov	r8, r11
	jmp	".Lrotate3_block1"
.Lrotate3_block2:
	# v12 = iconst.i64 100
	# v13 = imul v4, v12
	imul	rsi, rsi, 100
	# v14 = iconst.i64 10
	# v15 = imul v5, v14
	imul	rdi, rdi, 10
	# v16 = iadd v13, v15
	add	rsi, rdi
	# v17 = iadd v6, v16
	add	r8, rsi
	# return v17
	mov	rax, r8
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lrotate3_end:
	.cfi_endproc
	.size "rotate3", .-"rotate3"
	.section	".text","ax",@progbits
	.globl "rotate_many"
	.p2align	4
	.type	"rotate_many",@function
"rotate_many":
.Lrotate_many_begin:
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
	sub	rsp, 104
	mov	qword ptr [rbp + -144], rsi
	mov	rsi, rdi
.Lrotate_many_block0:
	# v4 = iconst.i64 1
	# v5 = iadd v0, v4
	lea	rax, [rsi + 1]
	mov	qword ptr [rbp + -136], rax
	# v6 = iconst.i64 2
	# v7 = iadd v0, v6
	lea	rax, [rsi + 2]
	mov	qword ptr [rbp + -128], rax
	# v8 = iconst.i64 3
	# v9 = iadd v0, v8
	lea	rax, [rsi + 3]
	mov	qword ptr [rbp + -120], rax
	# v10 = iconst.i64 4
	# v11 = iadd v0, v10
	lea	rax, [rsi + 4]
	mov	qword ptr [rbp + -112], rax
	# v12 = iconst.i64 5
	# v13 = iadd v0, v12
	lea	rax, [rsi + 5]
	mov	qword ptr [rbp + -104], rax
	# v14 = iconst.i64 6
	# v15 = iadd v0, v14
	lea	rax, [rsi + 6]
	mov	qword ptr [rbp + -96], rax
	# v16 = iconst.i64 7
	# v17 = iadd v0, v16
	lea	rax, [rsi + 7]
	mov	qword ptr [rbp + -88], rax
	# v18 = iconst.i64 8
	# v19 = iadd v0, v18
	lea	rax, [rsi + 8]
	mov	qword ptr [rbp + -80], rax
	# v20 = iconst.i64 9
	# v21 = iadd v0, v20
	lea	rax, [rsi + 9]
	mov	qword ptr [rbp + -72], rax
	# v22 = iconst.i64 10
	# v23 = iadd v0, v22
	lea	rax, [rsi + 10]
	mov	qword ptr [rbp + -64], rax
	# v24 = iconst.i64 11
	# v25 = iadd v0, v24
	lea	rax, [rsi + 11]
	mov	qword ptr [rbp + -56], rax
	# v26 = iconst.i64 12
	# v27 = iadd v0, v26
	lea	r12, [rsi + 12]
	# v28 = iconst.i64 13
	# v29 = iadd v0, v28
	lea	r13, [rsi + 13]
	# v30 = iconst.i64 14
	# v31 = iadd v0, v30
	lea	r14, [rsi + 14]
	# v32 = iconst.i64 15
	# v33 = iadd v0, v32
	lea	r15, [rsi + 15]
	# v34 = iconst.i64 16
	# v35 = iadd v0, v34
	lea	rdi, [rsi + 16]
	# v36 = iconst.i64 17
	# v37 = iadd v0, v36
	lea	r8, [rsi + 17]
	# v38 = iconst.i64 18
	# v39 = iadd v0, v38
	lea	r9, [rsi + 18]
	# v40 = iconst.i64 19
	# v41 = iadd v0, v40
	lea	rbx, [rsi + 19]
	# jump block1(v0, v5, v7, v9, v11, v13, v15, v17, v19, v21, v23, v25, v27, v29, v31, v33, v35, v37, v39, v41, v1)
	mov	qword ptr [rbp + -48], rbx
	mov	rbx, qword ptr [rbp + -112]
	mov	rax, qword ptr [rbp + -56]
	mov	qword ptr [rbp + -112], rax
	mov	qword ptr [rbp + -56], r9
	mov	r9, qword ptr [rbp + -120]
	mov	rax, qword ptr [rbp + -64]
	mov	qword ptr [rbp + -120], rax
	mov	qword ptr [rbp + -64], r8
	mov	r8, qword ptr [rbp + -128]
	mov	rax, qword ptr [rbp + -72]
	mov	qword ptr [rbp + -128], rax
	mov	qword ptr [rbp + -72], rdi
	mov	rdi, qword ptr [rbp + -136]
	mov	rax, qword ptr [rbp + -80]
	mov	qword ptr [rbp + -136], rax
	mov	qword ptr [rbp + -80], r15
	mov	r15, qword ptr [rbp + -144]
	mov	rax, qword ptr [rbp + -88]
	mov	qword ptr [rbp + -144], rax
	mov	qword ptr [rbp + -88], r14
	mov	r11, r12
	mov	r12, qword ptr [rbp + -104]
	mov	qword ptr [rbp + -104], r11
	mov	r11, r13
	mov	r13, qword ptr [rbp + -96]
	mov	qword ptr [rbp + -96], r11
.Lrotate_many_block1:
	# v2 = iconst.i64 0
	# v64 = icmp.eq v62, v2
	cmp	r15, 0
	sete	r14b
	# v111 = iconst.i64 1
	# v66 = isub v62, v111
	sub	r15, 1
	# brif v64, block2, block1(v43, v44, v45, v46, v47, v48, v49, v50, v51, v52, v53, v54, v55, v56, v57, v58, v59, v60, v61, v42, v66)
	test	r14b, r14b
	jne	".Lrotate_many_block2"
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r8
	mov	r8, r9
	mov	r9, rbx
	mov	rbx, r12
	mov	r12, r13
	mov	r13, qword ptr [rbp + -144]
	mov	rax, qword ptr [rbp + -136]
	mov	qword ptr [rbp + -144], rax
	mov	rax, qword ptr [rbp + -128]
	mov	qword ptr [rbp + -136], rax
	mov	rax, qword ptr [rbp + -120]
	mov	qword ptr [rbp + -128], rax
	mov	rax, qword ptr [rbp + -112]
	mov	qword ptr [rbp + -120], rax
	mov	rax, qword ptr [rbp + -104]
	mov	qword ptr [rbp + -112], rax
	mov	rax, qword ptr [rbp + -96]
	mov	qword ptr [rbp + -104], rax
	mov	rax, qword ptr [rbp + -88]
	mov	qword ptr [rbp + -96], rax
	mov	rax, qword ptr [rbp + -80]
	mov	qword ptr [rbp + -88], rax
	mov	rax, qword ptr [rbp + -72]
	mov	qword ptr [rbp + -80], rax
	mov	rax, qword ptr [rbp + -64]
	mov	qword ptr [rbp + -72], rax
	mov	rax, qword ptr [rbp + -56]
	mov	qword ptr [rbp + -64], rax
	mov	rax, qword ptr [rbp + -48]
	mov	qword ptr [rbp + -56], rax
	mov	qword ptr [rbp + -48], r11
	jmp	".Lrotate_many_block1"
.Lrotate_many_block2:
	# v67 = iconst.i64 31
	# v71 = imul v42, v67
	imul	rsi, rsi, 31
	# v72 = iadd v43, v71
	add	rdi, rsi
	# v73 = imul v72, v67
	imul	rdi, rdi, 31
	# v74 = iadd v44, v73
	add	r8, rdi
	# v75 = imul v74, v67
	imul	r8, r8, 31
	# v76 = iadd v45, v75
	add	r9, r8
	# v77 = imul v76, v67
	imul	r9, r9, 31
	# v78 = iadd v46, v77
	add	rbx, r9
	# v79 = imul v78, v67
	imul	rbx, rbx, 31
	# v80 = iadd v47, v79
	add	r12, rbx
	# v81 = imul v80, v67
	imul	r12, r12, 31
	# v82 = iadd v48, v81
	add	r13, r12
	# v83 = imul v82, v67
	imul	r13, r13, 31
	# v84 = iadd v49, v83
	mov	rax, qword ptr [rbp + -144]
	lea	rsi, [rax + r13 * 1]
	# v85 = imul v84, v67
	imul	rsi, rsi, 31
	# v86 = iadd v50, v85
	mov	rax, qword ptr [rbp + -136]
	add	rsi, rax
	# v87 = imul v86, v67
	imul	rsi, rsi, 31
	# v88 = iadd v51, v87
	mov	rax, qword ptr [rbp + -128]
	add	rsi, rax
	# v89 = imul v88, v67
	imul	rsi, rsi, 31
	# v90 = iadd v52, v89
	mov	rax, qword ptr [rbp + -120]
	add	rsi, rax
	# v91 = imul v90, v67
	imul	rsi, rsi, 31
	# v92 = iadd v53, v91
	mov	rax, qword ptr [rbp + -112]
	add	rsi, rax
	# v93 = imul v92, v67
	imul	rsi, rsi, 31
	# v94 = iadd v54, v93
	mov	rax, qword ptr [rbp + -104]
	add	rsi, rax
	# v95 = imul v94, v67
	imul	rsi, rsi, 31
	# v96 = iadd v55, v95
	mov	rax, qword ptr [rbp + -96]
	add	rsi, rax
	# v97 = imul v96, v67
	imul	rsi, rsi, 31
	# v98 = iadd v56, v97
	mov	rax, qword ptr [rbp + -88]
	add	rsi, rax
	# v99 = imul v98, v67
	imul	rsi, rsi, 31
	# v100 = iadd v57, v99
	mov	rax, qword ptr [rbp + -80]
	add	rsi, rax
	# v101 = imul v100, v67
	imul	rsi, rsi, 31
	# v102 = iadd v58, v101
	mov	rax, qword ptr [rbp + -72]
	add	rsi, rax
	# v103 = imul v102, v67
	imul	rsi, rsi, 31
	# v104 = iadd v59, v103
	mov	rax, qword ptr [rbp + -64]
	add	rsi, rax
	# v105 = imul v104, v67
	imul	rsi, rsi, 31
	# v106 = iadd v60, v105
	mov	rax, qword ptr [rbp + -56]
	add	rsi, rax
	# v107 = imul v106, v67
	imul	rsi, rsi, 31
	# v108 = iadd v61, v107
	mov	rax, qword ptr [rbp + -48]
	add	rsi, rax
	# return v108
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lrotate_many_end:
	.cfi_endproc
	.size "rotate_many", .-"rotate_many"
	.section	".text","ax",@progbits
	.globl "invariant"
	.p2align	4
	.type	"invariant",@function
"invariant":
.Linvariant_begin:
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
.Linvariant_block0:
	# v2 = iconst.i64 3
	# v3 = imul v0, v2
	imul	r8, rsi, 3
	# v4 = iconst.i64 7
	# v5 = iadd v0, v4
	add	rsi, 7
	# v6 = iconst.i64 0
	xor	r9d, r9d
	# jump block1(v1, v6)
.Linvariant_block1:
	# v25 = iconst.i64 0
	# v10 = icmp.eq v7, v25
	# brif v10, block3, block2
	cmp	rdi, 0
	je	".Linvariant_block3"
.Linvariant_block2:
	# v11 = iadd v3, v8
	add	r9, r8
	# v12 = xor v5, v11
	xor	r9, rsi
	# v13 = iconst.i64 1
	# v14 = isub v7, v13
	sub	rdi, 1
	# v15 = iconst.i64 5
	# v16 = imul v14, v15
	imul	rbx, rdi, 5
	# v17 = iconst.i64 11
	# v18 = iadd v16, v17
	add	rbx, 11
	# v19 = iconst.i64 13
	# v20 = xor v18, v19
	mov	r12, rbx
	xor	r12, 13
	# v21 = imul v18, v20
	imul	rbx, r12
	# v22 = iadd v12, v21
	add	r9, rbx
	# jump block1(v14, v22)
	jmp	".Linvariant_block1"
.Linvariant_block3:
	# return v8
	mov	rax, r9
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Linvariant_end:
	.cfi_endproc
	.size "invariant", .-"invariant"
	.section	".note.GNU-stack","",@progbits
