	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_constant_loop
	.p2align	2
_constant_loop:
Lconstant_loop_begin:
	; regalloc: v0=x9 v1=x10 v3=x10 v4=x11 v8=x11 v10=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lconstant_loop_block0:
	; v1 = iconst.i64 0
	movz	x10, #0x0
	; jump block1(v1, v1)
	mov	x11, x10
Lconstant_loop_block1:
	; v6 = icmp.ult v3, v0
	; brif v6, block2, block3
	cmp	x10, x9
	b.lo	Lconstant_loop_block2
Lconstant_loop_block3:
	; return v4
	mov	x0, x11
	ldp	x29, x30, [sp], #16
	ret
Lconstant_loop_block2:
	; v7 = iconst.i64 49
	; v8 = iadd v4, v7
	add	x11, x11, #49
	; v9 = iconst.i64 1
	; v10 = iadd v3, v9
	add	x10, x10, #1
	; jump block1(v10, v8)
	b	Lconstant_loop_block1
Lconstant_loop_end:
	.cfi_endproc
	.subsections_via_symbols
