	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_sum
	.p2align	2
_sum:
Lsum_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x11 v4=x12 v5=x13 v6=x13 v7=x13 v8=x13 v9=x13 v10=x13 v11=x12 v12=x13 v13=x11 v14=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lsum_block0:
	; v2 = iconst.i64 0
	movz	x11, #0x0
	; jump block1(v2, v2)
	mov	x12, x11
Lsum_block1:
	; v5 = icmp.ult v3, v1
	cmp	x11, x10
	cset	w13, lo
	; brif v5, block2, block3(v4)
	tst	w13, #255
	b.ne	Lsum_block2
	mov	x9, x12
Lsum_block3:
	; return v14
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsum_block2:
	; v6 = iconst.i64 4
	movz	x13, #0x4
	; v7 = imul v3, v6
	mul	x13, x11, x13
	; v8 = ptradd v0, v7
	add	x13, x9, x13
	; v9 = load.i32 v8
	ldr	w13, [x13]
	; v10 = sext.i64 v9
	sxtw	x13, w13
	; v11 = iadd v4, v10
	add	x12, x12, x13
	; v12 = iconst.i64 1
	; v13 = iadd v3, v12
	add	x11, x11, #1
	; jump block1(v13, v11)
	b	Lsum_block1
Lsum_end:
	.cfi_endproc
	.subsections_via_symbols
