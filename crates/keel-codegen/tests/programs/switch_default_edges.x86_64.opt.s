	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "sparse_default"
	.p2align	4
	.type	"sparse_default",@function
"sparse_default":
.Lsparse_default_begin:
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
.Lsparse_default_block0:
	# v3 = iconst.i32 17
	mov	r9d, 17
	# v4 = iconst.i32 29
	mov	ebx, 29
	# switch v0, block1, [259: block2(v3), 260: block2(v4), 288: block2(v0)]
	cmp	esi, 259
	je	".Lsparse_default_block2"
	cmp	esi, 260
	je	".Lsparse_default_inst2_edge2"
	cmp	esi, 288
	je	".Lsparse_default_inst2_edge3"
	jmp	".Lsparse_default_block1"
.Lsparse_default_inst2_edge2:
	mov	r9, rbx
	jmp	".Lsparse_default_block2"
.Lsparse_default_inst2_edge3:
	mov	r9, rsi
	jmp	".Lsparse_default_block2"
.Lsparse_default_block1:
	# v5 = iconst.i32 41
	mov	esi, 41
	# jump block2(v5)
	mov	r9, rsi
.Lsparse_default_block2:
	# store v6, v1
	mov	dword ptr [rdi], r9d
	# v7 = load.i32 v2
	mov	esi, dword ptr [r8]
	# return v7
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lsparse_default_end:
	.cfi_endproc
	.size "sparse_default", .-"sparse_default"
	.section	".text","ax",@progbits
	.globl "trap_default"
	.p2align	4
	.type	"trap_default",@function
"trap_default":
.Ltrap_default_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Ltrap_default_block0:
	# v1 = iconst.i32 1
	mov	edi, 1
	# switch v0, block1, [259: block2(v1), 288: block2(v1)]
	cmp	esi, 259
	je	".Ltrap_default_block2"
	cmp	esi, 288
	je	".Ltrap_default_block2"
.Ltrap_default_block1:
	# v2 = iconst.i32 0
	xor	edi, edi
	# jump block2(v2)
.Ltrap_default_block2:
	# brif v3, block3, block4
	test	edi, edi
	je	".Ltrap_default_block4"
.Ltrap_default_block3:
	# return v0
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltrap_default_block4:
	# trap
	ud2
.Ltrap_default_end:
	.cfi_endproc
	.size "trap_default", .-"trap_default"
	.section	".note.GNU-stack","",@progbits
