	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "scalar_simplify"
	.p2align	4
	.type	"scalar_simplify",@function
"scalar_simplify":
.Lscalar_simplify_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lscalar_simplify_block0:
	# v4 = xor v0, v1
	xor	rsi, rdi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lscalar_simplify_end:
	.cfi_endproc
	.size "scalar_simplify", .-"scalar_simplify"
	.section	".text","ax",@progbits
	.globl "scalar_casts"
	.p2align	4
	.type	"scalar_casts",@function
"scalar_casts":
.Lscalar_casts_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lscalar_casts_block0:
	# v1 = trunc.i8 v0
	# v5 = uext.i64 v1
	movzx	esi, sil
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lscalar_casts_end:
	.cfi_endproc
	.size "scalar_casts", .-"scalar_casts"
	.section	".text","ax",@progbits
	.globl "scalar_mixed_casts"
	.p2align	4
	.type	"scalar_mixed_casts",@function
"scalar_mixed_casts":
.Lscalar_mixed_casts_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lscalar_mixed_casts_block0:
	# v1 = trunc.i8 v0
	# v2 = sext.i16 v1
	movsx	esi, sil
	# v3 = uext.i64 v2
	movzx	esi, si
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lscalar_mixed_casts_end:
	.cfi_endproc
	.size "scalar_mixed_casts", .-"scalar_mixed_casts"
	.section	".text","ax",@progbits
	.globl "scalar_algebra"
	.p2align	4
	.type	"scalar_algebra",@function
"scalar_algebra":
.Lscalar_algebra_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lscalar_algebra_block0:
	# v8 = iconst.i64 44
	# v7 = iadd v1, v8
	add	rsi, 44
	# return v7
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lscalar_algebra_end:
	.cfi_endproc
	.size "scalar_algebra", .-"scalar_algebra"
	.section	".note.GNU-stack","",@progbits
