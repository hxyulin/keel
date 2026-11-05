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
	# v1 = iconst.i64 1000
	mov	r12d, 1000
	# v2 = imul v0, v1
	imul	rax, rbx, 1000
	mov	qword ptr [rbp + -80], rax
	# v3 = iconst.i64 8
	mov	r14d, 8
	# v4 = imul v0, v3
	imul	rsi, rbx, 8
	# v5 = intrinsic stack_alloc(v4)
	lea	rax, [rsi + 15]
	and	rax, -16
	sub	rsp, rax
	lea	r15, [rsp + 32]
	# call @c_fill(v5, v0)
	mov	rdi, r15
	mov	rsi, rbx
	call	"c_fill"@PLT
	# v6 = iconst.i64 0
	xor	esi, esi
	# jump block1(v6, v6)
	mov	rdi, rsi
	mov	r13, rsi
.Lk_vla_block1:
	# v9 = icmp.ult v7, v0
	# brif v9, block2, block3
	cmp	rdi, rbx
	jb	".Lk_vla_block2"
.Lk_vla_block3:
	# v16 = iconst.i64 2
	mov	edi, 2
	# v17 = iconst.i64 3
	mov	r8d, 3
	# v18 = call @c_many(v5, v0, v1, v3, v6, v16, v17, v2, v0, v1)
	mov	qword ptr [rsp], r8
	mov	rax, qword ptr [rbp + -80]
	mov	qword ptr [rsp + 8], rax
	mov	qword ptr [rsp + 16], rbx
	mov	qword ptr [rsp + 24], r12
	mov	rdx, r12
	mov	rcx, r14
	mov	r8, rsi
	mov	rsi, rbx
	mov	r9, rdi
	mov	rdi, r15
	call	"c_many"@PLT
	mov	r12, rax
	# v19 = stack_addr ss0
	lea	rsi, [rbp + -72]
	# store v0, v19
	mov	qword ptr [rbp + -72], rbx
	# v20 = ptradd v19, v3
	lea	rdi, [rsi + 8]
	# store v2, v20
	mov	rax, qword ptr [rbp + -80]
	mov	qword ptr [rdi], rax
	# v21 = ptradd v20, v3
	add	rdi, 8
	# store v8, v21
	mov	qword ptr [rdi], r13
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
	lea	rdi, [r13 + r12 * 1]
	# v24 = iadd v23, v22
	add	rsi, rdi
	# v25 = iadd v24, v2
	mov	rcx, qword ptr [rbp + -80]
	add	rsi, rcx
	# return v25
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_vla_block2:
	# v10 = imul v7, v3
	imul	r8, rdi, 8
	# v11 = ptradd v5, v10
	add	r8, r15
	# v12 = load.i64 v11
	mov	r8, qword ptr [r8]
	# v13 = iadd v8, v12
	add	r8, r13
	# v14 = iconst.i64 1
	# v15 = iadd v7, v14
	add	rdi, 1
	# jump block1(v15, v13)
	mov	r13, r8
	jmp	".Lk_vla_block1"
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
	# jump block1(v1, v1)
	mov	r8, rdi
.Lk_loop_block1:
	# v4 = icmp.ult v2, v0
	# brif v4, block2, block3
	cmp	rdi, rsi
	jb	".Lk_loop_block2"
.Lk_loop_block3:
	# return v3
	mov	rax, r8
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
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
	# store v2, v7
	mov	qword ptr [rbx], rdi
	# v8 = load.i64 v7
	mov	rbx, qword ptr [rbx]
	# v9 = iadd v3, v8
	add	r8, rbx
	# intrinsic stack_restore(v5)
	mov	rsp, r9
	# v10 = iconst.i64 1
	# v11 = iadd v2, v10
	add	rdi, 1
	# jump block1(v11, v9)
	jmp	".Lk_loop_block1"
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
	# v4 = iadd v3, v2
	add	rsi, rbx
	# return v4
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_outer_end:
	.cfi_endproc
	.size "k_outer", .-"k_outer"
	.section	".note.GNU-stack","",@progbits
