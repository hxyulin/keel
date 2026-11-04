	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "poly"
	.p2align	4
	.type	"poly",@function
"poly":
.Lpoly_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lpoly_block0:
	# v1 = fconst.f64 3.0
	movabs	rax, 4613937818241073152
	movq	xmm1, rax
	# v2 = fmul v0, v0
	movaps	xmm2, xmm0
	mulsd	xmm2, xmm0
	# v3 = fmul v1, v2
	mulsd	xmm1, xmm2
	# v4 = fconst.f64 2.0
	movabs	rax, 4611686018427387904
	movq	xmm2, rax
	# v5 = fmul v4, v0
	movaps	xmm13, xmm2
	mulsd	xmm13, xmm0
	movaps	xmm0, xmm13
	# v6 = fsub v3, v5
	movaps	xmm13, xmm1
	subsd	xmm13, xmm0
	movaps	xmm0, xmm13
	# v7 = fconst.f64 0.5
	movabs	rax, 4602678819172646912
	movq	xmm1, rax
	# v8 = fadd v6, v7
	addsd	xmm0, xmm1
	# return v8
	mov	rsp, rbp
	pop	rbp
	ret
.Lpoly_end:
	.cfi_endproc
	.size "poly", .-"poly"
	.section	".text","ax",@progbits
	.globl "poly32"
	.p2align	4
	.type	"poly32",@function
"poly32":
.Lpoly32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lpoly32_block0:
	# v1 = fconst.f32 3.0
	movabs	rax, 1077936128
	movq	xmm1, rax
	# v2 = fmul v0, v0
	movaps	xmm2, xmm0
	mulss	xmm2, xmm0
	# v3 = fmul v1, v2
	mulss	xmm1, xmm2
	# v4 = fconst.f32 2.0
	movabs	rax, 1073741824
	movq	xmm2, rax
	# v5 = fmul v4, v0
	movaps	xmm13, xmm2
	mulss	xmm13, xmm0
	movaps	xmm0, xmm13
	# v6 = fsub v3, v5
	movaps	xmm13, xmm1
	subss	xmm13, xmm0
	movaps	xmm0, xmm13
	# v7 = fconst.f32 0.5
	movabs	rax, 1056964608
	movq	xmm1, rax
	# v8 = fadd v6, v7
	addss	xmm0, xmm1
	# return v8
	mov	rsp, rbp
	pop	rbp
	ret
.Lpoly32_end:
	.cfi_endproc
	.size "poly32", .-"poly32"
	.section	".text","ax",@progbits
	.globl "mixed"
	.p2align	4
	.type	"mixed",@function
"mixed":
.Lmixed_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	movq	xmm14, rsi
	mov	rsi, rdi
	movq	rdi, xmm14
.Lmixed_block0:
	# v5 = sitofp.f64 v0
	xorps	xmm2, xmm2
	cvtsi2sd	xmm2, rsi
	# v6 = fconst.f64 2.0
	movabs	rax, 4611686018427387904
	movq	xmm3, rax
	# v7 = fmul v1, v6
	mulsd	xmm0, xmm3
	# v8 = fadd v5, v7
	movaps	xmm13, xmm2
	addsd	xmm13, xmm0
	movaps	xmm0, xmm13
	# v9 = sitofp.f64 v2
	xorps	xmm2, xmm2
	cvtsi2sd	xmm2, edi
	# v10 = fconst.f64 3.0
	movabs	rax, 4613937818241073152
	movq	xmm3, rax
	# v11 = fmul v9, v10
	mulsd	xmm2, xmm3
	# v12 = fadd v8, v11
	addsd	xmm0, xmm2
	# v13 = fpext.f64 v3
	cvtss2sd	xmm1, xmm1
	# v14 = fconst.f64 4.0
	movabs	rax, 4616189618054758400
	movq	xmm2, rax
	# v15 = fmul v13, v14
	mulsd	xmm1, xmm2
	# v16 = fadd v12, v15
	addsd	xmm0, xmm1
	# v17 = load.f64 v4
	movsd	xmm1, qword ptr [r8]
	# v18 = fadd v16, v17
	addsd	xmm0, xmm1
	# return v18
	mov	rsp, rbp
	pop	rbp
	ret
.Lmixed_end:
	.cfi_endproc
	.size "mixed", .-"mixed"
	.section	".text","ax",@progbits
	.globl "call_c"
	.p2align	4
	.type	"call_c",@function
"call_c":
.Lcall_c_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 32
	movsd	qword ptr [rbp + -32], xmm0
.Lcall_c_block0:
	# v1 = fconst.f64 10.0
	movabs	rax, 4621819117588971520
	movq	xmm0, rax
	# v2 = fmul v0, v1
	movsd	xmm13, qword ptr [rbp + -32]
	mulsd	xmm13, xmm0
	movsd	qword ptr [rbp + -24], xmm13
	# call @fclobber()
	call	"fclobber"@PLT
	# v3 = iconst.i64 7
	mov	esi, 7
	# v4 = fconst.f32 1.5
	movabs	rax, 1069547520
	movq	xmm0, rax
	# v5 = iconst.i32 -2
	mov	edi, 4294967294
	# v6 = call @c_mix(v3, v0, v4, v5)
	movaps	xmm1, xmm0
	movsd	xmm0, qword ptr [rbp + -32]
	movq	xmm14, rdi
	mov	rdi, rsi
	movq	rsi, xmm14
	call	"c_mix"@PLT
	movsd	qword ptr [rbp + -16], xmm0
	# call @fclobber()
	call	"fclobber"@PLT
	# v7 = fadd v0, v6
	movsd	xmm13, qword ptr [rbp + -32]
	movsd	xmm14, qword ptr [rbp + -16]
	movaps	xmm0, xmm13
	addsd	xmm0, xmm14
	# v8 = fadd v7, v2
	movsd	xmm14, qword ptr [rbp + -24]
	addsd	xmm0, xmm14
	# return v8
	mov	rsp, rbp
	pop	rbp
	ret
.Lcall_c_end:
	.cfi_endproc
	.size "call_c", .-"call_c"
	.section	".text","ax",@progbits
	.globl "unary"
	.p2align	4
	.type	"unary",@function
