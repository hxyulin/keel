	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_many"
	.p2align	4
	.type	"k_many",@function
"k_many":
.Lk_many_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	movq	xmm14, rsi
	mov	rsi, rdi
	movq	rdi, xmm14
	mov	r13, qword ptr [rbp + 16]
	mov	r14, qword ptr [rbp + 24]
	mov	r15, qword ptr [rbp + 32]
	mov	rax, qword ptr [rbp + 40]
	mov	qword ptr [rbp + -64], rax
	mov	rax, qword ptr [rbp + 48]
	mov	qword ptr [rbp + -56], rax
	movss	xmm8, dword ptr [rbp + 56]
	movsd	xmm9, qword ptr [rbp + 64]
	mov	rax, qword ptr [rbp + 72]
	mov	qword ptr [rbp + -48], rax
.Lk_many_block0:
	# v22 = iconst.i64 1
	# v24 = iconst.i64 2
	# v25 = shl v1, v22
	# v26 = iadd v0, v25
	lea	rsi, [rsi + rdi * 2]
	# v27 = iconst.i64 3
	# v28 = imul v2, v27
	imul	r8, r8, 3
	# v29 = iadd v26, v28
	add	rsi, r8
	# v31 = shl v3, v24
	# v32 = iadd v29, v31
	lea	rsi, [rsi + r9 * 4]
	# v33 = iconst.i64 5
	# v34 = imul v4, v33
	imul	rbx, rbx, 5
	# v35 = iadd v32, v34
	add	rsi, rbx
	# v36 = iconst.i64 6
	# v37 = imul v5, v36
	imul	r12, r12, 6
	# v38 = iadd v35, v37
	add	rsi, r12
	# v39 = iconst.i64 7
	# v40 = imul v6, v39
	imul	r13, r13, 7
	# v41 = iadd v38, v40
	add	rsi, r13
	# v43 = shl v7, v27
	# v44 = iadd v41, v43
	lea	rsi, [rsi + r14 * 8]
	# v45 = sitofp.f64 v44
	xorps	xmm10, xmm10
	cvtsi2sd	xmm10, rsi
	# v46 = fconst.f64 11.0
	movabs	rax, 4622382067542392832
	movq	xmm11, rax
	# v47 = fmul v8, v46
	mulsd	xmm0, xmm11
	# v48 = fconst.f64 12.0
	movabs	rax, 4622945017495814144
	movq	xmm11, rax
	# v49 = fmul v9, v48
	mulsd	xmm1, xmm11
	# v50 = fadd v47, v49
	addsd	xmm0, xmm1
	# v51 = fconst.f64 13.0
	movabs	rax, 4623507967449235456
	movq	xmm1, rax
	# v52 = fmul v10, v51
	mulsd	xmm2, xmm1
	# v53 = fadd v50, v52
	addsd	xmm0, xmm2
	# v54 = fconst.f64 14.0
	movabs	rax, 4624070917402656768
	movq	xmm1, rax
	# v55 = fmul v11, v54
	mulsd	xmm3, xmm1
	# v56 = fadd v53, v55
	addsd	xmm0, xmm3
	# v57 = fconst.f64 15.0
	movabs	rax, 4624633867356078080
	movq	xmm1, rax
	# v58 = fmul v12, v57
	mulsd	xmm4, xmm1
	# v59 = fadd v56, v58
	addsd	xmm0, xmm4
	# v60 = fconst.f64 16.0
	movabs	rax, 4625196817309499392
	movq	xmm1, rax
	# v61 = fmul v13, v60
	mulsd	xmm5, xmm1
	# v62 = fadd v59, v61
	addsd	xmm0, xmm5
	# v63 = fconst.f64 17.0
	movabs	rax, 4625478292286210048
	movq	xmm1, rax
	# v64 = fmul v14, v63
	mulsd	xmm6, xmm1
	# v65 = fadd v62, v64
	addsd	xmm0, xmm6
	# v66 = fconst.f64 18.0
	movabs	rax, 4625759767262920704
	movq	xmm1, rax
	# v67 = fmul v15, v66
	mulsd	xmm7, xmm1
	# v68 = fadd v65, v67
	addsd	xmm0, xmm7
	# v69 = fadd v45, v68
	addsd	xmm10, xmm0
	# v70 = sitofp.f64 v16
	xorps	xmm0, xmm0
	cvtsi2sd	xmm0, r15d
	# v71 = fconst.f64 101.0
	movabs	rax, 4636807660098813952
	movq	xmm1, rax
	# v72 = fmul v70, v71
	mulsd	xmm0, xmm1
	# v73 = sitofp.f64 v17
	mov	rax, qword ptr [rbp + -64]
	xorps	xmm1, xmm1
	cvtsi2sd	xmm1, rax
	# v74 = fconst.f64 103.0
	movabs	rax, 4636948397587169280
	movq	xmm2, rax
	# v75 = fmul v73, v74
	mulsd	xmm1, xmm2
	# v76 = sitofp.f64 v18
	mov	rax, qword ptr [rbp + -56]
	xorps	xmm2, xmm2
	cvtsi2sd	xmm2, eax
	# v77 = fconst.f64 107.0
	movabs	rax, 4637229872563879936
	movq	xmm3, rax
	# v78 = fmul v76, v77
	mulsd	xmm2, xmm3
	# v79 = fpext.f64 v19
	cvtss2sd	xmm3, xmm8
	# v80 = fconst.f64 109.0
	movabs	rax, 4637370610052235264
	movq	xmm4, rax
	# v81 = fmul v79, v80
	mulsd	xmm3, xmm4
	# v82 = fconst.f64 113.0
	movabs	rax, 4637652085028945920
	movq	xmm4, rax
	# v83 = fmul v20, v82
	mulsd	xmm9, xmm4
	# v84 = sitofp.f64 v21
	mov	rax, qword ptr [rbp + -48]
	xorps	xmm4, xmm4
	cvtsi2sd	xmm4, eax
	# v85 = fconst.f64 127.0
	movabs	rax, 4638637247447433216
	movq	xmm5, rax
	# v86 = fmul v84, v85
	mulsd	xmm4, xmm5
	# v87 = fadd v69, v72
	addsd	xmm10, xmm0
	# v88 = fadd v87, v75
	addsd	xmm10, xmm1
	# v89 = fadd v88, v78
	addsd	xmm10, xmm2
	# v90 = fadd v89, v81
	addsd	xmm10, xmm3
	# v91 = fadd v90, v83
	addsd	xmm10, xmm9
	# v92 = fadd v91, v86
	addsd	xmm10, xmm4
	# return v92
	movaps	xmm0, xmm10
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_many_end:
	.cfi_endproc
	.size "k_many", .-"k_many"
	.section	".text","ax",@progbits
	.globl "k_call_many"
	.p2align	4
	.type	"k_call_many",@function
