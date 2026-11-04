	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "known_bits"
	.p2align	4
	.type	"known_bits",@function
"known_bits":
.Lknown_bits_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lknown_bits_block0:
	# v1 = trunc.i8 v0
	# v2 = uext.i64 v1
	movzx	esi, sil
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lknown_bits_end:
	.cfi_endproc
	.size "known_bits", .-"known_bits"
	.section	".text","ax",@progbits
	.globl "known_add"
	.p2align	4
	.type	"known_add",@function
"known_add":
.Lknown_add_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lknown_add_block0:
	# v1 = iconst.i64 15
	# v2 = and v0, v1
	and	rsi, 15
	# v3 = uext.i128 v2
	mov	rax, rsi
	xor	edx, edx
	mov	rsi, rax
	mov	rdi, rdx
	# v4 = iconst.i128 16
	mov	eax, 16
	xor	edx, edx
	mov	r8, rax
	mov	r9, rdx
	# v5 = iadd v3, v4
	mov	rax, rsi
	add	rax, r8
	mov	rdx, rdi
	adc	rdx, r9
	mov	rsi, rax
	mov	rdi, rdx
	# v8 = trunc.i64 v5
	# return v8
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lknown_add_end:
	.cfi_endproc
	.size "known_add", .-"known_add"
	.section	".note.GNU-stack","",@progbits