"unary":
.Lunary_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lunary_block0:
	# v2 = iconst.i64 8
	# v3 = fsqrt v0
	sqrtsd	xmm1, xmm0
	# store v3, v1
	movsd	qword ptr [rsi], xmm1
	# v4 = ptradd v1, v2
	add	rsi, 8
	# v5 = fceil v0
	movaps	xmm14, xmm0
	movq	rax, xmm14
	btr	rax, 63
	movq	xmm15, rax
	mov	rcx, rax
	shr	rcx, 52
	cmp	rcx, 2047
	je	".Lunary_inst4_round_nan"
	cmp	rcx, 1075
	jae	".Lunary_inst4_round_sign"
	cmp	rcx, 1023
	jb	".Lunary_inst4_round_small"
	neg	rcx
	add	rcx, 1075
	mov	edx, 1
	shl	rdx, cl
	sub	rdx, 1
	and	rax, rdx
	add	rdx, 1
	test	rax, rax
	je	".Lunary_inst4_round_truncate"
	movq	rax, xmm14
	bt	rax, 63
	jae	".Lunary_inst4_round_increment"
	jmp	".Lunary_inst4_round_truncate"
.Lunary_inst4_round_increment:
	movq	rax, xmm15
	add	rax, rdx
	movq	xmm15, rax
.Lunary_inst4_round_truncate:
	movq	rax, xmm15
	neg	rdx
	and	rax, rdx
	jmp	".Lunary_inst4_round_sign"
.Lunary_inst4_round_small:
	test	rcx, rcx
	jne	".Lunary_inst4_round_normal_small"
	stmxcsr	dword ptr [rbp + -16]
	mov	ecx, dword ptr [rbp + -16]
	test	ecx, 64
	je	".Lunary_inst4_round_normal_small"
	xor	eax, eax
	jmp	".Lunary_inst4_round_sign"
.Lunary_inst4_round_normal_small:
	mov	rcx, rax
	xor	eax, eax
	test	rcx, rcx
	je	".Lunary_inst4_round_small_done"
	movq	rcx, xmm14
	bt	rcx, 63
	jb	".Lunary_inst4_round_small_done"
	movabs	rax, 4607182418800017408
.Lunary_inst4_round_small_done:
.Lunary_inst4_round_sign:
	movq	rcx, xmm14
	shr	rcx, 63
	shl	rcx, 63
	or	rax, rcx
	movq	xmm1, rax
	jmp	".Lunary_inst4_round_done"
.Lunary_inst4_round_nan:
	movaps	xmm13, xmm14
	addsd	xmm13, xmm14
	movaps	xmm1, xmm13
.Lunary_inst4_round_done:
	# store v5, v4
	movsd	qword ptr [rsi], xmm1
	# v6 = ptradd v4, v2
	add	rsi, 8
	# v7 = ffloor v0
	movaps	xmm14, xmm0
	movq	rax, xmm14
	btr	rax, 63
	movq	xmm15, rax
	mov	rcx, rax
	shr	rcx, 52
	cmp	rcx, 2047
	je	".Lunary_inst7_round_nan"
	cmp	rcx, 1075
	jae	".Lunary_inst7_round_sign"
	cmp	rcx, 1023
	jb	".Lunary_inst7_round_small"
	neg	rcx
	add	rcx, 1075
	mov	edx, 1
	shl	rdx, cl
	sub	rdx, 1
	and	rax, rdx
	add	rdx, 1
	test	rax, rax
	je	".Lunary_inst7_round_truncate"
	movq	rax, xmm14
	bt	rax, 63
	jb	".Lunary_inst7_round_increment"
	jmp	".Lunary_inst7_round_truncate"
.Lunary_inst7_round_increment:
	movq	rax, xmm15
	add	rax, rdx
	movq	xmm15, rax
.Lunary_inst7_round_truncate:
	movq	rax, xmm15
	neg	rdx
	and	rax, rdx
	jmp	".Lunary_inst7_round_sign"
.Lunary_inst7_round_small:
	test	rcx, rcx
	jne	".Lunary_inst7_round_normal_small"
	stmxcsr	dword ptr [rbp + -16]
	mov	ecx, dword ptr [rbp + -16]
	test	ecx, 64
	je	".Lunary_inst7_round_normal_small"
	xor	eax, eax
	jmp	".Lunary_inst7_round_sign"
.Lunary_inst7_round_normal_small:
	mov	rcx, rax
	xor	eax, eax
	test	rcx, rcx
	je	".Lunary_inst7_round_small_done"
	movq	rcx, xmm14
	bt	rcx, 63
	jae	".Lunary_inst7_round_small_done"
	movabs	rax, 4607182418800017408
.Lunary_inst7_round_small_done:
.Lunary_inst7_round_sign:
	movq	rcx, xmm14
	shr	rcx, 63
	shl	rcx, 63
	or	rax, rcx
	movq	xmm1, rax
	jmp	".Lunary_inst7_round_done"
.Lunary_inst7_round_nan:
	movaps	xmm13, xmm14
	addsd	xmm13, xmm14
	movaps	xmm1, xmm13
.Lunary_inst7_round_done:
	# store v7, v6
	movsd	qword ptr [rsi], xmm1
	# v8 = ptradd v6, v2
	add	rsi, 8
	# v9 = ftrunc v0
	movaps	xmm14, xmm0
	movq	rax, xmm14
	btr	rax, 63
	movq	xmm15, rax
	mov	rcx, rax
	shr	rcx, 52
	cmp	rcx, 2047
	je	".Lunary_inst10_round_nan"
	cmp	rcx, 1075
	jae	".Lunary_inst10_round_sign"
	cmp	rcx, 1023
	jb	".Lunary_inst10_round_small"
	neg	rcx
	add	rcx, 1075
	mov	edx, 1
	shl	rdx, cl
	sub	rdx, 1
	and	rax, rdx
	add	rdx, 1
	test	rax, rax
	je	".Lunary_inst10_round_truncate"
	jmp	".Lunary_inst10_round_truncate"
.Lunary_inst10_round_increment:
	movq	rax, xmm15
	add	rax, rdx
	movq	xmm15, rax
.Lunary_inst10_round_truncate:
	movq	rax, xmm15
	neg	rdx
	and	rax, rdx
	jmp	".Lunary_inst10_round_sign"
.Lunary_inst10_round_small:
	test	rcx, rcx
	jne	".Lunary_inst10_round_normal_small"
	stmxcsr	dword ptr [rbp + -16]
	mov	ecx, dword ptr [rbp + -16]
	test	ecx, 64
	je	".Lunary_inst10_round_normal_small"
	xor	eax, eax
	jmp	".Lunary_inst10_round_sign"
.Lunary_inst10_round_normal_small:
	mov	rcx, rax
	xor	eax, eax
	jmp	".Lunary_inst10_round_small_done"
	movabs	rax, 4607182418800017408