"k_call_many":
.Lk_call_many_begin:
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
	mov	qword ptr [rbp + -80], rsi
	mov	rsi, rdi
.Lk_call_many_block0:
	# v2 = iconst.i64 1
	# v3 = iadd v0, v2
	lea	rax, [rsi + 1]
	mov	qword ptr [rbp + -72], rax
	# v4 = iadd v3, v2
	mov	rax, qword ptr [rbp + -72]
	add	rax, 1
	mov	qword ptr [rbp + -64], rax
	# v5 = iadd v4, v2
	mov	rax, qword ptr [rbp + -64]
	lea	rbx, [rax + 1]
	# v6 = iadd v5, v2
	lea	r12, [rbx + 1]
	# v7 = iadd v6, v2
	lea	r13, [r12 + 1]
	# v8 = iadd v7, v2
	lea	r14, [r13 + 1]
	# v9 = iadd v8, v2
	lea	rax, [r14 + 1]
	mov	qword ptr [rbp + -56], rax
	# v10 = sitofp.f64 v0
	xorps	xmm0, xmm0
	cvtsi2sd	xmm0, rsi
	# v11 = fconst.f64 0.25
	movabs	rax, 4598175219545276416
	movq	xmm1, rax
	# v12 = fmul v10, v11
	movaps	xmm13, xmm0
	mulsd	xmm13, xmm1
	movaps	xmm1, xmm13
	# v13 = fadd v12, v12
	movaps	xmm2, xmm1
	addsd	xmm2, xmm1
	# v14 = fadd v13, v12
	movaps	xmm3, xmm2
	addsd	xmm3, xmm1
	# v15 = fadd v14, v12
	movaps	xmm4, xmm3
	addsd	xmm4, xmm1
	# v16 = fadd v15, v12
	movaps	xmm5, xmm4
	addsd	xmm5, xmm1
	# v17 = fadd v16, v12
	movaps	xmm6, xmm5
	addsd	xmm6, xmm1
	# v18 = fadd v17, v12
	movaps	xmm7, xmm6
	addsd	xmm7, xmm1
	# v19 = fadd v18, v12
	movaps	xmm8, xmm7
	addsd	xmm8, xmm1
	# v20 = trunc.i32 v0
	mov	rdi, rsi
	# v21 = ineg v20
	mov	r15, rdi
	neg	r15d
	# v22 = iconst.i64 33
	# v23 = shl v0, v22
	mov	r8, rsi
	shl	r8, 33
	# v24 = iconst.i32 7
	mov	r9d, 7
	# v25 = isub v24, v20
	sub	r9d, edi
	# v26 = fptrunc.f32 v10
	cvtsd2ss	xmm9, xmm0
	# v27 = fconst.f32 0.125
	movabs	rax, 1040187392
	movq	xmm10, rax
	# v28 = fmul v26, v27
	mulss	xmm9, xmm10
	# v29 = fconst.f64 1.5
	movabs	rax, 4609434218613702656
	movq	xmm10, rax
	# v30 = fmul v10, v29
	mulsd	xmm0, xmm10
	# v31 = iconst.i32 1000
	# v32 = imul v20, v31
	imul	edi, edi, 1000
	# v33 = call_indirect v1(v0, v3, v4, v5, v6, v7, v8, v9, v12, v13, v14, v15, v16, v17, v18, v19, v21, v23, v25, v28, v30, v32) : (i64, i64, i64, i64, i64, i64, i64, i64, f64, f64, f64, f64, f64, f64, f64, f64, i32, i64, i32, f32, f64, i32) -> f64 cc(c)
	mov	qword ptr [rsp], r14
	mov	rax, qword ptr [rbp + -56]
	mov	qword ptr [rsp + 8], rax
	mov	qword ptr [rsp + 16], r15
	mov	qword ptr [rsp + 24], r8
	mov	qword ptr [rsp + 32], r9
	movsd	qword ptr [rsp + 40], xmm9
	movsd	qword ptr [rsp + 48], xmm0
	mov	qword ptr [rsp + 56], rdi
	mov	rdi, rsi
	mov	rsi, qword ptr [rbp + -72]
	mov	rdx, qword ptr [rbp + -64]
	mov	rcx, rbx
	mov	r8, r12
	mov	r9, r13
	movaps	xmm0, xmm1
	movaps	xmm1, xmm2
	movaps	xmm2, xmm3
	movaps	xmm3, xmm4
	movaps	xmm4, xmm5
	movaps	xmm5, xmm6
	movaps	xmm6, xmm7
	movaps	xmm7, xmm8
	mov	r10, qword ptr [rbp + -80]
	call	r10
	# return v33
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_call_many_end:
	.cfi_endproc
	.size "k_call_many", .-"k_call_many"
	.section	".text","ax",@progbits
	.globl "k_call_c_many"
	.p2align	4
	.type	"k_call_c_many",@function
