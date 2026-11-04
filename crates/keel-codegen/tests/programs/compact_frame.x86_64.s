	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "compact_frame"
	.p2align	4
	.type	"compact_frame",@function
"compact_frame":
.Lcompact_frame_begin:
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
	sub	rsp, 40
	mov	rsi, rdi
.Lcompact_frame_block0:
	# v1 = stack_addr ss1
	lea	rbx, [rbp + -64]
	# v2 = stack_addr ss3
	lea	r12, [rbp + -40]
	# store v0, v1
	mov	qword ptr [rbp + -64], rsi
	# v3 = iconst.i64 17
	# store v3, v2
	mov	qword ptr [rbp + -40], 17
	# v4 = iconst.i64 64
	mov	edi, 64
	# v5 = intrinsic stack_alloc(v4)
	lea	rax, [rdi + 15]
	and	rax, -16
	sub	rsp, rax
	lea	r13, [rsp]
	# store v3, v5
	mov	qword ptr [r13], 17
	# jump block1(v1)
	mov	rsi, rbx
.Lcompact_frame_block1:
	# v7 = call @touch_frame(v6, v2)
	mov	rdi, rsi
	mov	rsi, r12
	call	"touch_frame"@PLT
	mov	rsi, rax
	# v8 = load.i64 v1
	mov	rdi, qword ptr [rbp + -64]
	# v9 = load.i64 v2
	mov	r8, qword ptr [rbp + -40]
	# v10 = load.i64 v5
	mov	r9, qword ptr [r13]
	# v11 = iadd v8, v9
	add	rdi, r8
	# v12 = iadd v11, v10
	add	rdi, r9
	# v13 = iadd v12, v7
	add	rsi, rdi
	# return v13
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lcompact_frame_end:
	.cfi_endproc
	.size "compact_frame", .-"compact_frame"
	.section	".note.GNU-stack","",@progbits
