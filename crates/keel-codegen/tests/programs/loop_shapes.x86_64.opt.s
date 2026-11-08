	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_isort"
	.p2align	4
	.type	"k_isort",@function
"k_isort":
.Lk_isort_begin:
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
.Lk_isort_block0:
	# v2 = iconst.i64 1
	mov	r8d, 1
	# jump block1(v2)
.Lk_isort_block1:
	# v4 = icmp.ugt v1, v3
	# brif v4, block2, block3
	cmp	rdi, r8
	ja	".Lk_isort_block2"
.Lk_isort_block3:
	# return
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_isort_block2:
	# v5 = iconst.i64 2
	# v6 = shl v3, v5
	# v7 = ptradd v0, v6
	# v8 = load.i32 v7
	mov	r9d, dword ptr [rsi + r8 * 4]
	# v10 = iconst.i64 0
	# v11 = icmp.ne v3, v10
	# brif v11, block7(v3), block6(v3)
	cmp	r8, 0
	je	".Lk_isort_inst12_else"
	mov	rbx, r8
	jmp	".Lk_isort_block7"
.Lk_isort_inst12_else:
	mov	rbx, r8
	jmp	".Lk_isort_block6"
.Lk_isort_block7:
	# v41 = iconst.i64 1
	# v22 = isub v43, v41
	mov	r12, rbx
	sub	r12, 1
	# v38 = iconst.i64 2
	# v24 = shl v22, v38
	# v25 = ptradd v0, v24
	# v26 = load.i32 v25
	mov	r13d, dword ptr [rsi + r12 * 4]
	# v27 = icmp.slt v8, v26
	# brif v27, block5, block6(v43)
	cmp	r9d, r13d
	jge	".Lk_isort_block6"
.Lk_isort_block5:
	# v40 = iconst.i64 2
	# v13 = shl v43, v40
	# v14 = ptradd v0, v13
	# store v26, v14
	mov	dword ptr [rsi + rbx * 4], r13d
	# v45 = iconst.i64 0
	# v46 = icmp.ne v22, v45
	# brif v46, block7(v22), block6(v22)
	cmp	r12, 0
	je	".Lk_isort_inst18_else"
	mov	rbx, r12
	jmp	".Lk_isort_block7"
.Lk_isort_inst18_else:
	mov	rbx, r12
.Lk_isort_block6:
	# v39 = iconst.i64 2
	# v17 = shl v44, v39
	# v18 = ptradd v0, v17
	# store v8, v18
	mov	dword ptr [rsi + rbx * 4], r9d
	# v42 = iconst.i64 1
	# v20 = iadd v3, v42
	add	r8, 1
	# jump block1(v20)
	jmp	".Lk_isort_block1"
.Lk_isort_end:
	.cfi_endproc
	.size "k_isort", .-"k_isort"
	.section	".text","ax",@progbits
	.globl "k_matmul"
	.p2align	4
	.type	"k_matmul",@function
"k_matmul":
.Lk_matmul_begin:
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
	sub	rsp, 40
	mov	qword ptr [rbp + -80], rsi
	mov	rsi, rdi
	mov	qword ptr [rbp + -72], rdx
	mov	r9, rcx
.Lk_matmul_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# jump block1(v4)
	mov	qword ptr [rbp + -64], rbx
.Lk_matmul_block1:
	# v6 = icmp.sgt v3, v5
	# brif v6, block2, block4
	mov	rcx, qword ptr [rbp + -64]
	cmp	r9d, ecx
	jg	".Lk_matmul_block2"
.Lk_matmul_block4:
	# return
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_matmul_block2:
	# v7 = imul v3, v5
	mov	rcx, qword ptr [rbp + -64]
	mov	r12, r9
	imul	r12d, ecx
	# v44 = iconst.i32 0
	xor	r13d, r13d
	# jump block5(v44)
	mov	qword ptr [rbp + -56], r13
.Lk_matmul_block5:
	# v12 = icmp.sgt v3, v11
	# brif v12, block6, block3
	mov	rcx, qword ptr [rbp + -56]
	cmp	r9d, ecx
	jg	".Lk_matmul_block6"
