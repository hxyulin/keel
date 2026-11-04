	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_rk_adjacent
	.p2align	2
_rk_adjacent:
Lrk_adjacent_begin:
	; regalloc: v0=x9 v3=x10 v4=x11 v5=x12 v7=x11 v8=x12 v12=x13 v13=x13 v15=x14 v18=x14 v19=x14 v20=x13 v23=x9 v26=x12 v28=x11 v33=x13
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x3
Lrk_adjacent_block0:
	; v4 = iconst.i64 0
	movz	x11, #0x0
	; v5 = iconst.i64 1
	movz	x12, #0x1
	; v6 = icmp.ugt v3, v5
	; brif v6, block2(v5, v4), block3(v4)
	cmp	x10, #1
	b.ls	Lrk_adjacent_inst4_else
	mov	x16, x11
	mov	x11, x12
	mov	x12, x16
	b	Lrk_adjacent_block2
Lrk_adjacent_inst4_else:
	mov	x9, x11
	b	Lrk_adjacent_block3
Lrk_adjacent_block2:
	; v9 = iconst.i64 2
	; v10 = shl v7, v9
	; v11 = ptradd v0, v10
	; v12 = load.i32 v11
	ldr	w13, [x9, x11, lsl #2]
	; v13 = sext.i64 v12
	sxtw	x13, w13
	; v36 = iconst.i64 1
	; v15 = isub v7, v36
	sub	x14, x11, #1
	; v16 = shl v15, v9
	; v17 = ptradd v0, v16
	; v18 = load.i32 v17
	ldr	w14, [x9, x14, lsl #2]
	; v19 = sext.i64 v18
	sxtw	x14, w14
	; v20 = isub v13, v19
	sub	x13, x13, x14
	; v37 = iconst.i64 0
	; v22 = icmp.slt v20, v37
	; v24 = ineg v20
	; v33 = select v22, v24, v20
	cmp	x13, #0
	csneg	x13, x13, x13, ge
	; v26 = iadd v8, v33
	add	x12, x12, x13
	; v28 = iadd v7, v36
	add	x11, x11, #1
	; v29 = icmp.ugt v3, v28
	; brif v29, block2(v28, v26), block3(v26)
	cmp	x10, x11
	b.hi	Lrk_adjacent_block2
	mov	x9, x12
Lrk_adjacent_block3:
	; return v23
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lrk_adjacent_end:
	.cfi_endproc
	.subsections_via_symbols
