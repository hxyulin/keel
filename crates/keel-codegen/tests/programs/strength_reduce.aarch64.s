	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_strength
	.p2align	2
_strength:
Lstrength_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10 v5=x9 v6=x10 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lstrength_block0:
	; v1 = iconst.i64 8
	movz	x10, #0x8
	; v2 = udiv v0, v1
	lsr	x11, x9, #3
	; v3 = urem v0, v1
	and	x9, x9, #7
	; v4 = imul v2, v1
	mul	x10, x11, x10
	; v5 = iadd v4, v3
	add	x9, x10, x9
	; v6 = iconst.i64 0
	; v7 = iadd v5, v6
	add	x9, x9, #0
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lstrength_end:
	.cfi_endproc
	.subsections_via_symbols
