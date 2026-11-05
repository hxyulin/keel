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
	sub	rsp, 40
	mov	rsi, rdi
.Lcompact_frame_block0:
	# v1 = stack_addr ss1
	lea	rdi, [rbp + -48]
	# v2 = stack_addr ss3
	lea	r8, [rbp + -24]
	# store notrap v0, v1
	mov	qword ptr [rbp + -48], rsi
	# v3 = iconst.i64 17
	# store notrap v3, v2
	mov	qword ptr [rbp + -24], 17
	# v4 = iconst.i64 64
	mov	esi, 64
	# v5 = intrinsic stack_alloc(v4)
	lea	rax, [rsi + 15]
	and	rax, -16
	sub	rsp, rax
	lea	rbx, [rsp]
	# store v3, v5
	mov	qword ptr [rbx], 17
	# v7 = call @touch_frame(v1, v2)
	mov	rsi, r8
	call	"touch_frame"@PLT
	mov	rsi, rax
	# v8 = load.i64 notrap v1
	mov	rdi, qword ptr [rbp + -48]
	# v9 = load.i64 notrap v2
	mov	r8, qword ptr [rbp + -24]
	# v10 = load.i64 v5
	mov	r9, qword ptr [rbx]
	# v11 = iadd v8, v9
	add	rdi, r8
	# v12 = iadd v10, v11
	add	r9, rdi
	# v13 = iadd v7, v12
	add	rsi, r9
	# return v13
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lcompact_frame_end:
	.cfi_endproc
	.size "compact_frame", .-"compact_frame"
	.section	".note.GNU-stack","",@progbits
