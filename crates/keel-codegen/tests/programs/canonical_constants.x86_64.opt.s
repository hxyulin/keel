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
	# v2 = iadd v0, v1
	add	rsi, 37
	# v3 = iconst.i64 63
	# v4 = xor v2, v3
	xor	rsi, 63
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
	# v2 = icmp.sgt v0, v1
	cmp	rsi, 37
	setg	dil
	# v3 = icmp.ule v0, v1
	cmp	rsi, 37
	setbe	sil
	# v4 = uext.i64 v2
	movzx	edi, dil
	# v5 = uext.i64 v3
	movzx	esi, sil
	# v6 = iconst.i64 1
	# v7 = shl v5, v6
	shl	rsi, 1
	# v8 = or v4, v7
	or	rdi, rsi
	# return v8
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lcanonical_compare_end:
	.cfi_endproc
	.size "canonical_compare", .-"canonical_compare"
	.section	".note.GNU-stack","",@progbits
