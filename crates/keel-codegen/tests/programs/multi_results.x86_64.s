	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_swap"
	.p2align	4
	.type	"k_swap",@function
"k_swap":
.Lk_swap_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_swap_block0:
	# return v1, v0
	mov	rax, rdi
	mov	rdx, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_swap_end:
	.cfi_endproc
	.size "k_swap", .-"k_swap"
	.section	".text","ax",@progbits
	.globl "k_narrow"
	.p2align	4
	.type	"k_narrow",@function
"k_narrow":
.Lk_narrow_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_narrow_block0:
	# v2 = trunc.i8 v0
	# return v2, v1
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_narrow_end:
	.cfi_endproc
	.size "k_narrow", .-"k_narrow"
	.section	".text","ax",@progbits
	.globl "k_padded"
	.p2align	4
	.type	"k_padded",@function
"k_padded":
.Lk_padded_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_padded_block0:
	# v1 = trunc.i32 v0
	mov	rdi, rsi
	# v2 = iconst.i64 1
	# v3 = iadd v0, v2
	add	rsi, 1
	# return v3, v1
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_padded_end:
	.cfi_endproc
	.size "k_padded", .-"k_padded"
	.section	".text","ax",@progbits
	.globl "k_floats"
	.p2align	4
	.type	"k_floats",@function
"k_floats":
.Lk_floats_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_floats_block0:
	# v2 = fadd v0, v1
	movaps	xmm2, xmm0
	addsd	xmm2, xmm1
	# v3 = fsub v0, v1
	subsd	xmm0, xmm1
	# return v3, v2
	movaps	xmm1, xmm2
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_floats_end:
	.cfi_endproc
	.size "k_floats", .-"k_floats"
	.section	".text","ax",@progbits
	.globl "k_twice"
	.p2align	4
	.type	"k_twice",@function
"k_twice":
.Lk_twice_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_twice_block0:
	# return v0, v0
	mov	rax, rsi
	mov	rdx, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_twice_end:
	.cfi_endproc
	.size "k_twice", .-"k_twice"
	.section	".text","ax",@progbits
	.globl "k_call"
	.p2align	4
	.type	"k_call",@function
"k_call":
.Lk_call_begin:
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
	mov	rbx, rdi
.Lk_call_block0:
	# v2, v3 = call @c_pair(v0, v1)
	mov	rdi, rbx
	call	"c_pair"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# v4, v5 = call @c_pair(v3, v2)
	call	"c_pair"@PLT
	mov	r12, rax
	mov	r13, rdx
	# v6, v7 = call @c_pair(v4, v0)
	mov	rdi, r12
	mov	rsi, rbx
	call	"c_pair"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# v8 = iconst.i64 7
	# v9 = imul v4, v8
	imul	rsi, r12, 7
	# v10 = iconst.i64 11
	# v11 = imul v5, v10
	imul	r8, r13, 11
	# v12 = iadd v9, v11
	add	rsi, r8
	# v13 = iadd v12, v7
	add	rsi, rdi
	# v14 = iadd v13, v0
	add	rsi, rbx
	# return v14
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_call_end:
	.cfi_endproc
	.size "k_call", .-"k_call"
	.section	".text","ax",@progbits
	.globl "k_call_sized"
	.p2align	4
	.type	"k_call_sized",@function
"k_call_sized":
.Lk_call_sized_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_call_sized_block0:
	# v2, v3 = call @c_sized(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"c_sized"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# v4 = sext.i64 v2
	movsxd	rsi, esi
	# v5 = iconst.i64 100
	# v6 = imul v4, v5
	imul	rsi, rsi, 100
	# v7 = iadd v6, v3
	add	rsi, rdi
	# return v7
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_call_sized_end:
	.cfi_endproc
	.size "k_call_sized", .-"k_call_sized"
	.section	".text","ax",@progbits
	.globl "k_call_floats"
	.p2align	4
	.type	"k_call_floats",@function
"k_call_floats":
.Lk_call_floats_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_call_floats_block0:
	# v2, v3 = call @c_floats(v0, v1)
	call	"c_floats"@PLT
	# v4 = fconst.f64 3.0
	movabs	rax, 4613937818241073152
	movq	xmm2, rax
	# v5 = fmul v2, v4
	mulsd	xmm0, xmm2
	# v6 = fadd v5, v3
	addsd	xmm0, xmm1
	# return v6
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_call_floats_end:
	.cfi_endproc
	.size "k_call_floats", .-"k_call_floats"
	.section	".text","ax",@progbits
	.globl "k_indirect"
	.p2align	4
	.type	"k_indirect",@function
"k_indirect":
.Lk_indirect_begin:
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
.Lk_indirect_block0:
	# v3, v4 = call_indirect v0(v1, v2) : (i64, i64) -> (i64, i64) cc(c)
	mov	r10, rsi
	mov	rsi, r8
	call	r10
	mov	rsi, rax
	mov	rdi, rdx
	# v5 = iconst.i64 5
	# v6 = imul v3, v5
	imul	rsi, rsi, 5
	# v7 = isub v6, v4
	sub	rsi, rdi
	# return v7
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_indirect_end:
	.cfi_endproc
	.size "k_indirect", .-"k_indirect"
	.section	".text","ax",@progbits
	.globl "k_forward"
	.p2align	4
	.type	"k_forward",@function
