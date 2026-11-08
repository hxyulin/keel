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
	push	r14
	.cfi_offset 14, -48
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
	jbe	".Lk_isort_block3"
.Lk_isort_block2:
	# v5 = iconst.i64 2
	# v6 = shl v3, v5
	mov	r9, r8
	shl	r9, 2
	# v7 = ptradd v0, v6
	add	r9, rsi
	# v8 = load.i32 v7
	mov	r9d, dword ptr [r9]
	# jump block4(v3)
	mov	rbx, r8
.Lk_isort_block4:
	# v10 = iconst.i64 0
	# v11 = icmp.ne v9, v10
	# brif v11, block7, block6
	cmp	rbx, 0
	je	".Lk_isort_block6"
.Lk_isort_block7:
	# v21 = iconst.i64 1
	# v22 = isub v9, v21
	mov	r12, rbx
	sub	r12, 1
	# v23 = iconst.i64 2
	# v24 = shl v22, v23
	mov	r13, r12
	shl	r13, 2
	# v25 = ptradd v0, v24
	add	r13, rsi
	# v26 = load.i32 v25
	mov	r14d, dword ptr [r13]
	# v27 = icmp.slt v8, v26
	# brif v27, block5, block6
	cmp	r9d, r14d
	jge	".Lk_isort_block6"
.Lk_isort_block5:
	# v12 = iconst.i64 2
	# v13 = shl v9, v12
	shl	rbx, 2
	# v14 = ptradd v0, v13
	add	rbx, rsi
	# v15 = load.i32 v25
	mov	r13d, dword ptr [r13]
	# store v15, v14
	mov	dword ptr [rbx], r13d
	# jump block4(v22)
	mov	rbx, r12
	jmp	".Lk_isort_block4"
.Lk_isort_block6:
	# v16 = iconst.i64 2
	# v17 = shl v9, v16
	shl	rbx, 2
	# v18 = ptradd v0, v17
	add	rbx, rsi
	# store v8, v18
	mov	dword ptr [rbx], r9d
	# v19 = iconst.i64 1
	# v20 = iadd v3, v19
	add	r8, 1
	# jump block1(v20)
	jmp	".Lk_isort_block1"
.Lk_isort_block3:
	# return
	lea	rsp, [rbp + -32]
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
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
	sub	rsp, 24
	mov	qword ptr [rbp + -64], rsi
	mov	rsi, rdi
	mov	qword ptr [rbp + -56], rdx
	mov	r9, rcx
.Lk_matmul_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# jump block1(v4)
	mov	qword ptr [rbp + -48], rbx
.Lk_matmul_block1:
	# v6 = icmp.sgt v3, v5
	# brif v6, block2, block4
	mov	rcx, qword ptr [rbp + -48]
	cmp	r9d, ecx
	jle	".Lk_matmul_block4"
.Lk_matmul_block2:
	# v7 = imul v3, v5
	mov	rcx, qword ptr [rbp + -48]
	mov	r12, r9
	imul	r12d, ecx
	# v8 = iconst.i32 0
	xor	r13d, r13d
	# jump block5(v8)
.Lk_matmul_block5:
	# v12 = icmp.sgt v3, v11
	# brif v12, block6, block3
	cmp	r9d, r13d
	jle	".Lk_matmul_block3"
.Lk_matmul_block6:
	# v13 = iconst.i32 0
	xor	r14d, r14d
	# jump block7(v13, v13)
	mov	r15, r14
.Lk_matmul_block7:
	# v16 = icmp.sgt v3, v14
	# brif v16, block8, block9
	cmp	r9d, r14d
	jle	".Lk_matmul_block9"
.Lk_matmul_block8:
	# v17 = iadd v14, v7
	lea	ebx, [r14 + r12 * 1]
	# v18 = sext.i64 v17
	movsxd	rbx, ebx
	# v19 = iconst.i64 2
	# v20 = shl v18, v19
	shl	rbx, 2
	# v21 = ptradd v0, v20
	add	rbx, rsi
	# v22 = load.i32 v21
	mov	ebx, dword ptr [rbx]
	# v23 = imul v3, v14
	mov	rdi, r9
	imul	edi, r14d
	# v24 = iadd v11, v23
	add	edi, r13d
	# v25 = sext.i64 v24
	movsxd	rdi, edi
	# v26 = shl v25, v19
	shl	rdi, 2
	# v27 = ptradd v1, v26
	mov	rax, qword ptr [rbp + -64]
	add	rdi, rax
	# v28 = load.i32 v27
	mov	edi, dword ptr [rdi]
	# v29 = imul v22, v28
	imul	edi, ebx
	# v30 = iadd v15, v29
	add	edi, r15d
	# v31 = iconst.i32 1
	# v32 = iadd v14, v31
	lea	r8d, [r14 + 1]
	# jump block7(v32, v30)
	mov	r14, r8
	mov	r15, rdi
	jmp	".Lk_matmul_block7"