.Lk_matmul_block3:
	# v9 = iconst.i32 1
	# v10 = iadd v5, v9
	mov	rax, qword ptr [rbp + -64]
	lea	ebx, [rax + 1]
	# jump block1(v10)
	mov	qword ptr [rbp + -64], rbx
	jmp	".Lk_matmul_block1"
.Lk_matmul_block6:
	# v45 = iconst.i32 0
	xor	r14d, r14d
	# v90 = iconst.i32 0
	xor	r15d, r15d
	# v56 = isub v3, v45
	mov	rbx, r9
	sub	ebx, 0
	# v57 = iconst.i32 2
	# v58 = icmp.sge v56, v57
	# brif v58, block11(v45, v45, v90, v11), block12(v45, v45, v90, v11)
	cmp	ebx, 2
	jl	".Lk_matmul_inst57_else"
	mov	rbx, r14
	mov	r8, qword ptr [rbp + -56]
	jmp	".Lk_matmul_block11"
.Lk_matmul_inst57_else:
	mov	rdi, r14
	mov	r8, r14
	mov	rbx, r15
	mov	r13, qword ptr [rbp + -56]
	jmp	".Lk_matmul_block12"
.Lk_matmul_block11:
	# v59 = iadd v7, v98
	lea	r13d, [r12 + rbx * 1]
	# v60 = sext.i64 v59
	movsxd	r13, r13d
	# v61 = iconst.i64 2
	# v62 = shl v60, v61
	# v63 = ptradd v0, v62
	# v64 = load.i32 v63
	mov	r13d, dword ptr [rsi + r13 * 4]
	# v65 = sext.i64 v101
	movsxd	rdi, r8d
	# v66 = shl v65, v61
	# v67 = ptradd v1, v66
	# v68 = load.i32 v67
	# v69 = imul v64, v68
	mov	rcx, qword ptr [rbp + -80]
	imul	r13d, dword ptr [rcx + rdi * 4]
	# v70 = iadd v99, v69
	add	r14d, r13d
	# v71 = iconst.i32 1
	# v72 = iadd v98, v71
	add	ebx, 1
	# v73 = iadd v3, v101
	lea	edi, [r9 + r8 * 1]
	# v74 = iadd v7, v72
	lea	r8d, [r12 + rbx * 1]
	# v75 = sext.i64 v74
	movsxd	r8, r8d
	# v76 = iconst.i64 2
	# v77 = shl v75, v76
	# v78 = ptradd v0, v77
	# v79 = load.i32 v78
	mov	r8d, dword ptr [rsi + r8 * 4]
	# v80 = sext.i64 v73
	movsxd	r13, edi
	# v81 = shl v80, v76
	# v82 = ptradd v1, v81
	# v83 = load.i32 v82
	# v84 = imul v79, v83
	mov	rcx, qword ptr [rbp + -80]
	imul	r8d, dword ptr [rcx + r13 * 4]
	# v85 = iadd v100, v84
	add	r15d, r8d
	# v86 = iconst.i32 1
	# v87 = iadd v72, v86
	add	ebx, 1
	# v88 = iadd v3, v73
	add	edi, r9d
	# v106 = isub v3, v87
	mov	r8, r9
	sub	r8d, ebx
	# v107 = iconst.i32 2
	# v108 = icmp.sge v106, v107
	# brif v108, block11(v87, v70, v85, v88), block12(v87, v70, v85, v88)
	cmp	r8d, 2
	jl	".Lk_matmul_inst88_else"
	mov	r8, rdi
	jmp	".Lk_matmul_block11"
.Lk_matmul_inst88_else:
	mov	r8, r14
	mov	r13, rdi
	mov	rdi, rbx
	mov	rbx, r15
.Lk_matmul_block12:
	# v89 = iadd v103, v104
	add	r8d, ebx
	# v16 = icmp.sgt v3, v102
	# brif v16, block8(v102, v89, v105), block9(v89)
	cmp	r9d, edi
	jle	".Lk_matmul_inst16_else"
	mov	rbx, r13
	jmp	".Lk_matmul_block8"
