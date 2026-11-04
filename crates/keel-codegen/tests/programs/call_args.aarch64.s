	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_weigh
	.p2align	2
_weigh:
Lweigh_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19 v8=x20 v9=x10 v10=x9 v11=x10 v12=x10 v13=x9 v14=x10 v15=x10 v16=x9 v17=x10 v18=x10 v19=x9 v20=x10 v21=x10 v22=x9 v23=x10 v24=x10 v25=x9 v26=x10 v27=x10 v28=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
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
	movz	x20, #0x2
	; v9 = imul v1, v8
	mul	x10, x10, x20
	; v10 = iadd v0, v9
	add	x9, x9, x10
	; v11 = iconst.i64 3
	movz	x10, #0x3
	; v12 = imul v2, v11
	mul	x10, x11, x10
	; v13 = iadd v10, v12
	add	x9, x9, x10
	; v14 = iconst.i64 4
	movz	x10, #0x4
	; v15 = imul v3, v14
	mul	x10, x12, x10
	; v16 = iadd v13, v15
	add	x9, x9, x10
	; v17 = iconst.i64 5
	movz	x10, #0x5
	; v18 = imul v4, v17
	mul	x10, x13, x10
	; v19 = iadd v16, v18
	add	x9, x9, x10
	; v20 = iconst.i64 6
	movz	x10, #0x6
	; v21 = imul v5, v20
	mul	x10, x14, x10
	; v22 = iadd v19, v21
	add	x9, x9, x10
	; v23 = iconst.i64 7
	movz	x10, #0x7
	; v24 = imul v6, v23
	mul	x10, x15, x10
	; v25 = iadd v22, v24
	add	x9, x9, x10
	; v26 = iconst.i64 8
	movz	x10, #0x8
	; v27 = imul v7, v26
	mul	x10, x19, x10
	; v28 = iadd v25, v27
	add	x9, x9, x10
	; return v28
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
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
	; v8 = iadd v7, v6
	add	x9, x9, x10
	; v9 = iadd v8, v3
	add	x9, x9, x12
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
	; regalloc: v0=x19 v1=x20 v2=x21 v3=x22 v4=x23 v5=x24 v6=x25 v7=x26 v8=x27 v9=[sp, #0] v10=x28 v11=x9 v12=x10 v13=x19 v14=x9 v15=x10 v16=x9 v17=x10 v18=x19 v19=x20 v20=x9 v21=x9 v22=x10 v23=x11 v24=x9 v25=x10 v26=x9 v27=x10 v28=x9 v29=x10 v30=x11 v31=x9
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
	; v0 = iconst.i32 0
	movz	x19, #0x0
	; v1 = iconst.i64 1
	movz	x20, #0x1
	; v2 = iconst.i64 2
	movz	x21, #0x2
	; v3 = iconst.i64 3
	movz	x22, #0x3
	; v4 = iconst.i64 4
	movz	x23, #0x4
	; v5 = iconst.i64 5
	movz	x24, #0x5
	; v6 = iconst.i64 6
	movz	x25, #0x6
	; v7 = iconst.i64 7
	movz	x26, #0x7
	; v8 = iconst.i64 8
	movz	x27, #0x8
	; v9 = call @weigh(v1, v2, v3, v4, v5, v6, v7, v8)
	mov	x0, x20
	mov	x1, x21
	mov	x2, x22
	mov	x3, x23
	mov	x4, x24
	mov	x5, x25
	mov	x6, x26
	mov	x7, x27
	bl	_weigh
	str	x0, [sp, #0]
	; v10 = iconst.i64 204
	; v11 = icmp.ne v9, v10
	ldr	x16, [sp, #0]
	cmp	x16, #204
	cset	w9, ne
	; v12 = iconst.i32 1
	movz	x10, #0x1
	; v13 = select v11, v12, v0
	tst	w9, #255
	csel	w19, w10, w19, ne
	; v14 = call @weigh(v8, v7, v6, v5, v4, v3, v2, v1)
	mov	x0, x27
	mov	x1, x26
	mov	x2, x25
	mov	x3, x24
	mov	x4, x23
	mov	x5, x22
	mov	x6, x21
	mov	x7, x20
	bl	_weigh
	mov	x9, x0
	; v15 = iconst.i64 120
	; v16 = icmp.ne v14, v15
	cmp	x9, #120
	cset	w9, ne
	; v17 = iconst.i32 2
	movz	x10, #0x2
	; v18 = select v16, v17, v13
	tst	w9, #255
	csel	w19, w10, w19, ne
	; v19 = stack_addr ss0
	add	x20, sp, #8
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
	cmp	x9, x10
	cset	w9, ne
	; v27 = iconst.i32 4
	movz	x10, #0x4
	; v28 = select v26, v27, v18
	tst	w9, #255
	csel	w9, w10, w19, ne
	; v29 = icmp.ne v9, v10
	ldr	x16, [sp, #0]
	cmp	x16, #204
	cset	w10, ne
	; v30 = iconst.i32 5
	movz	x11, #0x5
	; v31 = select v29, v30, v28
	tst	w10, #255
	csel	w9, w11, w9, ne
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