.Lunary_inst10_round_small_done:
.Lunary_inst10_round_sign:
	movq	rcx, xmm14
	shr	rcx, 63
	shl	rcx, 63
	or	rax, rcx
	movq	xmm1, rax
	jmp	".Lunary_inst10_round_done"
.Lunary_inst10_round_nan:
	movaps	xmm13, xmm14
	addsd	xmm13, xmm14
	movaps	xmm1, xmm13
.Lunary_inst10_round_done:
	# store v9, v8
	movsd	qword ptr [rsi], xmm1
	# v10 = ptradd v8, v2
	add	rsi, 8
	# v11 = fnearest v0
	movaps	xmm14, xmm0
	movq	rax, xmm14
	btr	rax, 63
	movq	xmm15, rax
	mov	rcx, rax
	shr	rcx, 52
	cmp	rcx, 2047
	je	".Lunary_inst13_round_nan"
	cmp	rcx, 1075
	jae	".Lunary_inst13_round_sign"
	cmp	rcx, 1023
	jb	".Lunary_inst13_round_small"
	neg	rcx
	add	rcx, 1075
	mov	edx, 1
	shl	rdx, cl
	sub	rdx, 1
	and	rax, rdx
	add	rdx, 1
	test	rax, rax
	je	".Lunary_inst13_round_truncate"
	mov	rcx, rdx
	shr	rcx, 1
	cmp	rax, rcx
	ja	".Lunary_inst13_round_increment"
	jb	".Lunary_inst13_round_truncate"
	movq	rax, xmm15
	test	rax, rdx
	jne	".Lunary_inst13_round_increment"
	jmp	".Lunary_inst13_round_truncate"
.Lunary_inst13_round_increment:
	movq	rax, xmm15
	add	rax, rdx
	movq	xmm15, rax
.Lunary_inst13_round_truncate:
	movq	rax, xmm15
	neg	rdx
	and	rax, rdx
	jmp	".Lunary_inst13_round_sign"
.Lunary_inst13_round_small:
	test	rcx, rcx
	jne	".Lunary_inst13_round_normal_small"
	stmxcsr	dword ptr [rbp + -16]
	mov	ecx, dword ptr [rbp + -16]
	test	ecx, 64
	je	".Lunary_inst13_round_normal_small"
	xor	eax, eax
	jmp	".Lunary_inst13_round_sign"
.Lunary_inst13_round_normal_small:
	mov	rcx, rax
	xor	eax, eax
	movabs	rdx, 4602678819172646912
	cmp	rcx, rdx
	jbe	".Lunary_inst13_round_small_done"
	movabs	rax, 4607182418800017408
.Lunary_inst13_round_small_done:
.Lunary_inst13_round_sign:
	movq	rcx, xmm14
	shr	rcx, 63
	shl	rcx, 63
	or	rax, rcx
	movq	xmm1, rax
	jmp	".Lunary_inst13_round_done"
.Lunary_inst13_round_nan:
	movaps	xmm13, xmm14
	addsd	xmm13, xmm14
	movaps	xmm1, xmm13
.Lunary_inst13_round_done:
	# store v11, v10
	movsd	qword ptr [rsi], xmm1
	# v12 = ptradd v10, v2
	add	rsi, 8
	# v13 = fneg v0
	movq	rax, xmm0
	btc	rax, 63
	movq	xmm1, rax
	# store v13, v12
	movsd	qword ptr [rsi], xmm1
	# v14 = ptradd v12, v2
	add	rsi, 8
	# v15 = fabs v0
	movq	rax, xmm0
	btr	rax, 63
	movq	xmm0, rax
	# store v15, v14
	movsd	qword ptr [rsi], xmm0
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lunary_end:
	.cfi_endproc
	.size "unary", .-"unary"
	.section	".text","ax",@progbits
	.globl "unary32"
	.p2align	4
	.type	"unary32",@function
"unary32":
.Lunary32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lunary32_block0:
	# v3 = iconst.i64 4
	# v4 = fsqrt v0
	sqrtss	xmm2, xmm0
	# store v4, v2
	movss	dword ptr [rsi], xmm2
	# v5 = ptradd v2, v3
	add	rsi, 4
	# v6 = fnearest v0
	movaps	xmm14, xmm0
	movd	eax, xmm14
	btr	rax, 31
	movd	xmm15, eax
	mov	rcx, rax
	shr	rcx, 23
	cmp	rcx, 255
	je	".Lunary32_inst4_round_nan"
	cmp	rcx, 150
	jae	".Lunary32_inst4_round_sign"
	cmp	rcx, 127
	jb	".Lunary32_inst4_round_small"
	neg	rcx
	add	rcx, 150
	mov	edx, 1
	shl	rdx, cl
	sub	rdx, 1
	and	rax, rdx
	add	rdx, 1
	test	rax, rax
	je	".Lunary32_inst4_round_truncate"
	mov	rcx, rdx
	shr	rcx, 1
	cmp	rax, rcx
	ja	".Lunary32_inst4_round_increment"
	jb	".Lunary32_inst4_round_truncate"
	movd	eax, xmm15
	test	rax, rdx
	jne	".Lunary32_inst4_round_increment"
	jmp	".Lunary32_inst4_round_truncate"
.Lunary32_inst4_round_increment:
	movd	eax, xmm15
	add	rax, rdx
	movd	xmm15, eax
.Lunary32_inst4_round_truncate:
	movd	eax, xmm15
	neg	rdx
	and	rax, rdx
	jmp	".Lunary32_inst4_round_sign"
.Lunary32_inst4_round_small:
	test	rcx, rcx
	jne	".Lunary32_inst4_round_normal_small"
	stmxcsr	dword ptr [rbp + -16]
	mov	ecx, dword ptr [rbp + -16]
	test	ecx, 64
	je	".Lunary32_inst4_round_normal_small"
	xor	eax, eax
	jmp	".Lunary32_inst4_round_sign"
.Lunary32_inst4_round_normal_small:
	mov	rcx, rax
	xor	eax, eax
	movabs	rdx, 1056964608
	cmp	rcx, rdx
	jbe	".Lunary32_inst4_round_small_done"
	movabs	rax, 1065353216
.Lunary32_inst4_round_small_done:
.Lunary32_inst4_round_sign:
	movd	ecx, xmm14
	shr	rcx, 31
	shl	rcx, 31
	or	rax, rcx
	movd	xmm2, eax
	jmp	".Lunary32_inst4_round_done"
