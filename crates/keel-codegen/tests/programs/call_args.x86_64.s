	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "weigh"
	.p2align	4
	.type	"weigh",@function
"weigh":
.Lweigh_begin:
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
	sub	rsp, 8
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
	mov	r13, qword ptr [rbp + 16]
	mov	r14, qword ptr [rbp + 24]
.Lweigh_block0:
	# v8 = iconst.i64 2
	# v9 = imul v1, v8
	imul	rdi, rdi, 2
	# v10 = iadd v0, v9
	add	rsi, rdi
	# v11 = iconst.i64 3
	# v12 = imul v2, v11
	imul	rdi, r8, 3
	# v13 = iadd v10, v12
	add	rsi, rdi
	# v14 = iconst.i64 4
	# v15 = imul v3, v14
	imul	rdi, r9, 4
	# v16 = iadd v13, v15
	add	rsi, rdi
	# v17 = iconst.i64 5
	# v18 = imul v4, v17
	imul	rdi, rbx, 5
	# v19 = iadd v16, v18
	add	rsi, rdi
	# v20 = iconst.i64 6
	# v21 = imul v5, v20
	imul	rdi, r12, 6
	# v22 = iadd v19, v21
	add	rsi, rdi
	# v23 = iconst.i64 7
	# v24 = imul v6, v23
	imul	rdi, r13, 7
	# v25 = iadd v22, v24
	add	rsi, rdi
	# v26 = iconst.i64 8
	# v27 = imul v7, v26
	imul	rdi, r14, 8
	# v28 = iadd v25, v27
	add	rsi, rdi
	# return v28
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lweigh_end:
	.cfi_endproc
	.size "weigh", .-"weigh"
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
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lmixed_block0:
	# v4 = load.i64 v2
	mov	r8, qword ptr [r8]
	# v5 = sext.i64 v0
	movsxd	rsi, esi
	# v6 = sext.i64 v1
	movsx	rdi, dil
	# v7 = iadd v4, v5
	add	rsi, r8
	# v8 = iadd v7, v6
	add	rsi, rdi
	# v9 = iadd v8, v3
	add	rsi, r9
	# return v9
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmixed_end:
	.cfi_endproc
	.size "mixed", .-"mixed"
	.section	".text","ax",@progbits
	.globl "poke"
	.p2align	4
	.type	"poke",@function
"poke":
.Lpoke_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lpoke_block0:
	# store v1, v0
	mov	qword ptr [rsi], rdi
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lpoke_end:
	.cfi_endproc
	.size "poke", .-"poke"
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
	push	r12
	.cfi_offset 12, -32
	push	r13
	.cfi_offset 13, -40
	push	r14
	.cfi_offset 14, -48
	push	r15
	.cfi_offset 15, -56
	sub	rsp, 88
.Lmain_block0:
	# v0 = iconst.i32 0
	xor	eax, eax
	mov	qword ptr [rbp + -112], rax
	# v1 = iconst.i64 1
	mov	eax, 1
	mov	qword ptr [rbp + -104], rax
	# v2 = iconst.i64 2
	mov	eax, 2
	mov	qword ptr [rbp + -96], rax
	# v3 = iconst.i64 3
	mov	eax, 3
	mov	qword ptr [rbp + -88], rax
	# v4 = iconst.i64 4
	mov	eax, 4
	mov	qword ptr [rbp + -80], rax
	# v5 = iconst.i64 5
	mov	ebx, 5
	# v6 = iconst.i64 6
	mov	r12d, 6
	# v7 = iconst.i64 7
	mov	r13d, 7
	# v8 = iconst.i64 8
	mov	r14d, 8
	# v9 = call @weigh(v1, v2, v3, v4, v5, v6, v7, v8)
	mov	qword ptr [rsp], r13
	mov	qword ptr [rsp + 8], r14
	mov	rdi, qword ptr [rbp + -104]
	mov	rsi, qword ptr [rbp + -96]
	mov	rdx, qword ptr [rbp + -88]
	mov	rcx, qword ptr [rbp + -80]
	mov	r8, rbx
	mov	r9, r12
	call	"weigh"
	mov	qword ptr [rbp + -72], rax
	# v10 = iconst.i64 204
	# v11 = icmp.ne v9, v10
	mov	rax, qword ptr [rbp + -72]
	cmp	rax, 204
	setne	sil
	# v12 = iconst.i32 1
	mov	edi, 1
	# v13 = select v11, v12, v0
	test	sil, sil
	mov	rdx, qword ptr [rbp + -112]
	mov	rax, rdx
	cmovne	rax, rdi
	mov	r15, rax
	# v14 = call @weigh(v8, v7, v6, v5, v4, v3, v2, v1)
	mov	rax, qword ptr [rbp + -96]
	mov	qword ptr [rsp], rax
	mov	rax, qword ptr [rbp + -104]
	mov	qword ptr [rsp + 8], rax
	mov	rdi, r14
	mov	rsi, r13
	mov	rdx, r12
	mov	rcx, rbx
	mov	r8, qword ptr [rbp + -80]
	mov	r9, qword ptr [rbp + -88]
	call	"weigh"
	mov	rsi, rax
	# v15 = iconst.i64 120
	# v16 = icmp.ne v14, v15
	cmp	rsi, 120
	setne	sil
	# v17 = iconst.i32 2
	mov	edi, 2
	# v18 = select v16, v17, v13
	test	sil, sil
	mov	rax, r15
	cmovne	rax, rdi
	mov	rbx, rax
	# v19 = stack_addr ss0
	lea	r12, [rbp + -56]
	# v20 = iconst.i64 1000
	mov	esi, 1000
	# call @poke(v19, v20)
	mov	rdi, r12
	call	"poke"
	# v21 = iconst.i32 -5
	mov	esi, 4294967291
	# v22 = iconst.i8 -3
	mov	edi, 253
	# v23 = iconst.i64 100000
	mov	r8d, 100000
	# v24 = call @mixed(v21, v22, v19, v23)
	mov	rdx, r12
	mov	rcx, r8
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"mixed"
	mov	rsi, rax
	# v25 = iconst.i64 100992
	# v26 = icmp.ne v24, v25
	cmp	rsi, 100992
	setne	sil
	# v27 = iconst.i32 4
	mov	edi, 4
	# v28 = select v26, v27, v18
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rsi, rax
	# v29 = icmp.ne v9, v10
	mov	rax, qword ptr [rbp + -72]
	cmp	rax, 204
	setne	dil
	# v30 = iconst.i32 5
	mov	r8d, 5
	# v31 = select v29, v30, v28
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# return v31
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