"k_forward":
.Lk_forward_begin:
	.cfi_startproc
	.cfi_def_cfa 7, 8
	jmp	"c_pair"@PLT
.Lk_forward_end:
	.cfi_endproc
	.size "k_forward", .-"k_forward"
	.section	".text","ax",@progbits
	.globl "k_forward_swapped"
	.p2align	4
	.type	"k_forward_swapped",@function
"k_forward_swapped":
.Lk_forward_swapped_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_forward_swapped_block0:
	# return_call @c_pair(v1, v0)
	mov	rsp, rbp
	pop	rbp
	jmp	"c_pair"@PLT
.Lk_forward_swapped_end:
	.cfi_endproc
	.size "k_forward_swapped", .-"k_forward_swapped"
	.section	".text","ax",@progbits
	.p2align	4
	.type	"fast_many",@function
"fast_many":
.Lfast_many_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	movq	xmm14, rsi
	mov	rsi, rdi
	movq	rdi, xmm14
	mov	r13, qword ptr [rbp + 16]
.Lfast_many_block0:
	# return v6, v5, v4, v3, v2, v1, v0, v14, v13, v12, v11, v10, v9, v8, v7
	mov	rax, r13
	mov	rdx, r12
	mov	rcx, r8
	mov	r8, rdi
	mov	rdi, rbx
	movq	xmm14, rsi
	mov	rsi, r9
	movq	r9, xmm14
	movaps	xmm14, xmm0
	movaps	xmm0, xmm7
	movaps	xmm7, xmm14
	movaps	xmm14, xmm1
	movaps	xmm1, xmm6
	movaps	xmm6, xmm14
	movaps	xmm14, xmm2
	movaps	xmm2, xmm5
	movaps	xmm5, xmm14
	movaps	xmm14, xmm3
	movaps	xmm3, xmm4
	movaps	xmm4, xmm14
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lfast_many_end:
	.cfi_endproc
	.size "fast_many", .-"fast_many"
	.section	".text","ax",@progbits
	.globl "k_fast"
	.p2align	4
	.type	"k_fast",@function
"k_fast":
.Lk_fast_begin:
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
	sub	rsp, 136
	mov	rsi, rdi