.Lunary32_inst4_round_nan:
	movaps	xmm13, xmm14
	addss	xmm13, xmm14
	movaps	xmm2, xmm13
.Lunary32_inst4_round_done:
	# store v6, v5
	movss	dword ptr [rsi], xmm2
	# v7 = ptradd v5, v3
	add	rsi, 4
	# v8 = fneg v0
	movd	eax, xmm0
	btc	eax, 31
	movd	xmm2, eax
	# store v8, v7
	movss	dword ptr [rsi], xmm2
	# v9 = ptradd v7, v3
	add	rsi, 4
	# v10 = fdiv v0, v1
	divss	xmm0, xmm1
	# store v10, v9
	movss	dword ptr [rsi], xmm0
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lunary32_end:
	.cfi_endproc
	.size "unary32", .-"unary32"
	.section	".text","ax",@progbits
	.globl "minmax"
	.p2align	4
	.type	"minmax",@function
"minmax":
.Lminmax_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lminmax_block0:
	# v3 = fmin v0, v1
	movaps	xmm2, xmm0
	ucomisd	xmm2, xmm1
	jp	".Lminmax_inst0_nan"
	jne	".Lminmax_inst0_ne"
	orpd	xmm2, xmm1
	jmp	".Lminmax_inst0_done"
.Lminmax_inst0_ne:
	minsd	xmm2, xmm1
	jmp	".Lminmax_inst0_done"
.Lminmax_inst0_nan:
	addsd	xmm2, xmm1
.Lminmax_inst0_done:
	# store v3, v2
	movsd	qword ptr [rsi], xmm2
	# v4 = iconst.i64 8
	# v5 = ptradd v2, v4
	add	rsi, 8
	# v6 = fmax v0, v1
	ucomisd	xmm0, xmm1
	jp	".Lminmax_inst4_nan"
	jne	".Lminmax_inst4_ne"
	andpd	xmm0, xmm1
	jmp	".Lminmax_inst4_done"
.Lminmax_inst4_ne:
	maxsd	xmm0, xmm1
	jmp	".Lminmax_inst4_done"
.Lminmax_inst4_nan:
	addsd	xmm0, xmm1
.Lminmax_inst4_done:
	# store v6, v5
	movsd	qword ptr [rsi], xmm0
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lminmax_end:
	.cfi_endproc
	.size "minmax", .-"minmax"
	.section	".text","ax",@progbits
	.globl "compare"
	.p2align	4
	.type	"compare",@function
"compare":
.Lcompare_begin:
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
.Lcompare_block0:
	# v2 = fcmp.eq v0, v1
	ucomisd	xmm0, xmm1
	sete	al
	setnp	cl
	and	al, cl
	mov	sil, al
	# v3 = fcmp.ne v0, v1
	ucomisd	xmm0, xmm1
	setne	al
	setp	cl
	or	al, cl
	mov	dil, al
	# v4 = fcmp.lt v0, v1
	ucomisd	xmm1, xmm0
	seta	r8b
	# v5 = fcmp.le v0, v1
	ucomisd	xmm1, xmm0
	setae	r9b
	# v6 = fcmp.gt v0, v1
	ucomisd	xmm0, xmm1
	seta	bl
	# v7 = fcmp.ge v0, v1
	ucomisd	xmm0, xmm1
	setae	r12b
	# v8 = fcmp.ord v0, v1
	ucomisd	xmm0, xmm1
	setnp	r13b
	# v9 = fcmp.uno v0, v1
	ucomisd	xmm0, xmm1
	setp	r14b
	# v10 = uext.i32 v2
	movzx	eax, sil
	mov	qword ptr [rbp + -96], rax
	# v11 = uext.i32 v3
	movzx	edi, dil
	# v12 = uext.i32 v4
	movzx	r8d, r8b
	# v13 = uext.i32 v5
	movzx	eax, r9b
	mov	qword ptr [rbp + -88], rax
	# v14 = uext.i32 v6
	movzx	eax, bl
	mov	qword ptr [rbp + -80], rax
	# v15 = uext.i32 v7
	movzx	eax, r12b
	mov	qword ptr [rbp + -72], rax
	# v16 = uext.i32 v8
	movzx	eax, r13b
	mov	qword ptr [rbp + -64], rax
	# v17 = uext.i32 v9
	movzx	eax, r14b
	mov	qword ptr [rbp + -56], rax
	# v18 = iconst.i32 1
	# v19 = iconst.i32 2
	# v20 = iconst.i32 3
	# v21 = iconst.i32 4
	# v22 = iconst.i32 5
	# v23 = iconst.i32 6
	# v24 = iconst.i32 7
	# v25 = shl v11, v18
	shl	edi, 1
	# v26 = shl v12, v19
	mov	rsi, r8
	shl	esi, 2
	# v27 = shl v13, v20
	mov	rax, qword ptr [rbp + -88]
	mov	r8, rax
	shl	r8d, 3
	# v28 = shl v14, v21
	mov	rax, qword ptr [rbp + -80]
	mov	r13, rax
	shl	r13d, 4
	# v29 = shl v15, v22
	mov	rax, qword ptr [rbp + -72]
	mov	r12, rax
	shl	r12d, 5
	# v30 = shl v16, v23
	mov	rax, qword ptr [rbp + -64]
	mov	rbx, rax
	shl	ebx, 6
	# v31 = shl v17, v24
	mov	rax, qword ptr [rbp + -56]
	mov	r9, rax
	shl	r9d, 7
	# v32 = or v10, v25
	mov	rax, qword ptr [rbp + -96]
	or	edi, eax
	# v33 = or v32, v26
	or	esi, edi
	# v34 = or v33, v27
	or	esi, r8d
	# v35 = or v34, v28
	or	esi, r13d
	# v36 = or v35, v29
	or	esi, r12d
	# v37 = or v36, v30
	or	esi, ebx
	# v38 = or v37, v31
	or	esi, r9d
	# return v38
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lcompare_end:
	.cfi_endproc
	.size "compare", .-"compare"
	.section	".text","ax",@progbits
	.globl "compare32"
	.p2align	4
	.type	"compare32",@function
"compare32":
.Lcompare32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lcompare32_block0:
	# v2 = fcmp.lt v0, v1
	ucomiss	xmm1, xmm0
	seta	sil
	# v3 = fcmp.uno v0, v1
	ucomiss	xmm0, xmm1
	setp	dil
	# v4 = uext.i32 v2
	movzx	esi, sil
	# v5 = uext.i32 v3
	movzx	edi, dil
	# v6 = iconst.i32 1
	# v7 = shl v5, v6
	shl	edi, 1
	# v8 = or v4, v7
	or	esi, edi
	# return v8
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lcompare32_end:
	.cfi_endproc
	.size "compare32", .-"compare32"
	.section	".text","ax",@progbits
	.globl "to_int"
	.p2align	4
	.type	"to_int",@function
