	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_weigh
	.p2align	2
_weigh:
Lweigh_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19 v10=x9 v11=x10 v13=x9 v16=x9 v17=x10 v19=x9 v20=x10 v22=x9 v23=x10 v25=x9 v28=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
	mov	x14, x5
	mov	x15, x6
	mov	x19, x7
Lweigh_block0:
	; v8 = iconst.i64 2
	; v29 = iconst.i64 1
	; v9 = shl v1, v29
	; v10 = iadd v0, v9
	add	x9, x9, x10, lsl #1
	; v11 = iconst.i64 3
	movz	x10, #0x3
	; v12 = imul v2, v11
	; v13 = iadd v10, v12
	madd	x9, x11, x10, x9
	; v15 = shl v3, v8
	; v16 = iadd v13, v15
	add	x9, x9, x12, lsl #2
	; v17 = iconst.i64 5
	movz	x10, #0x5
	; v18 = imul v4, v17
	; v19 = iadd v16, v18
	madd	x9, x13, x10, x9
	; v20 = iconst.i64 6
	movz	x10, #0x6
	; v21 = imul v5, v20
	; v22 = iadd v19, v21
	madd	x9, x14, x10, x9
	; v23 = iconst.i64 7
	movz	x10, #0x7
	; v24 = imul v6, v23
	; v25 = iadd v22, v24
	madd	x9, x15, x10, x9
	; v27 = shl v7, v11
	; v28 = iadd v25, v27
	add	x9, x9, x19, lsl #3
	; return v28
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lweigh_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_mixed
	.p2align	2
_mixed:
Lmixed_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x11 v5=x9 v6=x10 v7=x9 v8=x9 v9=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
Lmixed_block0:
	; v4 = load.i64 v2
	ldr	x11, [x11]
	; v5 = sext.i64 v0
	sxtw	x9, w9
	; v6 = sext.i64 v1
	sxtb	x10, w10
	; v7 = iadd v4, v5
	add	x9, x11, x9
	; v8 = iadd v6, v7
	add	x9, x10, x9
	; v9 = iadd v3, v8
	add	x9, x12, x9
	; return v9
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmixed_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_poke
	.p2align	2
_poke:
Lpoke_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lpoke_block0:
	; store v1, v0
	str	x10, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lpoke_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v1=x19 v2=x20 v3=x21 v4=x22 v5=x23 v6=x24 v7=x25 v8=x26 v9=x9 v11=x27 v13=x28 v14=x9 v17=x10 v18=x19 v19=x20 v20=x9 v21=x9 v22=x10 v23=x11 v24=x9 v25=x10 v27=x11 v28=x9 v30=x10 v31=x9
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
	stp	x27, x28, [sp, #-16]!
	.cfi_offset x27, -96
	.cfi_offset x28, -88
	sub	sp, sp, #16
Lmain_block0:
	; v1 = iconst.i64 1
	movz	x19, #0x1
	; v2 = iconst.i64 2
	movz	x20, #0x2
	; v3 = iconst.i64 3
	movz	x21, #0x3
	; v4 = iconst.i64 4
	movz	x22, #0x4
	; v5 = iconst.i64 5
	movz	x23, #0x5
	; v6 = iconst.i64 6
	movz	x24, #0x6
	; v7 = iconst.i64 7
	movz	x25, #0x7
	; v8 = iconst.i64 8
	movz	x26, #0x8
	; v9 = call @weigh(v1, v2, v3, v4, v5, v6, v7, v8)
	mov	x0, x19
	mov	x1, x20
	mov	x2, x21
	mov	x3, x22
	mov	x4, x23
	mov	x5, x24
	mov	x6, x25
	mov	x7, x26
	bl	_weigh
	mov	x9, x0
	; v10 = iconst.i64 204
	; v11 = icmp.ne v9, v10
	cmp	x9, #204
	cset	w27, ne
	; v13 = uext.i32 v11
	mov	w28, w27
	; v14 = call @weigh(v8, v7, v6, v5, v4, v3, v2, v1)
	mov	x0, x26
	mov	x1, x25
	mov	x2, x24
	mov	x3, x23
	mov	x4, x22
	mov	x5, x21
	mov	x6, x20
	mov	x7, x19
	bl	_weigh
	mov	x9, x0
	; v15 = iconst.i64 120
	; v16 = icmp.ne v14, v15
	; v17 = iconst.i32 2
	movz	x10, #0x2
	; v18 = select v16, v17, v13
	cmp	x9, #120
	csel	w19, w10, w28, ne
	; v19 = stack_addr ss0
	add	x20, sp, #0
	; v20 = iconst.i64 1000
	movz	x9, #0x3e8
	; call @poke(v19, v20)
	mov	x0, x20
	mov	x1, x9
	bl	_poke
	; v21 = iconst.i32 -5
	movn	x9, #0x4
	; v22 = iconst.i8 -3
	movn	x10, #0x2
	; v23 = iconst.i64 100000
	movz	x11, #0x86a0
	movk	x11, #0x1, lsl #16
	; v24 = call @mixed(v21, v22, v19, v23)
	mov	x0, x9
	mov	x1, x10
	mov	x2, x20
	mov	x3, x11
	bl	_mixed
	mov	x9, x0
	; v25 = iconst.i64 100992
	movz	x10, #0x8a80
	movk	x10, #0x1, lsl #16
	; v26 = icmp.ne v24, v25
	; v27 = iconst.i32 4
	movz	x11, #0x4
	; v28 = select v26, v27, v18
	cmp	x9, x10
	csel	w9, w11, w19, ne
	; v30 = iconst.i32 5
	movz	x10, #0x5
	; v31 = select v11, v30, v28
	tst	w27, #255
	csel	w9, w10, w9, ne
	; return v31
	mov	x0, x9
	sub	sp, x29, #80
	ldp	x27, x28, [sp], #16
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