.Lk_matmul_inst16_else:
	mov	rdi, r8
	jmp	".Lk_matmul_block9"
.Lk_matmul_block8:
	# v17 = iadd v7, v91
	lea	r13d, [r12 + rdi * 1]
	# v18 = sext.i64 v17
	movsxd	r13, r13d
	# v19 = iconst.i64 2
	# v20 = shl v18, v19
	# v21 = ptradd v0, v20
	# v22 = load.i32 v21
	mov	r13d, dword ptr [rsi + r13 * 4]
	# v25 = sext.i64 v93
	movsxd	r14, ebx
	# v26 = shl v25, v19
	# v27 = ptradd v1, v26
	# v28 = load.i32 v27
	# v29 = imul v22, v28
	mov	rcx, qword ptr [rbp + -80]
	imul	r13d, dword ptr [rcx + r14 * 4]
	# v30 = iadd v92, v29
	add	r8d, r13d
	# v31 = iconst.i32 1
	# v32 = iadd v91, v31
	add	edi, 1
	# v50 = iadd v3, v93
	add	ebx, r9d
	# v97 = icmp.sgt v3, v32
	# brif v97, block8(v32, v30, v50), block9(v30)
	cmp	r9d, edi
	jg	".Lk_matmul_block8"
	mov	rdi, r8
.Lk_matmul_block9:
	# v33 = iadd v7, v11
	mov	rcx, qword ptr [rbp + -56]
	lea	r8d, [r12 + rcx * 1]
	# v34 = sext.i64 v33
	movsxd	r8, r8d
	# v35 = iconst.i64 2
	# v36 = shl v34, v35
	# v37 = ptradd v2, v36
	# store v95, v37
	mov	rcx, qword ptr [rbp + -72]
	mov	dword ptr [rcx + r8 * 4], edi
	# v38 = iconst.i32 1
	# v39 = iadd v11, v38
	mov	rax, qword ptr [rbp + -56]
	lea	edi, [rax + 1]
	# jump block5(v39)
	mov	qword ptr [rbp + -56], rdi
	jmp	".Lk_matmul_block5"
.Lk_matmul_end:
	.cfi_endproc
	.size "k_matmul", .-"k_matmul"
	.section	".text","ax",@progbits
	.globl "signed_dot32"
	.p2align	4
	.type	"signed_dot32",@function
"signed_dot32":
.Lsigned_dot32_begin:
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsigned_dot32_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# v48 = iconst.i32 0
	xor	r12d, r12d
	# v22 = isub v2, v4
	mov	r13, r8
	sub	r13d, 0
	# v23 = iconst.i32 2
	# v24 = icmp.sge v22, v23
	# brif v24, block5(v4, v3, v48), block6(v4, v3, v48)
	cmp	r13d, 2
	jl	".Lsigned_dot32_inst20_else"
	mov	r11, r9
	mov	r9, rbx
	mov	rbx, r11
	jmp	".Lsigned_dot32_block5"
.Lsigned_dot32_inst20_else:
	mov	r11, r9
	mov	r9, rbx
	mov	rbx, r11
	jmp	".Lsigned_dot32_block6"
.Lsigned_dot32_block5:
	# v25 = sext.i64 v54
	movsxd	r13, r9d
	# v26 = iconst.i64 2
	# v27 = shl v25, v26
	shl	r13, 2
	# v28 = ptradd v0, v27
	# v29 = load.i32 v28
	mov	r14d, dword ptr [rsi + r13 * 1]
	# v30 = ptradd v1, v27
	# v31 = load.i32 v30
	# v32 = imul v29, v31
	imul	r14d, dword ptr [rdi + r13 * 1]
	# v33 = iadd v55, v32
	add	ebx, r14d
	# v34 = iconst.i32 1
	# v35 = iadd v54, v34
	add	r9d, 1
	# v36 = sext.i64 v35
	movsxd	r13, r9d
	# v37 = iconst.i64 2
	# v38 = shl v36, v37
	shl	r13, 2
	# v39 = ptradd v0, v38
	# v40 = load.i32 v39
	mov	r14d, dword ptr [rsi + r13 * 1]
	# v41 = ptradd v1, v38
	# v42 = load.i32 v41
	# v43 = imul v40, v42
	imul	r14d, dword ptr [rdi + r13 * 1]
	# v44 = iadd v56, v43
	add	r12d, r14d
	# v45 = iconst.i32 1
	# v46 = iadd v35, v45
	add	r9d, 1
	# v60 = isub v2, v46
	mov	r13, r8
	sub	r13d, r9d
	# v61 = iconst.i32 2
	# v62 = icmp.sge v60, v61
	# brif v62, block5(v46, v33, v44), block6(v46, v33, v44)
	cmp	r13d, 2
	jge	".Lsigned_dot32_block5"