"k_call_c_many":
.Lk_call_c_many_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_call_c_many_block0:
	# v1 = symbol_addr @c_many
	mov	rdi, qword ptr [rip + "c_many"@GOTPCREL]
	# v2 = call @k_call_many(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"k_call_many"
	# return v2
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_call_c_many_end:
	.cfi_endproc
	.size "k_call_c_many", .-"k_call_c_many"
	.section	".text","ax",@progbits
	.globl "k_print"
	.p2align	4
	.type	"k_print",@function
"k_print":
.Lk_print_begin:
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
	sub	rsp, 72
.Lk_print_block0:
	# v0 = symbol_addr @fmt
	lea	rax, [rip + "fmt"]
	mov	qword ptr [rbp + -64], rax
	# v1 = iconst.i32 7
	mov	edi, 7
	# v2 = fconst.f64 2.5
	movabs	rax, 4612811918334230528
	movq	xmm0, rax
	# v3 = symbol_addr @hi
	lea	rax, [rip + "hi"]
	mov	qword ptr [rbp + -56], rax
	# v4 = iconst.i64 123456789012
	movabs	r9, 123456789012
	# v5 = iconst.i32 -1
	mov	ebx, 4294967295
	# v6 = iconst.i32 2
	mov	r12d, 2
	# v7 = iconst.i32 3
	mov	r13d, 3
	# v8 = iconst.i32 4
	mov	r14d, 4
	# v9 = iconst.i32 5
	mov	r15d, 5
	# v10 = iconst.i32 6
	mov	esi, 6
	# v12 = iconst.i32 8
	mov	r8d, 8
	# v13 = call @printf(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v1, v12)
	mov	qword ptr [rsp], r13
	mov	qword ptr [rsp + 8], r14
	mov	qword ptr [rsp + 16], r15
	mov	qword ptr [rsp + 24], rsi
	mov	qword ptr [rsp + 32], rdi
	mov	qword ptr [rsp + 40], r8
	mov	rsi, rdi
	mov	rdi, qword ptr [rbp + -64]
	mov	rdx, qword ptr [rbp + -56]
	mov	rcx, r9
	mov	r8, rbx
	mov	r9, r12
	mov	eax, 1
	call	"printf"@PLT
	mov	rsi, rax
	# v14 = symbol_addr @plain
	lea	rsi, [rip + "plain"]
	# v15 = call @printf(v14)
	mov	rdi, rsi
	mov	eax, 0
	call	"printf"@PLT
	mov	rsi, rax
	# return
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_print_end:
	.cfi_endproc
	.size "k_print", .-"k_print"
	.section	".text","ax",@progbits
	.globl "k_vsum"
	.p2align	4
	.type	"k_vsum",@function
