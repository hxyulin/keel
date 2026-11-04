	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_checked_fold
	.p2align	2
_checked_fold:
Lchecked_fold_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9 v5=x10 v6=x10 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lchecked_fold_block0:
	; v0 = iconst.i64 9223372036854775807
	movn	x9, #0x8000, lsl #48
	; v1 = iconst.i64 1
	movz	x10, #0x1
	; jump block1(v0, v1)
Lchecked_fold_block1:
	; v4, v5 = sadd_overflow v2, v3
	adds	x16, x9, x10
	cset	w8, vs
	mov	x9, x16
	mov	x10, x8
	; v6 = uext.i64 v5
	uxtb	w10, w10
	; v7 = xor v4, v6
	eor	x9, x9, x10
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lchecked_fold_end:
	.cfi_endproc
	.subsections_via_symbols
