	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_sum
	.p2align	2
_sum:
Lsum_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v9=x13 v10=x13 v11=x12 v13=x11 v16=x11 v17=x12 v18=x13 v19=x14 v25=x14 v26=x14 v27=x12 v29=x11 v33=x14 v34=x14 v35=x13 v37=x11 v38=x12 v39=x12 v40=x11 v41=x12 v43=x9
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
	; v39 = iconst.i64 0
	movz	x12, #0x0
	; jump block4(v2, v2, v39)
	mov	x13, x12
	mov	x12, x11
Lsum_block4:
	; v19 = isub v1, v16
	sub	x14, x10, x11
	; v20 = iconst.i64 2
	; v21 = icmp.uge v19, v20
	; brif v21, block5, block6
	cmp	x14, #2
	b.hs	Lsum_block5
Lsum_block6:
	; v38 = iadd v17, v18
	add	x12, x12, x13
	; v5 = icmp.ugt v1, v16
	; brif v5, block2(v16, v38), block3(v38)
	cmp	x10, x11
	b.hi	Lsum_block2
	mov	x9, x12
	b	Lsum_block3
Lsum_block2:
	; v15 = iconst.i64 2
	; v7 = shl v40, v15
	; v8 = ptradd v0, v7
	; v9 = load.i32 v8
	ldr	w13, [x9, x11, lsl #2]
	; v10 = sext.i64 v9
	sxtw	x13, w13
	; v11 = iadd v41, v10
	add	x12, x12, x13
	; v12 = iconst.i64 1
	; v13 = iadd v40, v12
	add	x11, x11, #1
	; v44 = icmp.ugt v1, v13
	; brif v44, block2(v13, v11), block3(v11)
	cmp	x10, x11
	b.hi	Lsum_block2
	mov	x9, x12
Lsum_block3:
	; return v43
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsum_block5:
	; v22 = iconst.i64 2
	; v23 = shl v16, v22
	; v24 = ptradd v0, v23
	; v25 = load.i32 v24
	ldr	w14, [x9, x11, lsl #2]
	; v26 = sext.i64 v25
	sxtw	x14, w14
	; v27 = iadd v17, v26
	add	x12, x12, x14
	; v28 = iconst.i64 1
	; v29 = iadd v16, v28
	add	x11, x11, #1
	; v30 = iconst.i64 2
	; v31 = shl v29, v30
	; v32 = ptradd v0, v31
	; v33 = load.i32 v32
	ldr	w14, [x9, x11, lsl #2]
	; v34 = sext.i64 v33
	sxtw	x14, w14
	; v35 = iadd v18, v34
	add	x13, x13, x14
	; v36 = iconst.i64 1
	; v37 = iadd v29, v36
	add	x11, x11, #1
	; jump block4(v37, v27, v35)
	b	Lsum_block4
Lsum_end:
	.cfi_endproc
	.subsections_via_symbols
