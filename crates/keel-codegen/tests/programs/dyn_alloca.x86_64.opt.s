	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_vla"
	.p2align	4
	.type	"k_vla",@function
"k_vla":
.Lk_vla_begin:
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
	mov	rbx, rdi
.Lk_vla_block0:
	# v26 = iconst.i64 3
	# v4 = shl v0, v26
	mov	r12, rbx
	shl	r12, 3
	# v5 = intrinsic stack_alloc(v4)
	lea	rax, [r12 + 15]
	and	rax, -16
	sub	rsp, rax
	lea	r13, [rsp + 32]
	# call @c_fill(v5, v0)
	mov	rdi, r13
	mov	rsi, rbx
	call	"c_fill"@PLT
	# v6 = iconst.i64 0
	xor	esi, esi
	# v33 = iconst.i64 8
	# v34 = icmp.uge v0, v33
	# brif v34, block7, block1(v6, v6)
	cmp	rbx, 8
	jae	".Lk_vla_block7"
	mov	r12, rsi
	jmp	".Lk_vla_block1"
.Lk_vla_block7:
	# v79 = iconst.i64 2305843009213693951
	movabs	rsi, 2305843009213693951
	# v37 = and v0, v79
	and	rsi, rbx
	# v38 = icmp.eq v0, v37
	cmp	rbx, rsi
	sete	sil
	# v39 = ptrtoint.i64 v5
	mov	rdi, r13
	# v40 = iadd v4, v39
	add	r12, rdi
	# v41 = icmp.ule v39, v40
	cmp	rdi, r12
	setbe	dil
	# v42 = and v38, v41
	and	esi, edi
	# v89 = iconst.i64 0
	xor	edi, edi
	# brif v42, block5, block1(v89, v89)
	test	sil, sil
	jne	".Lk_vla_block5"
	mov	rsi, rdi
	mov	r12, rdi
	jmp	".Lk_vla_block1"
.Lk_vla_block5:
	# v78 = iconst.i64 -8
	# v45 = and v0, v78
	mov	rsi, rbx
	and	rsi, -8
	# v90 = iconst.i64 0
	xor	edi, edi
	# v47 = vsplat.i64x2 v90
	movq	xmm0, rdi
	punpcklqdq	xmm0, xmm0
	# jump block6(v45, v47, v47, v47, v47, v5)
	mov	rdi, rsi
	movaps	xmm1, xmm0
	movaps	xmm2, xmm0
	movaps	xmm3, xmm0
	mov	r8, r13
.Lk_vla_block6:
	# v53 = load.i64x2 v52
	movups	xmm4, xmmword ptr [r8]
	# v54 = viadd v48, v53
	paddq	xmm0, xmm4
	# v55 = iconst.i64 16
	# v56 = ptradd v52, v55
	lea	r9, [r8 + 16]
	# v57 = load.i64x2 v56
	movups	xmm4, xmmword ptr [r9]
	# v58 = viadd v49, v57
	paddq	xmm1, xmm4
	# v59 = iconst.i64 32
	# v60 = ptradd v52, v59
	lea	r9, [r8 + 32]
	# v61 = load.i64x2 v60
	movups	xmm4, xmmword ptr [r9]
	# v62 = viadd v50, v61
	paddq	xmm2, xmm4
	# v63 = iconst.i64 48
	# v64 = ptradd v52, v63
	lea	r9, [r8 + 48]
	# v65 = load.i64x2 v64
	movups	xmm4, xmmword ptr [r9]
	# v66 = viadd v51, v65
	paddq	xmm3, xmm4
	# v87 = iconst.i64 8
	# v68 = isub v31, v87
	mov	r9, rdi
	sub	r9, 8
	# v69 = iconst.i64 64
	# v70 = ptradd v52, v69
	add	r8, 64
	# v72 = icmp.ne v31, v87
	# brif v72, block6(v68, v54, v58, v62, v66, v70), block8
	cmp	rdi, 8
	je	".Lk_vla_block8"
	mov	rdi, r9
	jmp	".Lk_vla_block6"
.Lk_vla_block8:
	# v73 = viadd v54, v58
	paddq	xmm0, xmm1
	# v74 = viadd v73, v62
	paddq	xmm0, xmm2
	# v75 = viadd v74, v66
	paddq	xmm0, xmm3
	# v76 = vreduce_add v75
	movaps	xmm14, xmm0
	pshufd	xmm15, xmm14, 78
	paddq	xmm14, xmm15
	movq	rdi, xmm14
	# jump block1(v45, v76)
	mov	r12, rdi
.Lk_vla_block1:
	# v9 = icmp.ugt v0, v7
	# brif v9, block2, block3
	cmp	rbx, rsi
	jbe	".Lk_vla_block3"
