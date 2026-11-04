	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_known_bits
	.p2align	2
_known_bits:
Lknown_bits_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x11 v5=x12 v6=x12 v7=x10:x12 v8=x13:x14 v9=x10 v10=x10 v11=x9 v12=x12 v13=x9 v14=x9 v15=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lknown_bits_block0:
	; v1 = trunc.i8 v0
	mov	x10, x9
	; v2 = uext.i64 v1
	uxtb	w11, w10
	; v3 = iconst.i64 255
	; v4 = and v2, v3
	and	x11, x11, #0xff
	; v5 = iconst.i64 256
	; v6 = icmp.ult v4, v5
	cmp	x11, #256
	cset	w12, lo
	; brif v6, block1, block4
	tst	w12, #255
	b.eq	Lknown_bits_block4
Lknown_bits_block1:
	; v7 = uext.i128 v1
	mov	x0, x10
	uxtb	w4, w0
	mov	x5, #0
	mov	x10, x4
	mov	x12, x5
	; v8 = iconst.i128 256
	movz	x4, #0x100
	movz	x5, #0x0
	mov	x13, x4
	mov	x14, x5
	; v9 = icmp.ult v7, v8
	mov	x0, x10
	mov	x1, x12
	mov	x2, x13
	mov	x3, x14
	cmp	x0, x2
	sbcs	xzr, x1, x3
	cset	w10, lo
	; brif v9, block2, block4
	tst	w10, #255
	b.eq	Lknown_bits_block4
Lknown_bits_block2:
	; v10 = iconst.i64 15
	; v11 = and v0, v10
	and	x9, x9, #0xf
	; v12 = iconst.i64 60
	; v13 = shl v11, v12
	lsl	x9, x9, #60
	; v14 = ushr v13, v12
	lsr	x9, x9, #60
	; v15 = icmp.ule v14, v10
	cmp	x9, #15
	cset	w9, ls
	; brif v15, block3, block4
	tst	w9, #255
	b.ne	Lknown_bits_block3
Lknown_bits_block4:
	; trap
	brk	#1
Lknown_bits_block3:
	; return v4
	mov	x0, x11
	ldp	x29, x30, [sp], #16
	ret
Lknown_bits_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_known_add
	.p2align	2
_known_add:
Lknown_add_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x9:x10 v4=x11:x12 v5=x9:x10 v6=x11:x12 v7=x11 v8=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lknown_add_block0:
	; v1 = iconst.i64 15
	; v2 = and v0, v1
	and	x9, x9, #0xf
	; v3 = uext.i128 v2
	mov	x0, x9
	mov	x4, x0
	mov	x5, #0
	mov	x9, x4
	mov	x10, x5
	; v4 = iconst.i128 16
	movz	x4, #0x10
	movz	x5, #0x0
	mov	x11, x4
	mov	x12, x5
	; v5 = iadd v3, v4
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v6 = iconst.i128 32
	movz	x4, #0x20
	movz	x5, #0x0
	mov	x11, x4
	mov	x12, x5
	; v7 = icmp.ult v5, v6
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x0, x2
	sbcs	xzr, x1, x3
	cset	w11, lo
	; brif v7, block1, block2
	tst	w11, #255
	b.ne	Lknown_add_block1
Lknown_add_block2:
	; trap
	brk	#1
Lknown_add_block1:
	; v8 = trunc.i64 v5
	; return v8
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lknown_add_end:
	.cfi_endproc
	.subsections_via_symbols
