	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "memory_flags"
	.p2align	4
	.type	"memory_flags",@function
"memory_flags":
.Lmemory_flags_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lmemory_flags_block0:
	# v1 = load.i64 volatile align 8 v0
	mov	rdi, qword ptr [rsi]
	# v2 = iconst.i64 1
	# v3 = iadd v1, v2
	add	rdi, 1
	# store volatile align 8 v3, v0
	mov	qword ptr [rsi], rdi
	# v4 = load.i64 align 8 notrap v0
	mov	rsi, qword ptr [rsi]
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmemory_flags_end:
	.cfi_endproc
	.size "memory_flags", .-"memory_flags"
	.section	".note.GNU-stack","",@progbits