.Lk_vla_block2:
	# v93 = iconst.i64 3
	# v10 = shl v7, v93
	# v11 = ptradd v5, v10
	# v12 = load.i64 v11
	# v13 = iadd v8, v12
	add	r12, qword ptr [r13 + rsi * 8]
	# v14 = iconst.i64 1
	# v15 = iadd v7, v14
	add	rsi, 1
	# jump block1(v15, v13)
	jmp	".Lk_vla_block1"
.Lk_vla_block3:
	# v16 = iconst.i64 2
	mov	esi, 2
	# v1 = iconst.i64 1000
	mov	edi, 1000
	# v2 = imul v0, v1
	imul	r14, rbx, 1000
	# v88 = iconst.i64 8
	mov	r8d, 8
	# v91 = iconst.i64 0
	xor	r9d, r9d
	# v92 = iconst.i64 3
	mov	r15d, 3
	# v18 = call @c_many(v5, v0, v1, v88, v91, v16, v92, v2, v0, v1)
	mov	qword ptr [rsp], r15
	mov	qword ptr [rsp + 8], r14
	mov	qword ptr [rsp + 16], rbx
	mov	qword ptr [rsp + 24], rdi
	mov	rdx, rdi
	mov	rdi, r13
	mov	rcx, r8
	mov	r8, r9
	mov	r9, rsi
	mov	rsi, rbx
	call	"c_many"@PLT
	mov	r13, rax
	# v19 = stack_addr ss0
	lea	rsi, [rbp + -64]
	# store notrap v0, v19
	mov	qword ptr [rbp + -64], rbx
	# v20 = ptradd v19, v88
	lea	rdi, [rsi + 8]
	# store notrap v2, v20
	mov	qword ptr [rdi], r14
	# v21 = ptradd v20, v88
	# store notrap v8, v21
	mov	qword ptr [rdi + 8], r12
	# v22 = call @c_big(v19)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rsp], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rsp + 8], rax
	mov	rax, qword ptr [rsi + 16]
	mov	qword ptr [rsp + 16], rax
	call	"c_big"@PLT
	mov	rsi, rax
	# v23 = iadd v8, v18
	add	r12, r13
	# v24 = iadd v22, v23
	add	rsi, r12
	# v25 = iadd v2, v24
	add	r14, rsi
	# return v25
	mov	rax, r14
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_vla_end:
	.cfi_endproc
	.size "k_vla", .-"k_vla"
	.section	".text","ax",@progbits
	.globl "k_loop"
	.p2align	4
	.type	"k_loop",@function
"k_loop":
.Lk_loop_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rsi, rdi
.Lk_loop_block0:
	# v1 = iconst.i64 0
	xor	edi, edi
	# v4 = icmp.ugt v0, v1
	# brif v4, block2(v1, v1), block3(v1)
	cmp	rsi, 0
	jbe	".Lk_loop_inst3_else"
	mov	r8, rdi
	jmp	".Lk_loop_block2"
.Lk_loop_inst3_else:
	mov	rsi, rdi
	jmp	".Lk_loop_block3"
.Lk_loop_block2:
	# v5 = intrinsic stack_save()
	mov	r9, rsp
	# v6 = iconst.i64 65536
	mov	ebx, 65536
	# v7 = intrinsic stack_alloc(v6)
	lea	rax, [rbx + 15]
	and	rax, -16
	sub	rsp, rax
	lea	rbx, [rsp]
	# store v12, v7
	mov	qword ptr [rbx], rdi
	# v8 = load.i64 v7
	# v9 = iadd v13, v8
	add	r8, qword ptr [rbx]
	# intrinsic stack_restore(v5)
	mov	rsp, r9
	# v10 = iconst.i64 1
	# v11 = iadd v12, v10
	add	rdi, 1
	# v16 = icmp.ugt v0, v11
	# brif v16, block2(v11, v9), block3(v9)
	cmp	rsi, rdi
	ja	".Lk_loop_block2"
	mov	rsi, r8
.Lk_loop_block3:
	# return v15
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_loop_end:
	.cfi_endproc
	.size "k_loop", .-"k_loop"
	.section	".text","ax",@progbits
	.globl "k_outer"
	.p2align	4
	.type	"k_outer",@function
"k_outer":
.Lk_outer_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rsi, rdi
.Lk_outer_block0:
	# v1 = iconst.i64 7
	# v2 = imul v0, v1
	imul	rbx, rsi, 7
	# v3 = call @k_vla(v0)
	mov	rdi, rsi
	call	"k_vla"
	mov	rsi, rax
	# v4 = iadd v2, v3
	add	rbx, rsi
	# return v4
	mov	rax, rbx
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_outer_end:
	.cfi_endproc
	.size "k_outer", .-"k_outer"
	.section	".note.GNU-stack","",@progbits