"k_vsum":
.Lk_vsum_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
.Lk_vsum_block0:
	# v0 = symbol_addr @types
	lea	rsi, [rip + "types"]
	# v1 = iconst.i32 1
	mov	edi, 1
	# v2 = fconst.f64 2.5
	movabs	rax, 4612811918334230528
	movq	xmm0, rax
	# v3 = iconst.i64 3
	mov	r8d, 3
	# v4 = iconst.i32 -4
	mov	r9d, 4294967292
	# v5 = fconst.f64 0.25
	movabs	rax, 4598175219545276416
	movq	xmm1, rax
	# v6 = fconst.f64 -1.0
	movabs	rax, 13830554455654793216
	movq	xmm2, rax
	# v7 = symbol_addr @c_vsum
	mov	rbx, qword ptr [rip + "c_vsum"@GOTPCREL]
	# v8 = call_indirect v7(v0, v1, v2, v3, v0, v4, v5, v6) : (ptr, ...) -> i64 cc(c)
	mov	rdx, r8
	mov	rcx, rsi
	mov	r8, r9
	mov	r10, rbx
	movq	xmm14, rdi
	mov	rdi, rsi
	movq	rsi, xmm14
	mov	eax, 3
	call	r10
	mov	rsi, rax
	# return v8
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_vsum_end:
	.cfi_endproc
	.size "k_vsum", .-"k_vsum"
	.section	".text","ax",@progbits
	.globl "k_vtail"
	.p2align	4
	.type	"k_vtail",@function
"k_vtail":
.Lk_vtail_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 48
.Lk_vtail_block0:
	# v0 = iconst.i64 1
	mov	esi, 1
	# v1 = iconst.i32 -9
	mov	edi, 4294967287
	# v2 = iconst.i64 100
	mov	r8d, 100
	# v3 = fconst.f64 0.5
	movabs	rax, 4602678819172646912
	movq	xmm0, rax
	# v4 = iconst.i32 3
	mov	r9d, 3
	# v5 = call @c_vtail(v0, v0, v0, v0, v0, v0, v0, v0, v1, v2, v3, v4)
	mov	qword ptr [rsp], rsi
	mov	qword ptr [rsp + 8], rsi
	mov	qword ptr [rsp + 16], rdi
	mov	qword ptr [rsp + 24], r8
	mov	qword ptr [rsp + 32], r9
	mov	rdi, rsi
	mov	rdx, rsi
	mov	rcx, rsi
	mov	r8, rsi
	mov	r9, rsi
	mov	eax, 1
	call	"c_vtail"@PLT
	mov	rsi, rax
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_vtail_end:
	.cfi_endproc
	.size "k_vtail", .-"k_vtail"
	.section	".text","ax",@progbits
	.globl "k_small"
	.p2align	4
	.type	"k_small",@function
