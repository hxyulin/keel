	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_scalar_simplify
	.p2align	2
_scalar_simplify:
Lscalar_simplify_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x11 v4=x10 v5=x9 v6=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lscalar_simplify_block0:
	; v2 = or v0, v1
	orr	x11, x9, x10
	; v3 = and v2, v0
	and	x11, x11, x9
	; v4 = xor v1, v0
	eor	x10, x10, x9
	; v5 = xor v4, v0
	eor	x9, x10, x9
	; v6 = xor v3, v5
	eor	x9, x11, x9
	; return v6
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lscalar_simplify_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_scalar_casts
	.p2align	2
_scalar_casts:
Lscalar_casts_begin:
	; regalloc: v0=x9 v1=x9 v2=x9 v3=x9:x10 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lscalar_casts_block0:
	; v1 = trunc.i8 v0
	; v2 = uext.i16 v1
	uxtb	w9, w9
	; v3 = sext.i128 v2
	mov	x0, x9
	sxth	x4, w0
	asr	x5, x4, #63
	mov	x9, x4
	mov	x10, x5
	; v4 = trunc.i32 v3
	; v5 = uext.i64 v4
	mov	w9, w9
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lscalar_casts_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_scalar_mixed_casts
	.p2align	2
_scalar_mixed_casts:
Lscalar_mixed_casts_begin:
	; regalloc: v0=x9 v1=x9 v2=x9 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lscalar_mixed_casts_block0:
	; v1 = trunc.i8 v0
	; v2 = sext.i16 v1
	sxtb	w9, w9
	; v3 = uext.i64 v2
	uxth	w9, w9
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lscalar_mixed_casts_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_scalar_algebra
	.p2align	2
_scalar_algebra:
Lscalar_algebra_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v3=x9 v4=x10 v5=x9 v6=x10 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lscalar_algebra_block0:
	; v2 = iadd v0, v1
	add	x10, x9, x10
	; v3 = isub v2, v0
	sub	x9, x10, x9
	; v4 = iconst.i64 37
	; v5 = iadd v3, v4
	add	x9, x9, #37
	; v6 = iconst.i64 7
	; v7 = iadd v5, v6
	add	x9, x9, #7
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lscalar_algebra_end:
	.cfi_endproc
	.subsections_via_symbols
