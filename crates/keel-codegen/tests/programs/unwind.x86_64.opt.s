	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_through"
	.p2align	4
	.type	"k_through",@function
"k_through":
.Lk_through_begin:
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
	movq	xmm14, rsi
	mov	rsi, rdi
	movq	rdi, xmm14
.Lk_through_block0:
	# v3 = iconst.i64 1
	# v4 = iadd v1, v3
	lea	rax, [rdi + 1]
	mov	qword ptr [rbp + -144], rax
	# v5 = iconst.i64 2
	# v6 = iadd v1, v5
	lea	rax, [rdi + 2]
	mov	qword ptr [rbp + -136], rax
	# v7 = iconst.i64 3
	# v8 = iadd v1, v7
	lea	rax, [rdi + 3]
	mov	qword ptr [rbp + -128], rax
	# v9 = iconst.i64 4
	# v10 = iadd v1, v9
	lea	rax, [rdi + 4]
	mov	qword ptr [rbp + -120], rax
	# v11 = iconst.i64 5
	# v12 = iadd v1, v11
	lea	rax, [rdi + 5]
	mov	qword ptr [rbp + -112], rax
	# v13 = iconst.i64 6
	# v14 = iadd v1, v13
	lea	rbx, [rdi + 6]
	# v15 = iconst.i64 7
	# v16 = iadd v1, v15
	lea	r12, [rdi + 7]
	# v17 = iconst.i64 8
	# v18 = iadd v1, v17
	lea	r13, [rdi + 8]
	# v19 = iconst.i64 9
	# v20 = iadd v1, v19
	lea	r14, [rdi + 9]
	# v21 = iconst.i64 10
	# v22 = iadd v1, v21
	lea	r15, [rdi + 10]
	# v23 = fconst.f64 1.5
	movabs	rax, 4609434218613702656
	movq	xmm1, rax
	# v24 = fadd v2, v23
	movaps	xmm13, xmm0
	addsd	xmm13, xmm1
	movsd	qword ptr [rbp + -104], xmm13
	# v25 = fconst.f64 2.5
	movabs	rax, 4612811918334230528
	movq	xmm1, rax
	# v26 = fadd v2, v25
	movaps	xmm13, xmm0
	addsd	xmm13, xmm1
	movsd	qword ptr [rbp + -96], xmm13
	# v27 = fconst.f64 3.5
	movabs	rax, 4615063718147915776
	movq	xmm1, rax
	# v28 = fadd v2, v27
	movaps	xmm13, xmm0
	addsd	xmm13, xmm1
	movsd	qword ptr [rbp + -88], xmm13
	# v29 = fconst.f64 4.5
	movabs	rax, 4616752568008179712
	movq	xmm1, rax
	# v30 = fadd v2, v29
	movaps	xmm13, xmm0
	addsd	xmm13, xmm1
	movsd	qword ptr [rbp + -80], xmm13
	# v31 = fconst.f64 5.5
	movabs	rax, 4617878467915022336
	movq	xmm1, rax
	# v32 = fadd v2, v31
	movaps	xmm13, xmm0
	addsd	xmm13, xmm1
	movsd	qword ptr [rbp + -72], xmm13
	# v33 = fconst.f64 6.5
	movabs	rax, 4619004367821864960
	movq	xmm1, rax
	# v34 = fadd v2, v33
	movaps	xmm13, xmm0
	addsd	xmm13, xmm1
	movsd	qword ptr [rbp + -64], xmm13
	# v35 = fconst.f64 7.5
	movabs	rax, 4620130267728707584
	movq	xmm1, rax
	# v36 = fadd v2, v35
	movaps	xmm13, xmm0
	addsd	xmm13, xmm1
	movsd	qword ptr [rbp + -56], xmm13
	# v37 = fconst.f64 8.5
	movabs	rax, 4620974692658839552
	movq	xmm1, rax
	# v38 = fadd v2, v37
	movaps	xmm13, xmm0
	addsd	xmm13, xmm1
	movsd	qword ptr [rbp + -48], xmm13
	# v39 = call_indirect v0(v1) : (i64) -> i64 cc(c)
	mov	r10, rsi
	call	r10
	mov	rsi, rax
	# v40 = iadd v4, v39
	mov	rax, qword ptr [rbp + -144]
	add	rsi, rax
	# v41 = iadd v6, v40
	mov	rax, qword ptr [rbp + -136]
	add	rsi, rax
	# v42 = iadd v8, v41
	mov	rax, qword ptr [rbp + -128]
	add	rsi, rax
	# v43 = iadd v10, v42
	mov	rax, qword ptr [rbp + -120]
	add	rsi, rax
	# v44 = iadd v12, v43
	mov	rax, qword ptr [rbp + -112]
	add	rsi, rax
	# v45 = iadd v14, v44
	add	rbx, rsi
	# v46 = iadd v16, v45
	add	r12, rbx
	# v47 = iadd v18, v46
	add	r13, r12
	# v48 = iadd v20, v47
	add	r14, r13
	# v49 = iadd v22, v48
	add	r15, r14
	# v50 = fadd v24, v26
	movsd	xmm13, qword ptr [rbp + -104]
	movsd	xmm14, qword ptr [rbp + -96]
	movaps	xmm0, xmm13
	addsd	xmm0, xmm14
	# v51 = fadd v50, v28
	movsd	xmm14, qword ptr [rbp + -88]
	addsd	xmm0, xmm14
	# v52 = fadd v51, v30
	movsd	xmm14, qword ptr [rbp + -80]
	addsd	xmm0, xmm14
	# v53 = fadd v52, v32
	movsd	xmm14, qword ptr [rbp + -72]
	addsd	xmm0, xmm14
	# v54 = fadd v53, v34
	movsd	xmm14, qword ptr [rbp + -64]
	addsd	xmm0, xmm14
	# v55 = fadd v54, v36
	movsd	xmm14, qword ptr [rbp + -56]
	addsd	xmm0, xmm14
	# v56 = fadd v55, v38
	movsd	xmm14, qword ptr [rbp + -48]
	addsd	xmm0, xmm14
	# v57 = fptosi.i64 v56
	cvttsd2si	rax, xmm0
	movabs	r11, 4890909195324358656
	movq	xmm14, r11
	ucomisd	xmm0, xmm14
	movabs	rcx, 9223372036854775807
	cmovae	rax, rcx
	mov	ecx, 0
	cmovp	rax, rcx
	mov	rsi, rax
	# v58 = iadd v49, v57
	add	r15, rsi
	# return v58
	mov	rax, r15
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_through_end:
	.cfi_endproc
	.size "k_through", .-"k_through"
	.section	".note.GNU-stack","",@progbits