"to_int":
.Lto_int_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lto_int_block0:
	# v2 = iconst.i64 8
	# v3 = fptosi.i32 v0
	cvttsd2si	eax, xmm0
	movabs	r11, 4746794007248502784
	movq	xmm14, r11
	ucomisd	xmm0, xmm14
	movabs	rcx, 2147483647
	cmovae	eax, ecx
	mov	ecx, 0
	cmovp	eax, ecx
	mov	r8, rax
	# v4 = sext.i64 v3
	movsxd	r8, r8d
	# store v4, v1
	mov	qword ptr [rsi], r8
	# v5 = ptradd v1, v2
	add	rsi, 8
	# v6 = fptosi.i64 v0
	cvttsd2si	rax, xmm0
	movabs	r11, 4890909195324358656
	movq	xmm14, r11
	ucomisd	xmm0, xmm14
	movabs	rcx, 9223372036854775807
	cmovae	rax, rcx
	mov	ecx, 0
	cmovp	rax, rcx
	mov	r8, rax
	# store v6, v5
	mov	qword ptr [rsi], r8
	# v7 = ptradd v5, v2
	add	rsi, 8
	# v8 = fptoui.i32 v0
	cvttsd2si	rax, xmm0
	mov	ecx, 0
	test	rax, rax
	cmovs	rax, rcx
	mov	ecx, 4294967295
	cmp	rax, rcx
	cmova	rax, rcx
	movabs	r11, 4751297606875873280
	movq	xmm14, r11
	ucomisd	xmm0, xmm14
	cmovae	rax, rcx
	mov	r8, rax
	# v9 = uext.i64 v8
	mov	r8d, r8d
	# store v9, v7
	mov	qword ptr [rsi], r8
	# v10 = ptradd v7, v2
	add	rsi, 8
	# v11 = fptoui.i64 v0
	movabs	r11, 4890909195324358656
	movq	xmm14, r11
	ucomisd	xmm0, xmm14
	jae	".Lto_int_inst12_big"
	cvttsd2si	rax, xmm0
	mov	ecx, 0
	test	rax, rax
	cmovs	rax, rcx
	jmp	".Lto_int_inst12_done"
.Lto_int_inst12_big:
	movaps	xmm15, xmm0
	subsd	xmm15, xmm14
	cvttsd2si	rax, xmm15
	test	rax, rax
	js	".Lto_int_inst12_max"
	btc	rax, 63
	jmp	".Lto_int_inst12_done"
.Lto_int_inst12_max:
	mov	rax, -1
.Lto_int_inst12_done:
	mov	rdi, rax
	# store v11, v10
	mov	qword ptr [rsi], rdi
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lto_int_end:
	.cfi_endproc
	.size "to_int", .-"to_int"
	.section	".text","ax",@progbits
	.globl "to_int32"
	.p2align	4
	.type	"to_int32",@function
"to_int32":
.Lto_int32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lto_int32_block0:
	# v1 = fptosi.i32 v0
	cvttss2si	eax, xmm0
	movabs	r11, 1325400064
	movq	xmm14, r11
	ucomiss	xmm0, xmm14
	movabs	rcx, 2147483647
	cmovae	eax, ecx
	mov	ecx, 0
	cmovp	eax, ecx
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lto_int32_end:
	.cfi_endproc
	.size "to_int32", .-"to_int32"
	.section	".text","ax",@progbits
	.globl "from_int"
	.p2align	4
	.type	"from_int",@function
"from_int":
.Lfrom_int_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lfrom_int_block0:
	# v3 = iconst.i64 8
	# v4 = sitofp.f64 v0
	xorps	xmm0, xmm0
	movsx	eax, sil
	cvtsi2sd	xmm0, eax
	# store v4, v2
	movsd	qword ptr [r8], xmm0
	# v5 = ptradd v2, v3
	add	r8, 8
	# v6 = uitofp.f64 v0
	xorps	xmm0, xmm0
	movzx	eax, sil
	mov	eax, eax
	cvtsi2sd	xmm0, rax
	# store v6, v5
	movsd	qword ptr [r8], xmm0
	# v7 = ptradd v5, v3
	lea	rsi, [r8 + 8]
	# v8 = sitofp.f64 v1
	xorps	xmm0, xmm0
	cvtsi2sd	xmm0, rdi
	# store v8, v7
	movsd	qword ptr [rsi], xmm0
	# v9 = ptradd v7, v3
	add	rsi, 8
	# v10 = uitofp.f64 v1
	xorps	xmm0, xmm0
	mov	rax, rdi
	test	rax, rax
	js	".Lfrom_int_inst10_big"
	cvtsi2sd	xmm0, rax
	jmp	".Lfrom_int_inst10_done"
.Lfrom_int_inst10_big:
	mov	rcx, rax
	shr	rcx, 1
	and	eax, 1
	or	rcx, rax
	cvtsi2sd	xmm0, rcx
	addsd	xmm0, xmm0
.Lfrom_int_inst10_done:
	# store v10, v9
	movsd	qword ptr [rsi], xmm0
	# v11 = ptradd v9, v3
	add	rsi, 8
	# v12 = uitofp.f32 v1
	xorps	xmm0, xmm0
	mov	rax, rdi
	test	rax, rax
	js	".Lfrom_int_inst13_big"
	cvtsi2ss	xmm0, rax
	jmp	".Lfrom_int_inst13_done"
.Lfrom_int_inst13_big:
	mov	rcx, rax
	shr	rcx, 1
	and	eax, 1
	or	rcx, rax
	cvtsi2ss	xmm0, rcx
	addss	xmm0, xmm0
.Lfrom_int_inst13_done:
	# v13 = fpext.f64 v12
	cvtss2sd	xmm0, xmm0
	# store v13, v11
	movsd	qword ptr [rsi], xmm0
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lfrom_int_end:
	.cfi_endproc
	.size "from_int", .-"from_int"
	.section	".text","ax",@progbits
	.globl "bits"
	.p2align	4
	.type	"bits",@function
