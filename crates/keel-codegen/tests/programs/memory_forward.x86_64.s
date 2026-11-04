	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "memory_forward"
	.p2align	4
	.type	"memory_forward",@function
"memory_forward":
.Lmemory_forward_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lmemory_forward_block0:
	# v3 = load.i64 v0
	mov	r9, qword ptr [rsi]
	# v4 = load.i64 v0
	mov	rbx, qword ptr [rsi]
	# store v2, v1
	mov	qword ptr [rdi], r8
	# v5 = load.i64 v0
	mov	rdi, qword ptr [rsi]
	# store v5, v0
	mov	qword ptr [rsi], rdi
	# v6 = load.i64 v0
	mov	rsi, qword ptr [rsi]
	# v7 = iadd v3, v4
	lea	r8, [r9 + rbx * 1]
	# v8 = iadd v5, v6
	add	rsi, rdi
	# v9 = iadd v7, v8
	add	rsi, r8
	# return v9
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lmemory_forward_end:
	.cfi_endproc
	.size "memory_forward", .-"memory_forward"
	.section	".note.GNU-stack","",@progbits
