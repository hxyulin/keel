	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "canonical_constants"
	.p2align	4
	.type	"canonical_constants",@function
"canonical_constants":
.Lcanonical_constants_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lcanonical_constants_block0:
	# v1 = iconst.i64 37
	mov	edi, 37
	# v2 = iadd v1, v0
	add	rsi, rdi
	# v3 = iconst.i64 63
	mov	edi, 63
	# v4 = xor v3, v2
	xor	rsi, rdi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lcanonical_constants_end:
	.cfi_endproc
	.size "canonical_constants", .-"canonical_constants"
	.section	".text","ax",@progbits
	.globl "canonical_compare"
	.p2align	4
	.type	"canonical_compare",@function
"canonical_compare":
.Lcanonical_compare_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lcanonical_compare_block0:
	# v1 = iconst.i64 37
	mov	edi, 37
	# v2 = icmp.slt v1, v0
	cmp	rdi, rsi
	setl	r8b
	# v3 = icmp.uge v1, v0
	cmp	rdi, rsi
	setae	sil
	# v4 = uext.i64 v2
	movzx	edi, r8b
	# v5 = uext.i64 v3
	movzx	esi, sil
	# v6 = iconst.i64 1
	# v7 = shl v5, v6
	shl	rsi, 1
	# v8 = or v7, v4
	or	rsi, rdi
	# return v8
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lcanonical_compare_end:
	.cfi_endproc
	.size "canonical_compare", .-"canonical_compare"
	.section	".note.GNU-stack","",@progbits