"bits":
.Lbits_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lbits_block0:
	# v1 = bitcast.i64 v0
	movq	rsi, xmm0
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lbits_end:
	.cfi_endproc
	.size "bits", .-"bits"
	.section	".text","ax",@progbits
	.globl "from_bits32"
	.p2align	4
	.type	"from_bits32",@function
"from_bits32":
.Lfrom_bits32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lfrom_bits32_block0:
	# v1 = bitcast.f32 v0
	movd	xmm0, esi
	# return v1
	mov	rsp, rbp
	pop	rbp
	ret
.Lfrom_bits32_end:
	.cfi_endproc
	.size "from_bits32", .-"from_bits32"
	.section	".text","ax",@progbits
	.globl "narrow"
	.p2align	4
	.type	"narrow",@function
"narrow":
.Lnarrow_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lnarrow_block0:
	# v1 = fptrunc.f32 v0
	cvtsd2ss	xmm0, xmm0
	# return v1
	mov	rsp, rbp
	pop	rbp
	ret
.Lnarrow_end:
	.cfi_endproc
	.size "narrow", .-"narrow"
	.section	".text","ax",@progbits
	.globl "neg_zero"
	.p2align	4
	.type	"neg_zero",@function
"neg_zero":
.Lneg_zero_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lneg_zero_block0:
	# v0 = fconst.f64 -0.0
	movabs	rax, 9223372036854775808
	movq	xmm0, rax
	# return v0
	mov	rsp, rbp
	pop	rbp
	ret
.Lneg_zero_end:
	.cfi_endproc
	.size "neg_zero", .-"neg_zero"
	.section	".text","ax",@progbits
	.globl "snan"
	.p2align	4
	.type	"snan",@function
"snan":
.Lsnan_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lsnan_block0:
	# v0 = fconst.f64 0x7ff0000000000001
	movabs	rax, 9218868437227405313
	movq	xmm0, rax
	# return v0
	mov	rsp, rbp
	pop	rbp
	ret
.Lsnan_end:
	.cfi_endproc
	.size "snan", .-"snan"
	.section	".text","ax",@progbits
	.globl "tiny"
	.p2align	4
	.type	"tiny",@function
"tiny":
.Ltiny_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Ltiny_block0:
	# v0 = fconst.f64 5e-324
	movabs	rax, 1
	movq	xmm0, rax
	# return v0
	mov	rsp, rbp
	pop	rbp
	ret
.Ltiny_end:
	.cfi_endproc
	.size "tiny", .-"tiny"
	.section	".text","ax",@progbits
	.globl "tenth32"
	.p2align	4
	.type	"tenth32",@function
"tenth32":
.Ltenth32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Ltenth32_block0:
	# v0 = fconst.f32 0.1
	movabs	rax, 1036831949
	movq	xmm0, rax
	# return v0
	mov	rsp, rbp
	pop	rbp
	ret
.Ltenth32_end:
	.cfi_endproc
	.size "tenth32", .-"tenth32"
	.section	".text","ax",@progbits
	.globl "select_f"
	.p2align	4
	.type	"select_f",@function
"select_f":
.Lselect_f_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lselect_f_block0:
	# v3 = select v0, v1, v2
	test	esi, esi
	movaps	xmm13, xmm1
	je	".Lselect_f_inst0_select"
	movaps	xmm13, xmm0
.Lselect_f_inst0_select:
	movaps	xmm0, xmm13
	# return v3
	mov	rsp, rbp
	pop	rbp
	ret
.Lselect_f_end:
	.cfi_endproc
	.size "select_f", .-"select_f"
	.section	".text","ax",@progbits
	.globl "harmonic"
	.p2align	4
	.type	"harmonic",@function
"harmonic":
.Lharmonic_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lharmonic_block0:
	# v1 = fconst.f64 0.0
	xorps	xmm0, xmm0
	# v2 = iconst.i64 1
	mov	edi, 1
	# jump block1(v2, v1)
	mov	r8, rdi
.Lharmonic_block1:
	# v5 = icmp.sle v3, v0
	cmp	r8, rsi
	setle	r9b
	# brif v5, block2, block3
	test	r9b, r9b
	jne	".Lharmonic_block2"
.Lharmonic_block3:
	# return v4
	mov	rsp, rbp
	pop	rbp
	ret
.Lharmonic_block2:
	# v6 = sitofp.f64 v3
	xorps	xmm1, xmm1
	cvtsi2sd	xmm1, r8
	# v7 = fconst.f64 1.0
	movabs	rax, 4607182418800017408
	movq	xmm2, rax
	# v8 = fdiv v7, v6
	movaps	xmm13, xmm2
	divsd	xmm13, xmm1
	movaps	xmm1, xmm13
	# v9 = fadd v4, v8
	addsd	xmm0, xmm1
	# v10 = iadd v3, v2
	add	r8, 1
	# jump block1(v10, v9)
	jmp	".Lharmonic_block1"
.Lharmonic_end:
	.cfi_endproc
	.size "harmonic", .-"harmonic"
	.section	".text","ax",@progbits
	.globl "swaps"
	.p2align	4
	.type	"swaps",@function
"swaps":
.Lswaps_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lswaps_block0:
	# v3 = iconst.i64 0
	xor	edi, edi
	# jump block1(v0, v1, v3)
.Lswaps_block1:
	# v7 = icmp.ult v6, v2
	cmp	rdi, rsi
	setb	r8b
	# brif v7, block2, block3
	test	r8b, r8b
	jne	".Lswaps_block2"
.Lswaps_block3:
	# v10 = fconst.f64 10.0
	movabs	rax, 4621819117588971520
	movq	xmm2, rax
	# v11 = fmul v4, v10
	mulsd	xmm0, xmm2
	# v12 = fadd v11, v5
	addsd	xmm0, xmm1
	# return v12
	mov	rsp, rbp
	pop	rbp
	ret
.Lswaps_block2:
	# v8 = iconst.i64 1
	# v9 = iadd v6, v8
	add	rdi, 1
	# jump block1(v5, v4, v9)
	movaps	xmm14, xmm0
	movaps	xmm0, xmm1
	movaps	xmm1, xmm14
	jmp	".Lswaps_block1"
.Lswaps_end:
	.cfi_endproc
	.size "swaps", .-"swaps"
	.section	".text","ax",@progbits
	.globl "pressure"
	.p2align	4
	.type	"pressure",@function
"pressure":
.Lpressure_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 160
	movsd	qword ptr [rbp + -160], xmm0