.Lk_matmul_block9:
	# v33 = iadd v11, v7
	lea	edi, [r13 + r12 * 1]
	# v34 = sext.i64 v33
	movsxd	rdi, edi
	# v35 = iconst.i64 2
	# v36 = shl v34, v35
	shl	rdi, 2
	# v37 = ptradd v2, v36
	mov	rax, qword ptr [rbp + -56]
	add	rdi, rax
	# store v15, v37
	mov	dword ptr [rdi], r15d
	# v38 = iconst.i32 1
	# v39 = iadd v11, v38
	lea	edi, [r13 + 1]
	# jump block5(v39)
	mov	r13, rdi
	jmp	".Lk_matmul_block5"
.Lk_matmul_block3:
	# v9 = iconst.i32 1
	# v10 = iadd v5, v9
	mov	rax, qword ptr [rbp + -48]
	lea	edi, [rax + 1]
	# jump block1(v10)
	mov	qword ptr [rbp + -48], rdi
	jmp	".Lk_matmul_block1"
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
	sub	rsp, 8
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsigned_dot32_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# jump block1(v4, v3)
	mov	r11, r9
	mov	r9, rbx
	mov	rbx, r11
.Lsigned_dot32_block1:
	# v7 = icmp.slt v5, v2
	# brif v7, block2, block3
	cmp	r9d, r8d
	jge	".Lsigned_dot32_block3"
.Lsigned_dot32_block2:
	# v8 = sext.i64 v5
	movsxd	r12, r9d
	# v9 = iconst.i64 2
	# v10 = shl v8, v9
	shl	r12, 2
	# v11 = ptradd v0, v10
	lea	r13, [rsi + r12 * 1]
	# v12 = load.i32 v11
	mov	r13d, dword ptr [r13]
	# v13 = ptradd v1, v10
	add	r12, rdi
	# v14 = load.i32 v13
	mov	r12d, dword ptr [r12]
	# v15 = imul v12, v14
	imul	r12d, r13d
	# v16 = iadd v6, v15
	add	ebx, r12d
	# v17 = iconst.i32 1
	# v18 = iadd v5, v17
	add	r9d, 1
	# jump block1(v18, v16)
	jmp	".Lsigned_dot32_block1"
.Lsigned_dot32_block3:
	# return v6
	mov	rax, rbx
	lea	rsp, [rbp + -24]
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
	sub	rsp, 8
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsigned_dot64_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# jump block1(v4, v3)
	mov	r11, r9
	mov	r9, rbx
	mov	rbx, r11
.Lsigned_dot64_block1:
	# v7 = icmp.slt v5, v2
	# brif v7, block2, block3
	cmp	r9, r8
	jge	".Lsigned_dot64_block3"
.Lsigned_dot64_block2:
	# v8 = iconst.i64 3
	# v9 = shl v5, v8
	mov	r12, r9
	shl	r12, 3
	# v10 = ptradd v0, v9
	lea	r13, [rsi + r12 * 1]
	# v11 = load.i64 v10
	mov	r13, qword ptr [r13]
	# v12 = ptradd v1, v9
	add	r12, rdi
	# v13 = load.i64 v12
	mov	r12, qword ptr [r12]
	# v14 = imul v11, v13
	imul	r12, r13
	# v15 = iadd v6, v14
	add	rbx, r12
	# v16 = iconst.i64 1
	# v17 = iadd v5, v16
	add	r9, 1
	# jump block1(v17, v15)
	jmp	".Lsigned_dot64_block1"
.Lsigned_dot64_block3:
	# return v6
	mov	rax, rbx
	lea	rsp, [rbp + -24]
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
	push	r14
	.cfi_offset 14, -48
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ldiv_loop_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# jump block1(v4, v4)
	mov	r12, rbx
.Ldiv_loop_block1:
	# v7 = icmp.slt v5, v2
	# brif v7, block2, block3
	cmp	rbx, r8
	jge	".Ldiv_loop_block3"
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
	mov	r14, qword ptr [rdi]
	# v11 = iadd v9, v10
	add	r13, r14
	# v12 = iadd v6, v11
	add	r12, r13
	# v13 = iconst.i64 1
	# v14 = iadd v5, v13
	add	rbx, 1
	# jump block1(v14, v12)
	jmp	".Ldiv_loop_block1"
.Ldiv_loop_block3:
	# return v6
	mov	rax, r12
	lea	rsp, [rbp + -32]
	pop	r14
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
