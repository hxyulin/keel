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
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lmemory_forward_block0:
	# v3 = load.i64 v0
	mov	r9, qword ptr [rsi]
	# store v2, v1
	mov	qword ptr [rdi], r8
	# v5 = load.i64 v0
	mov	rdi, qword ptr [rsi]
	# store v5, v0
	mov	qword ptr [rsi], rdi
	# v7 = iadd v3, v3
	add	r9, r9
	# v8 = iadd v5, v5
	add	rdi, rdi
	# v9 = iadd v7, v8
	add	r9, rdi
	# return v9
	mov	rax, r9
	mov	rsp, rbp
	pop	rbp
	ret
.Lmemory_forward_end:
	.cfi_endproc
	.size "memory_forward", .-"memory_forward"
	.section	".note.GNU-stack","",@progbits