.Lpressure_block0:
	# v1 = fconst.f64 1.0
	movabs	rax, 4607182418800017408
	movq	xmm0, rax
	# v2 = fadd v0, v1
	movsd	xmm13, qword ptr [rbp + -160]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -152], xmm13
	# v3 = fadd v2, v1
	movsd	xmm13, qword ptr [rbp + -152]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -144], xmm13
	# v4 = fadd v3, v1
	movsd	xmm13, qword ptr [rbp + -144]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -136], xmm13
	# v5 = fadd v4, v1
	movsd	xmm13, qword ptr [rbp + -136]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -128], xmm13
	# v6 = fadd v5, v1
	movsd	xmm13, qword ptr [rbp + -128]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -120], xmm13
	# v7 = fadd v6, v1
	movsd	xmm13, qword ptr [rbp + -120]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -112], xmm13
	# v8 = fadd v7, v1
	movsd	xmm13, qword ptr [rbp + -112]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -104], xmm13
	# v9 = fadd v8, v1
	movsd	xmm13, qword ptr [rbp + -104]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -96], xmm13
	# v10 = fadd v9, v1
	movsd	xmm13, qword ptr [rbp + -96]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -88], xmm13
	# v11 = fadd v10, v1
	movsd	xmm13, qword ptr [rbp + -88]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -80], xmm13
	# v12 = fadd v11, v1
	movsd	xmm13, qword ptr [rbp + -80]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -72], xmm13
	# v13 = fadd v12, v1
	movsd	xmm13, qword ptr [rbp + -72]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -64], xmm13
	# v14 = fadd v13, v1
	movsd	xmm13, qword ptr [rbp + -64]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -56], xmm13
	# v15 = fadd v14, v1
	movsd	xmm13, qword ptr [rbp + -56]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -48], xmm13
	# v16 = fadd v15, v1
	movsd	xmm13, qword ptr [rbp + -48]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -40], xmm13
	# v17 = fadd v16, v1
	movsd	xmm13, qword ptr [rbp + -40]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -32], xmm13
	# v18 = fadd v17, v1
	movsd	xmm13, qword ptr [rbp + -32]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -24], xmm13
	# v19 = fadd v18, v1
	movsd	xmm13, qword ptr [rbp + -24]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -16], xmm13
	# v20 = fadd v19, v1
	movsd	xmm13, qword ptr [rbp + -16]
	addsd	xmm13, xmm0
	movsd	qword ptr [rbp + -8], xmm13
	# call @fclobber()
	call	"fclobber"@PLT
	# v21 = fmul v2, v3
	movsd	xmm13, qword ptr [rbp + -152]
	movsd	xmm14, qword ptr [rbp + -144]
	movaps	xmm0, xmm13
	mulsd	xmm0, xmm14
	# v22 = fadd v21, v4
	movsd	xmm14, qword ptr [rbp + -136]
	addsd	xmm0, xmm14
	# v23 = fmul v22, v5
	movsd	xmm14, qword ptr [rbp + -128]
	mulsd	xmm0, xmm14
	# v24 = fadd v23, v6
	movsd	xmm14, qword ptr [rbp + -120]
	addsd	xmm0, xmm14
	# v25 = fsub v24, v7
	movsd	xmm14, qword ptr [rbp + -112]
	subsd	xmm0, xmm14
	# v26 = fadd v25, v8
	movsd	xmm14, qword ptr [rbp + -104]
	addsd	xmm0, xmm14
	# v27 = fsub v26, v9
	movsd	xmm14, qword ptr [rbp + -96]
	subsd	xmm0, xmm14
	# v28 = fadd v27, v10
	movsd	xmm14, qword ptr [rbp + -88]
	addsd	xmm0, xmm14
	# v29 = fmul v28, v11
	movsd	xmm14, qword ptr [rbp + -80]
	mulsd	xmm0, xmm14
	# v30 = fadd v29, v12
	movsd	xmm14, qword ptr [rbp + -72]
	addsd	xmm0, xmm14
	# v31 = fsub v30, v13
	movsd	xmm14, qword ptr [rbp + -64]
	subsd	xmm0, xmm14
	# v32 = fadd v31, v14
	movsd	xmm14, qword ptr [rbp + -56]
	addsd	xmm0, xmm14
	# v33 = fadd v32, v15
	movsd	xmm14, qword ptr [rbp + -48]
	addsd	xmm0, xmm14
	# v34 = fsub v33, v16
	movsd	xmm14, qword ptr [rbp + -40]
	subsd	xmm0, xmm14
	# v35 = fadd v34, v17
	movsd	xmm14, qword ptr [rbp + -32]
	addsd	xmm0, xmm14
	# v36 = fadd v35, v18
	movsd	xmm14, qword ptr [rbp + -24]
	addsd	xmm0, xmm14
	# v37 = fsub v36, v19
	movsd	xmm14, qword ptr [rbp + -16]
	subsd	xmm0, xmm14
	# v38 = fadd v37, v20
	movsd	xmm14, qword ptr [rbp + -8]
	addsd	xmm0, xmm14
	# v39 = fadd v38, v0
	movsd	xmm14, qword ptr [rbp + -160]
	addsd	xmm0, xmm14
	# return v39
	mov	rsp, rbp
	pop	rbp
	ret
.Lpressure_end:
	.cfi_endproc
	.size "pressure", .-"pressure"
	.section	".text","ax",@progbits
	.globl "consts"
	.p2align	4
	.type	"consts",@function
