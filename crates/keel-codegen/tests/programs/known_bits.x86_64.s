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
	push	rbx
	.cfi_offset 3, -24
	push	r12
	.cfi_offset 12, -32
	mov	rsi, rdi
.Lknown_bits_block0:
	# v1 = trunc.i8 v0
	mov	rdi, rsi
	# v2 = uext.i64 v1
	movzx	r8d, dil
	# v3 = iconst.i64 255
	# v4 = and v2, v3
	and	r8, 255
	# v5 = iconst.i64 256
	# v6 = icmp.ult v4, v5
	cmp	r8, 256
	setb	r9b
	# brif v6, block1, block4
	test	r9b, r9b
	je	".Lknown_bits_block4"
.Lknown_bits_block1:
	# v7 = uext.i128 v1
	movzx	eax, dil
	xor	edx, edx
	mov	rdi, rax
	mov	r9, rdx
	# v8 = iconst.i128 256
	mov	eax, 256
	xor	edx, edx
	mov	rbx, rax
	mov	r12, rdx
	# v9 = icmp.ult v7, v8
	mov	rax, rdi
	cmp	rax, rbx
	mov	rdx, r9
	sbb	rdx, r12
	setb	dil
	# brif v9, block2, block4
	test	dil, dil
	je	".Lknown_bits_block4"
.Lknown_bits_block2:
	# v10 = iconst.i64 15
	# v11 = and v0, v10
	and	rsi, 15
	# v12 = iconst.i64 60
	# v13 = shl v11, v12
	shl	rsi, 60
	# v14 = ushr v13, v12
	shr	rsi, 60
	# v15 = icmp.ule v14, v10
	cmp	rsi, 15
	setbe	sil
	# brif v15, block3, block4
	test	sil, sil
	jne	".Lknown_bits_block3"
.Lknown_bits_block4:
	# trap
	ud2
.Lknown_bits_block3:
	# return v4
	mov	rax, r8
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
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
	# v6 = iconst.i128 32
	mov	eax, 32
	xor	edx, edx
	mov	r8, rax
	mov	r9, rdx
	# v7 = icmp.ult v5, v6
	mov	rax, rsi
	cmp	rax, r8
	mov	rdx, rdi
	sbb	rdx, r9
	setb	r8b
	# brif v7, block1, block2
	test	r8b, r8b
	jne	".Lknown_add_block1"
.Lknown_add_block2:
	# trap
	ud2
.Lknown_add_block1:
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
