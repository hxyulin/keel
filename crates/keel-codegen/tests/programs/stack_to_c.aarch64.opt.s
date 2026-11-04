	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v9=x11 v10=x11 v11=x10 v13=x9 v14=x9 v15=x10 v18=x10 v22=x9 v24=x9 v27=x9 v34=x11 v35=x9 v36=x10 v38=x19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	sub	sp, sp, #48
Lmain_block0:
	; v0 = stack_addr ss0
	add	x9, sp, #0
	; v1 = iconst.i64 10
	movz	x10, #0xa
	; call @fill_squares(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_fill_squares
	; v2 = iconst.i64 0
	movz	x9, #0x0
	; jump block2(v2, v2)
	mov	x10, x9
Lmain_block2:
	; v28 = iconst.i64 2
	; v7 = shl v35, v28
	; v34 = stack_addr ss0
	add	x11, sp, #0
	; v8 = ptradd v34, v7
	; v9 = load.i32 v8
	ldr	w11, [x11, x9, lsl #2]
	; v10 = sext.i64 v9
	sxtw	x11, w11
	; v11 = iadd v36, v10
	add	x10, x10, x11
	; v12 = iconst.i64 1
	; v13 = iadd v35, v12
	add	x9, x9, #1
	; v39 = iconst.i64 10
	; v40 = icmp.ult v13, v39
	; brif v40, block2(v13, v11), block3(v11)
	cmp	x9, #10
	b.lo	Lmain_block2
	mov	x19, x10
Lmain_block3:
	; v14 = stack_addr ss1
	add	x9, sp, #40
	; v15 = iconst.i8 111
	movz	x10, #0x6f
	; store v15, v14
	strb	w10, [sp, #40]
	; v16 = iconst.i64 1
	; v17 = ptradd v14, v16
	; v18 = iconst.i8 107
	movz	x10, #0x6b
	; store v18, v17
	strb	w10, [x9, #1]
	; v19 = iconst.i64 2
	; v20 = ptradd v14, v19
	; v21 = iconst.i8 0
	; store v21, v20
	strb	wzr, [x9, #2]
	; v22 = call @puts(v14)
	mov	x0, x9
	bl	_puts
	mov	x9, x0
	; v23 = iconst.i64 285
	; v24 = icmp.ne v38, v23
	cmp	x19, #285
	cset	w9, ne
	; v27 = uext.i32 v24
	; return v27
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