"consts":
.Lconsts_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lconsts_block0:
	# v1 = iconst.i64 8
	# v2 = fconst.f64 0.1
	movabs	rax, 4591870180066957722
	movq	xmm0, rax
	# v3 = fconst.f64 0.2
	movabs	rax, 4596373779694328218
	movq	xmm1, rax
	# v4 = fadd v2, v3
	addsd	xmm0, xmm1
	# store v4, v0
	movsd	qword ptr [rsi], xmm0
	# v5 = ptradd v0, v1
	add	rsi, 8
	# v6 = fconst.f64 1.0
	movabs	rax, 4607182418800017408
	movq	xmm0, rax
	# v7 = fconst.f64 3.0
	movabs	rax, 4613937818241073152
	movq	xmm1, rax
	# v8 = fdiv v6, v7
	divsd	xmm0, xmm1
	# store v8, v5
	movsd	qword ptr [rsi], xmm0
	# v9 = ptradd v5, v1
	add	rsi, 8
	# v10 = fconst.f64 2.0
	movabs	rax, 4611686018427387904
	movq	xmm0, rax
	# v11 = fsqrt v10
	sqrtsd	xmm0, xmm0
	# store v11, v9
	movsd	qword ptr [rsi], xmm0
	# v12 = ptradd v9, v1
	add	rsi, 8
	# v13 = fconst.f64 -2.5
	movabs	rax, 13836183955189006336
	movq	xmm0, rax
	# v14 = fnearest v13
	movaps	xmm14, xmm0
	movq	rax, xmm14
	btr	rax, 63
	movq	xmm15, rax
	mov	rcx, rax
	shr	rcx, 52
	cmp	rcx, 2047
	je	".Lconsts_inst16_round_nan"
	cmp	rcx, 1075
	jae	".Lconsts_inst16_round_sign"
	cmp	rcx, 1023
	jb	".Lconsts_inst16_round_small"
	neg	rcx
	add	rcx, 1075
	mov	edx, 1
	shl	rdx, cl
	sub	rdx, 1
	and	rax, rdx
	add	rdx, 1
	test	rax, rax
	je	".Lconsts_inst16_round_truncate"
	mov	rcx, rdx
	shr	rcx, 1
	cmp	rax, rcx
	ja	".Lconsts_inst16_round_increment"
	jb	".Lconsts_inst16_round_truncate"
	movq	rax, xmm15
	test	rax, rdx
	jne	".Lconsts_inst16_round_increment"
	jmp	".Lconsts_inst16_round_truncate"
.Lconsts_inst16_round_increment:
	movq	rax, xmm15
	add	rax, rdx
	movq	xmm15, rax
.Lconsts_inst16_round_truncate:
	movq	rax, xmm15
	neg	rdx
	and	rax, rdx
	jmp	".Lconsts_inst16_round_sign"
.Lconsts_inst16_round_small:
	test	rcx, rcx
	jne	".Lconsts_inst16_round_normal_small"
	stmxcsr	dword ptr [rbp + -16]
	mov	ecx, dword ptr [rbp + -16]
	test	ecx, 64
	je	".Lconsts_inst16_round_normal_small"
	xor	eax, eax
	jmp	".Lconsts_inst16_round_sign"
.Lconsts_inst16_round_normal_small:
	mov	rcx, rax
	xor	eax, eax
	movabs	rdx, 4602678819172646912
	cmp	rcx, rdx
	jbe	".Lconsts_inst16_round_small_done"
	movabs	rax, 4607182418800017408
.Lconsts_inst16_round_small_done:
.Lconsts_inst16_round_sign:
	movq	rcx, xmm14
	shr	rcx, 63
	shl	rcx, 63
	or	rax, rcx
	movq	xmm0, rax
	jmp	".Lconsts_inst16_round_done"
.Lconsts_inst16_round_nan:
	movaps	xmm13, xmm14
	addsd	xmm13, xmm14
	movaps	xmm0, xmm13
.Lconsts_inst16_round_done:
	# store v14, v12
	movsd	qword ptr [rsi], xmm0
	# v15 = ptradd v12, v1
	add	rsi, 8
	# v16 = fconst.f64 0.0
	xorps	xmm0, xmm0
	# v17 = fconst.f64 -0.0
	movabs	rax, 9223372036854775808
	movq	xmm1, rax
	# v18 = fmin v16, v17
	movaps	xmm2, xmm0
	ucomisd	xmm2, xmm1
	jp	".Lconsts_inst21_nan"
	jne	".Lconsts_inst21_ne"
	orpd	xmm2, xmm1
	jmp	".Lconsts_inst21_done"
.Lconsts_inst21_ne:
	minsd	xmm2, xmm1
	jmp	".Lconsts_inst21_done"
.Lconsts_inst21_nan:
	addsd	xmm2, xmm1
.Lconsts_inst21_done:
	# store v18, v15
	movsd	qword ptr [rsi], xmm2
	# v19 = ptradd v15, v1
	add	rsi, 8
	# v20 = fconst.f64 1e300
	movabs	rax, 9094988921128908188
	movq	xmm2, rax
	# v21 = fmul v20, v20
	mulsd	xmm2, xmm2
	# store v21, v19
	movsd	qword ptr [rsi], xmm2
	# v22 = ptradd v19, v1
	add	rsi, 8
	# v23 = fptosi.i64 v21
	cvttsd2si	rax, xmm2
	movabs	r11, 4890909195324358656
	movq	xmm14, r11
	ucomisd	xmm2, xmm14
	movabs	rcx, 9223372036854775807
	cmovae	rax, rcx
	mov	ecx, 0
	cmovp	rax, rcx
	mov	r8, rax
	# store v23, v22
	mov	qword ptr [rsi], r8
	# v24 = ptradd v22, v1
	add	rsi, 8
	# v25 = fconst.f32 16777216.0
	movabs	rax, 1266679808
	movq	xmm3, rax
	# v26 = fconst.f32 1.0
	movabs	rax, 1065353216
	movq	xmm4, rax
	# v27 = fadd v25, v26
	addss	xmm3, xmm4
	# v28 = fpext.f64 v27
	cvtss2sd	xmm3, xmm3
	# store v28, v24
	movsd	qword ptr [rsi], xmm3
	# v29 = ptradd v24, v1
	add	rsi, 8
	# v30 = iconst.i64 -1
	mov	r8, -1
	# v31 = uitofp.f64 v30
	xorps	xmm3, xmm3
	mov	rax, r8
	test	rax, rax
	js	".Lconsts_inst38_big"
	cvtsi2sd	xmm3, rax
	jmp	".Lconsts_inst38_done"
.Lconsts_inst38_big:
	mov	rcx, rax
	shr	rcx, 1
	and	eax, 1
	or	rcx, rax
	cvtsi2sd	xmm3, rcx
	addsd	xmm3, xmm3
.Lconsts_inst38_done:
	# store v31, v29
	movsd	qword ptr [rsi], xmm3
	# v32 = ptradd v29, v1
	add	rsi, 8
	# v33 = fcmp.lt v16, v17
	ucomisd	xmm1, xmm0
	seta	dil
	# v34 = fcmp.ne v21, v21
	ucomisd	xmm2, xmm2
	setp	r8b
	# v35 = uext.i64 v33
	movzx	edi, dil
	# v36 = uext.i64 v34
	movzx	r8d, r8b
	# v37 = iconst.i64 2
	# v38 = imul v36, v37
	imul	r8, r8, 2
	# v39 = iadd v35, v38
	add	rdi, r8
	# store v39, v32
	mov	qword ptr [rsi], rdi
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lconsts_end:
	.cfi_endproc
	.size "consts", .-"consts"
	.section	".note.GNU-stack","",@progbits