"k_small":
.Lk_small_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rsi, qword ptr [rbp + 24]
	mov	rdi, qword ptr [rbp + 32]
	mov	r8, qword ptr [rbp + 40]
	mov	r9, qword ptr [rbp + 48]
	mov	rbx, qword ptr [rbp + 56]
.Lk_small_block0:
	# v12 = sext.i64 v8
	movsx	rdi, dil
	# v13 = sext.i64 v9
	movsx	r8, r8w
	# v14 = iconst.i64 10
	# v15 = imul v13, v14
	imul	r8, r8, 10
	# v16 = sext.i64 v10
	movsx	r9, r9b
	# v17 = iconst.i64 1000
	# v18 = imul v16, v17
	imul	r9, r9, 1000
	# v19 = sext.i64 v11
	movsxd	rbx, ebx
	# v20 = iconst.i64 100000
	# v21 = imul v19, v20
	imul	rbx, rbx, 100000
	# v22 = iadd v12, v15
	add	rdi, r8
	# v23 = iadd v18, v22
	add	r9, rdi
	# v24 = iadd v21, v23
	add	rbx, r9
	# v25 = iadd v7, v24
	add	rsi, rbx
	# return v25
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_small_end:
	.cfi_endproc
	.size "k_small", .-"k_small"
	.section	".text","ax",@progbits
	.globl "k_call_small"
	.p2align	4
	.type	"k_call_small",@function
"k_call_small":
.Lk_call_small_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 56
	mov	rsi, rdi
.Lk_call_small_block0:
	# v1 = iconst.i8 -3
	mov	edi, 253
	# v2 = iconst.i16 -300
	mov	r8d, 65236
	# v3 = iconst.i8 5
	mov	r9d, 5
	# v4 = trunc.i32 v0
	mov	rbx, rsi
	# v5 = call @k_small(v0, v0, v0, v0, v0, v0, v0, v0, v1, v2, v3, v4)
	mov	qword ptr [rsp], rsi
	mov	qword ptr [rsp + 8], rsi
	mov	qword ptr [rsp + 16], rdi
	mov	qword ptr [rsp + 24], r8
	mov	qword ptr [rsp + 32], r9
	mov	qword ptr [rsp + 40], rbx
	mov	rdi, rsi
	mov	rdx, rsi
	mov	rcx, rsi
	mov	r8, rsi
	mov	r9, rsi
	call	"k_small"
	mov	rsi, rax
	# return v5
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_call_small_end:
	.cfi_endproc
	.size "k_call_small", .-"k_call_small"
	.section	".rodata","a",@progbits
	.globl "fmt"
	.type	"fmt",@object
	.size "fmt", 40
	.p2align	0
"fmt":
	.byte	37, 100, 32, 37, 46, 51, 102, 32, 37, 115, 32, 37, 108, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 10, 0
	.section	".rodata","a",@progbits
	.globl "plain"
	.type	"plain",@object
	.size "plain", 14
	.p2align	0
"plain":
	.byte	110, 111, 32, 97, 114, 103, 117, 109, 101, 110, 116, 115, 10, 0
	.section	".rodata","a",@progbits
	.globl "hi"
	.type	"hi",@object
	.size "hi", 3
	.p2align	0
"hi":
	.byte	104, 105, 0
	.section	".rodata","a",@progbits
	.globl "types"
	.type	"types",@object
	.size "types", 8
	.p2align	0
"types":
	.byte	105, 100, 108, 112, 105, 100, 100, 0
	.section	".note.GNU-stack","",@progbits
