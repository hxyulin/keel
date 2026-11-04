	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x19 v1=x9 v2=x9 v3=x9 v4=x20 v6=x10 v7=x10 v8=x10 v9=x10 v10=x10 v11=x10 v12=x11 v13=x9 v14=x9 v15=x10 v16=x10 v17=x10 v18=x11 v19=x10 v20=x10 v21=x11 v22=x9 v23=x9 v25=x9 v26=x10 v27=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	sub	sp, sp, #48
Lmain_block0:
	; v0 = stack_addr ss0
	add	x19, sp, #0
	; v1 = iconst.i64 10
	movz	x9, #0xa
	; call @fill_squares(v0, v1)
	mov	x0, x19
	mov	x1, x9
	bl	_fill_squares
	; v2 = iconst.i64 0
	movz	x9, #0x0
	; jump block1(v2, v2)
	mov	x20, x9
Lmain_block1:
	; v5 = icmp.ult v3, v1
	; brif v5, block2, block3
	cmp	x9, #10
	b.lo	Lmain_block2
Lmain_block3:
	; v14 = stack_addr ss1
	add	x9, sp, #40
	; v15 = iconst.i8 111
	movz	x10, #0x6f
	; store v15, v14
	strb	w10, [sp, #40]
	; v16 = iconst.i64 1
	; v17 = ptradd v14, v16
	add	x10, x9, #1
	; v18 = iconst.i8 107
	movz	x11, #0x6b
	; store v18, v17
	strb	w11, [x10]
	; v19 = iconst.i64 2
	; v20 = ptradd v14, v19
	add	x10, x9, #2
	; v21 = iconst.i8 0
	; store v21, v20
	strb	wzr, [x10]
	; v22 = call @puts(v14)
	mov	x0, x9
	bl	_puts
	mov	x9, x0
	; v23 = iconst.i64 285
	; v24 = icmp.ne v4, v23
	; v25 = iconst.i32 1
	movz	x9, #0x1
	; v26 = iconst.i32 0
	movz	x10, #0x0
	; v27 = select v24, v25, v26
	cmp	x20, #285
	csel	w9, w9, w10, ne
	; return v27
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lmain_block2:
	; v6 = iconst.i64 4
	movz	x10, #0x4
	; v7 = imul v3, v6
	mul	x10, x9, x10
	; v8 = ptradd v0, v7
	add	x10, x19, x10
	; v9 = load.i32 v8
	ldr	w10, [x10]
	; v10 = sext.i64 v9
	sxtw	x10, w10
	; v11 = iadd v4, v10
	add	x10, x20, x10
	; v12 = iconst.i64 1
	; v13 = iadd v3, v12
	add	x9, x9, #1
	; jump block1(v13, v11)
	mov	x20, x10
	b	Lmain_block1
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
