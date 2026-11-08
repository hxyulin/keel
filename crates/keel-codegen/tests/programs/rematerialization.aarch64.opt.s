	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_remat_probe
	.p2align	2
_remat_probe:
Lremat_probe_begin:
	; regalloc: v0=x19 v1=x20 v2=x21 v3=x22 v4=x23 v5=x24 v6=x25 v8=x26 v9=x9 v14=x9 v15=x9 v16=x9 v18=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	stp	x21, x22, [sp, #-16]!
	.cfi_offset x21, -48
	.cfi_offset x22, -40
	stp	x23, x24, [sp, #-16]!
	.cfi_offset x23, -64
	.cfi_offset x24, -56
	stp	x25, x26, [sp, #-16]!
	.cfi_offset x25, -80
	.cfi_offset x26, -72
	sub	sp, sp, #16
	mov	x19, x0
	mov	x20, x1
Lremat_probe_block0:
	; v2 = iconst.i64 0
	movz	x21, #0x0
	; v3 = iconst.i64 17
	movz	x22, #0x11
	; v4 = iconst.i64 31
	movz	x23, #0x1f
	; v5 = iconst.i64 255
	movz	x24, #0xff
	; v6 = iconst.i64 65535
	movz	x25, #0xffff
	; call @clobber()
	bl	_clobber
	; v8 = select v0, v2, v1
	cmp	x19, #0
	csel	x26, x21, x20, ne
	; v9 = call @remat_sum(v8, v3, v4, v5, v6, v2, v3, v4, v5, v6)
	str	x24, [sp, #0]
	str	x25, [sp, #8]
	mov	x0, x26
	mov	x1, x22
	mov	x2, x23
	mov	x3, x24
	mov	x4, x25
	mov	x5, x21
	mov	x6, x22
	mov	x7, x23
	bl	_remat_sum
	mov	x9, x0
	; v18 = select v0, v3, v4
	cmp	x19, #0
	csel	x10, x22, x23, ne
	; v14 = iadd v8, v9
	add	x9, x26, x9
	; v15 = iadd v14, v18
	add	x9, x9, x10
	; v16 = iadd v1, v15
	add	x9, x20, x9
	; return v16
	mov	x0, x9
	sub	sp, x29, #64
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lremat_probe_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_remat_shift
	.p2align	2
_remat_shift:
Lremat_shift_begin:
	; regalloc: v0=x19:x20 v1=x21 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	str	x21, [sp, #-16]!
	.cfi_offset x21, -48
	mov	x19, x0
	mov	x20, x1
Lremat_shift_block0:
	; v1 = iconst.i64 7
	movz	x21, #0x7
	; call @clobber()
	bl	_clobber
	; v2 = shl v0, v1
	mov	x0, x19
	mov	x1, x20
	mov	x2, x21
	and	x2, x2, #127
	lsl	x3, x1, x2
	mvn	w6, w2
	lsr	x7, x0, #1
	lsr	x7, x7, x6
	orr	x3, x3, x7
	lsl	x7, x0, x2
	tst	x2, #64
	csel	x5, x7, x3, ne
	csel	x4, xzr, x7, ne
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	sub	sp, x29, #32
	ldr	x21, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lremat_shift_end:
	.cfi_endproc
	.subsections_via_symbols