.Lsigned_dot32_block6:
	# v47 = iadd v58, v59
	add	ebx, r12d
	# v7 = icmp.sgt v2, v57
	# brif v7, block2(v57, v47), block3(v47)
	cmp	r8d, r9d
	jg	".Lsigned_dot32_block2"
	mov	rsi, rbx
	jmp	".Lsigned_dot32_block3"
.Lsigned_dot32_block2:
	# v8 = sext.i64 v49
	movsxd	r12, r9d
	# v9 = iconst.i64 2
	# v10 = shl v8, v9
	shl	r12, 2
	# v11 = ptradd v0, v10
	# v12 = load.i32 v11
	mov	r13d, dword ptr [rsi + r12 * 1]
	# v13 = ptradd v1, v10
	# v14 = load.i32 v13
	# v15 = imul v12, v14
	imul	r13d, dword ptr [rdi + r12 * 1]
	# v16 = iadd v50, v15
	add	ebx, r13d
	# v17 = iconst.i32 1
	# v18 = iadd v49, v17
	add	r9d, 1
	# v53 = icmp.sgt v2, v18
	# brif v53, block2(v18, v16), block3(v16)
	cmp	r8d, r9d
	jg	".Lsigned_dot32_block2"
	mov	rsi, rbx
.Lsigned_dot32_block3:
	# return v52
	mov	rax, rsi
	lea	rsp, [rbp + -32]
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lsigned_dot32_end:
	.cfi_endproc
	.size "signed_dot32", .-"signed_dot32"
	.section	".text","ax",@progbits
	.globl "signed_dot64"
	.p2align	4
	.type	"signed_dot64",@function
"signed_dot64":
.Lsigned_dot64_begin:
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsigned_dot64_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# v45 = iconst.i64 0
	xor	r12d, r12d
	# v21 = isub v2, v4
	mov	r13, r8
	sub	r13, 0
	# v22 = iconst.i64 2
	# v23 = icmp.sge v21, v22
	# brif v23, block5(v4, v3, v45), block6(v4, v3, v45)
	cmp	r13, 2
	jl	".Lsigned_dot64_inst19_else"
	mov	r11, r9
	mov	r9, rbx
	mov	rbx, r11
	jmp	".Lsigned_dot64_block5"
.Lsigned_dot64_inst19_else:
	mov	r11, r9
	mov	r9, rbx
	mov	rbx, r11
	jmp	".Lsigned_dot64_block6"
.Lsigned_dot64_block5:
	# v24 = iconst.i64 3
	# v25 = shl v51, v24
	mov	r13, r9
	shl	r13, 3
	# v26 = ptradd v0, v25
	# v27 = load.i64 v26
	mov	r14, qword ptr [rsi + r13 * 1]
	# v28 = ptradd v1, v25
	# v29 = load.i64 v28
	# v30 = imul v27, v29
	imul	r14, qword ptr [rdi + r13 * 1]
	# v31 = iadd v52, v30
	add	rbx, r14
	# v32 = iconst.i64 1
	# v33 = iadd v51, v32
	add	r9, 1
	# v34 = iconst.i64 3
	# v35 = shl v33, v34
	mov	r13, r9
	shl	r13, 3
	# v36 = ptradd v0, v35
	# v37 = load.i64 v36
	mov	r14, qword ptr [rsi + r13 * 1]
	# v38 = ptradd v1, v35
	# v39 = load.i64 v38
	# v40 = imul v37, v39
	imul	r14, qword ptr [rdi + r13 * 1]
	# v41 = iadd v53, v40
	add	r12, r14
	# v42 = iconst.i64 1
	# v43 = iadd v33, v42
	add	r9, 1
	# v57 = isub v2, v43
	mov	r13, r8
	sub	r13, r9
	# v58 = iconst.i64 2
	# v59 = icmp.sge v57, v58
	# brif v59, block5(v43, v31, v41), block6(v43, v31, v41)
	cmp	r13, 2
	jge	".Lsigned_dot64_block5"