.Lk_fast_block0:
	# v2 = iconst.i64 1
	# v3 = iconst.i64 2
	# v4 = iconst.i64 3
	# v5 = iconst.i64 4
	# v6 = iconst.i64 5
	# v7 = iconst.i64 6
	# v8 = iconst.i64 7
	# v9 = iadd v0, v2
	lea	rax, [rsi + 1]
	mov	qword ptr [rbp + -128], rax
	# v10 = iadd v0, v3
	lea	rax, [rsi + 2]
	mov	qword ptr [rbp + -120], rax
	# v11 = iadd v0, v4
	lea	rax, [rsi + 3]
	mov	qword ptr [rbp + -112], rax
	# v12 = iadd v0, v5
	lea	rdi, [rsi + 4]
	# v13 = iadd v0, v6
	lea	r8, [rsi + 5]
	# v14 = iadd v0, v7
	lea	r9, [rsi + 6]
	# v15 = iadd v0, v8
	add	rsi, 7
	# v16 = fconst.f64 1.0
	movabs	rax, 4607182418800017408
	movq	xmm1, rax
	# v17 = fconst.f64 2.0
	movabs	rax, 4611686018427387904
	movq	xmm13, rax
	movsd	qword ptr [rbp + -104], xmm13
	# v18 = fconst.f64 3.0
	movabs	rax, 4613937818241073152
	movq	xmm13, rax
	movsd	qword ptr [rbp + -96], xmm13
	# v19 = fconst.f64 4.0
	movabs	rax, 4616189618054758400
	movq	xmm13, rax
	movsd	qword ptr [rbp + -88], xmm13
	# v20 = fconst.f64 5.0
	movabs	rax, 4617315517961601024
	movq	xmm13, rax
	movsd	qword ptr [rbp + -80], xmm13
	# v21 = fconst.f64 6.0
	movabs	rax, 4618441417868443648
	movq	xmm13, rax
	movsd	qword ptr [rbp + -72], xmm13
	# v22 = fconst.f64 7.0
	movabs	rax, 4619567317775286272
	movq	xmm13, rax
	movsd	qword ptr [rbp + -64], xmm13
	# v23 = fconst.f64 8.0
	movabs	rax, 4620693217682128896
	movq	xmm13, rax
	movsd	qword ptr [rbp + -56], xmm13
	# v24 = fadd v1, v16
	movaps	xmm13, xmm0
	addsd	xmm13, xmm1
	movaps	xmm1, xmm13
	# v25 = fadd v1, v17
	movsd	xmm14, qword ptr [rbp + -104]
	movaps	xmm2, xmm0
	addsd	xmm2, xmm14
	# v26 = fadd v1, v18
	movsd	xmm14, qword ptr [rbp + -96]
	movaps	xmm3, xmm0
	addsd	xmm3, xmm14
	# v27 = fadd v1, v19
	movsd	xmm14, qword ptr [rbp + -88]
	movaps	xmm4, xmm0
	addsd	xmm4, xmm14
	# v28 = fadd v1, v20
	movsd	xmm14, qword ptr [rbp + -80]
	movaps	xmm5, xmm0
	addsd	xmm5, xmm14
	# v29 = fadd v1, v21
	movsd	xmm14, qword ptr [rbp + -72]
	movaps	xmm6, xmm0
	addsd	xmm6, xmm14
	# v30 = fadd v1, v22
	movsd	xmm14, qword ptr [rbp + -64]
	movaps	xmm7, xmm0
	addsd	xmm7, xmm14
	# v31 = fadd v1, v23
	movsd	xmm14, qword ptr [rbp + -56]
	addsd	xmm0, xmm14
	# v32, v33, v34, v35, v36, v37, v38, v39, v40, v41, v42, v43, v44, v45, v46 = call @fast_many(v9, v10, v11, v12, v13, v14, v15, v24, v25, v26, v27, v28, v29, v30, v31)
	mov	qword ptr [rsp], rsi
	mov	rsi, qword ptr [rbp + -120]
	mov	rdx, qword ptr [rbp + -112]
	mov	rcx, rdi
	mov	rdi, qword ptr [rbp + -128]
	movaps	xmm14, xmm0
	movaps	xmm0, xmm1
	movaps	xmm1, xmm2
	movaps	xmm2, xmm3
	movaps	xmm3, xmm4
	movaps	xmm4, xmm5
	movaps	xmm5, xmm6
	movaps	xmm6, xmm7
	movaps	xmm7, xmm14
	call	"fast_many"
	mov	r15, rcx
	mov	r14, r8
	mov	r8, rdi
	mov	rdi, rdx
	mov	r13, r9
	mov	r9, rsi
	mov	rsi, rax
	# v47 = imul v33, v3
	imul	rdi, rdi, 2
	# v48 = imul v34, v4
	imul	r8, r8, 3
	# v49 = imul v35, v5
	imul	r9, r9, 4
	# v50 = imul v36, v6
	imul	rbx, r15, 5
	# v51 = imul v37, v7
	imul	r12, r14, 6
	# v52 = imul v38, v8
	imul	r13, r13, 7
	# v53 = iadd v32, v47
	add	rsi, rdi
	# v54 = iadd v53, v48
	add	rsi, r8
	# v55 = iadd v54, v49
	add	rsi, r9
	# v56 = iadd v55, v50
	add	rsi, rbx
	# v57 = iadd v56, v51
	add	rsi, r12
	# v58 = iadd v57, v52
	add	rsi, r13
	# v59 = fmul v40, v17
	movsd	xmm14, qword ptr [rbp + -104]
	mulsd	xmm1, xmm14
	# v60 = fmul v41, v18
	movsd	xmm14, qword ptr [rbp + -96]
	mulsd	xmm2, xmm14
	# v61 = fmul v42, v19
	movsd	xmm14, qword ptr [rbp + -88]
	mulsd	xmm3, xmm14
	# v62 = fmul v43, v20
	movsd	xmm14, qword ptr [rbp + -80]
	mulsd	xmm4, xmm14
	# v63 = fmul v44, v21
	movsd	xmm14, qword ptr [rbp + -72]
	mulsd	xmm5, xmm14
	# v64 = fmul v45, v22
	movsd	xmm14, qword ptr [rbp + -64]
	mulsd	xmm6, xmm14
	# v65 = fmul v46, v23
	movsd	xmm14, qword ptr [rbp + -56]
	mulsd	xmm7, xmm14
	# v66 = fadd v39, v59
	addsd	xmm0, xmm1
	# v67 = fadd v66, v60
	addsd	xmm0, xmm2
	# v68 = fadd v67, v61
	addsd	xmm0, xmm3
	# v69 = fadd v68, v62
	addsd	xmm0, xmm4
	# v70 = fadd v69, v63
	addsd	xmm0, xmm5
	# v71 = fadd v70, v64
	addsd	xmm0, xmm6
	# v72 = fadd v71, v65
	addsd	xmm0, xmm7
	# v73 = fptosi.i64 v72
	cvttsd2si	rax, xmm0
	movabs	r11, 4890909195324358656
	movq	xmm14, r11
	ucomisd	xmm0, xmm14
	movabs	rcx, 9223372036854775807
	cmovae	rax, rcx
	mov	ecx, 0
	cmovp	rax, rcx
	mov	rdi, rax
	# v74 = iconst.i64 1000000
	# v75 = imul v58, v74
	imul	rsi, rsi, 1000000
	# v76 = iadd v75, v73
	add	rsi, rdi
	# return v76
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_fast_end:
	.cfi_endproc
	.size "k_fast", .-"k_fast"
	.section	".note.GNU-stack","",@progbits