.Lsigned_dot64_block6:
	# v44 = iadd v55, v56
	add	rbx, r12
	# v7 = icmp.sgt v2, v54
	# brif v7, block2(v54, v44), block3(v44)
	cmp	r8, r9
	jg	".Lsigned_dot64_block2"
	mov	rsi, rbx
	jmp	".Lsigned_dot64_block3"
.Lsigned_dot64_block2:
	# v8 = iconst.i64 3
	# v9 = shl v46, v8
	mov	r12, r9
	shl	r12, 3
	# v10 = ptradd v0, v9
	# v11 = load.i64 v10
	mov	r13, qword ptr [rsi + r12 * 1]
	# v12 = ptradd v1, v9
	# v13 = load.i64 v12
	# v14 = imul v11, v13
	imul	r13, qword ptr [rdi + r12 * 1]
	# v15 = iadd v47, v14
	add	rbx, r13
	# v16 = iconst.i64 1
	# v17 = iadd v46, v16
	add	r9, 1
	# v50 = icmp.sgt v2, v17
	# brif v50, block2(v17, v15), block3(v15)
	cmp	r8, r9
	jg	".Lsigned_dot64_block2"
	mov	rsi, rbx
.Lsigned_dot64_block3:
	# return v49
	mov	rax, rsi
	lea	rsp, [rbp + -32]
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lsigned_dot64_end:
	.cfi_endproc
	.size "signed_dot64", .-"signed_dot64"
	.section	".text","ax",@progbits
	.globl "div_loop"
	.p2align	4
	.type	"div_loop",@function
"div_loop":
.Ldiv_loop_begin:
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ldiv_loop_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# v7 = icmp.sgt v2, v4
	# brif v7, block2(v4, v4), block3(v4)
	cmp	r8, 0
	jle	".Ldiv_loop_inst3_else"
	mov	r12, rbx
	jmp	".Ldiv_loop_block2"
.Ldiv_loop_inst3_else:
	mov	rsi, rbx
	jmp	".Ldiv_loop_block3"
.Ldiv_loop_block2:
	# v8 = load.i64 v0
	mov	r13, qword ptr [rsi]
	# v9 = sdiv v8, v3
	mov	rax, r13
	test	r9, r9
	je	".Ldiv_loop_trap"
	cmp	r9, -1
	jne	".Ldiv_loop_inst5_divide"
	movabs	rdx, 9223372036854775808
	cmp	rax, rdx
	je	".Ldiv_loop_trap"
.Ldiv_loop_inst5_divide:
	cqo
	idiv	r9
.Ldiv_loop_inst5_div:
	mov	r13, rax
	# v10 = load.i64 v1
	# v11 = iadd v9, v10
	add	r13, qword ptr [rdi]
	# v12 = iadd v16, v11
	add	r12, r13
	# v13 = iconst.i64 1
	# v14 = iadd v15, v13
	add	rbx, 1
	# v19 = icmp.sgt v2, v14
	# brif v19, block2(v14, v12), block3(v12)
	cmp	r8, rbx
	jg	".Ldiv_loop_block2"
	mov	rsi, r12
.Ldiv_loop_block3:
	# return v18
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Ldiv_loop_trap:
	ud2
.Ldiv_loop_end:
	.cfi_endproc
	.size "div_loop", .-"div_loop"
	.section	".note.GNU-stack","",@progbits
