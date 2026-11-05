	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_hot32p
	.p2align	2
_hot32p:
Lhot32p_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v10=x11 v14=x12 v16=x11 v20=x13 v21=x11 v22=x12 v23=x13 v24=x9
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
Lhot32p_block0:
	; v4 = iconst.i32 0
	movz	w13, #0x0
	; v16 = imul v1, v2
	mul	w11, w10, w11
	; v8 = icmp.ugt v0, v4
	; brif v8, block2(v3, v4, v16), block3(v3)
	cmp	w9, #0
	b.ls	Lhot32p_inst3_else
	mov	x16, x11
	mov	x11, x12
	mov	x12, x13
	mov	x13, x16
	b	Lhot32p_block2
Lhot32p_inst3_else:
	mov	x9, x12
	b	Lhot32p_block3
Lhot32p_block2:
	; v10 = iadd v21, v23
	add	w11, w11, w13
	; v11 = iconst.i32 1
	; v14 = iadd v22, v11
	add	w12, w12, #1
	; v20 = iadd v1, v23
	add	w13, w10, w13
	; v27 = icmp.ugt v0, v14
	; brif v27, block2(v10, v14, v20), block3(v10)
	cmp	w9, w12
	b.hi	Lhot32p_block2
	mov	x9, x11
Lhot32p_block3:
	; return v24
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lhot32p_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_hot32m
	.p2align	2
_hot32m:
Lhot32m_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v10=x11 v14=x12 v16=x11 v17=x14 v18=x10 v20=x13 v21=x11 v22=x12 v23=x13 v24=x9
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
Lhot32m_block0:
	; v4 = iconst.i32 0
	movz	w13, #0x0
	; v16 = imul v1, v2
	mul	w11, w10, w11
	; v17 = iconst.i32 -7
	movn	w14, #0x6
	; v18 = imul v1, v17
	mul	w10, w10, w14
	; v8 = icmp.ugt v0, v4
	; brif v8, block2(v3, v4, v16), block3(v3)
	cmp	w9, #0
	b.ls	Lhot32m_inst3_else
	mov	x16, x11
	mov	x11, x12
	mov	x12, x13
	mov	x13, x16
	b	Lhot32m_block2
Lhot32m_inst3_else:
	mov	x9, x12
	b	Lhot32m_block3
Lhot32m_block2:
	; v10 = iadd v21, v23
	add	w11, w11, w13
	; v13 = iconst.i32 1
	; v14 = iadd v22, v13
	add	w12, w12, #1
	; v20 = iadd v18, v23
	add	w13, w10, w13
	; v27 = icmp.ugt v0, v14
	; brif v27, block2(v10, v14, v20), block3(v10)
	cmp	w9, w12
	b.hi	Lhot32m_block2
	mov	x9, x11
Lhot32m_block3:
	; return v24
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lhot32m_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_hot32z
	.p2align	2
_hot32z:
Lhot32z_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v10=x12 v14=x13 v16=x12 v17=x13 v18=x9
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
Lhot32z_block0:
	; v4 = iconst.i32 0
	movz	w13, #0x0
	; v8 = icmp.ugt v0, v4
	; brif v8, block2(v3, v4), block3(v3)
	cmp	w9, #0
	b.hi	Lhot32z_block2
	mov	x9, x12
	b	Lhot32z_block3
Lhot32z_block2:
	; v9 = imul v1, v2
	; v10 = iadd v16, v9
	madd	w12, w10, w11, w12
	; v13 = iconst.i32 1
	; v14 = iadd v17, v13
	add	w13, w13, #1
	; v20 = icmp.ugt v0, v14
	; brif v20, block2(v10, v14), block3(v10)
	cmp	w9, w13
	b.hi	Lhot32z_block2
	mov	x9, x12
Lhot32z_block3:
	; return v18
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lhot32z_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_hot64p
	.p2align	2
_hot64p:
Lhot64p_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v10=x11 v14=x12 v16=x11 v20=x13 v21=x11 v22=x12 v23=x13 v24=x9
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
Lhot64p_block0:
	; v4 = iconst.i64 0
	movz	x13, #0x0
	; v16 = imul v1, v2
	mul	x11, x10, x11
	; v8 = icmp.ugt v0, v4
	; brif v8, block2(v3, v4, v16), block3(v3)
	cmp	x9, #0
	b.ls	Lhot64p_inst3_else
	mov	x16, x11
	mov	x11, x12
	mov	x12, x13
	mov	x13, x16
	b	Lhot64p_block2
Lhot64p_inst3_else:
	mov	x9, x12
	b	Lhot64p_block3
Lhot64p_block2:
	; v10 = iadd v21, v23
	add	x11, x11, x13
	; v11 = iconst.i64 1
	; v14 = iadd v22, v11
	add	x12, x12, #1
	; v20 = iadd v1, v23
	add	x13, x10, x13
	; v27 = icmp.ugt v0, v14
	; brif v27, block2(v10, v14, v20), block3(v10)
	cmp	x9, x12
	b.hi	Lhot64p_block2
	mov	x9, x11
Lhot64p_block3:
	; return v24
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lhot64p_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_hot64m
	.p2align	2
_hot64m:
Lhot64m_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v10=x11 v14=x12 v16=x11 v17=x14 v18=x10 v20=x13 v21=x11 v22=x12 v23=x13 v24=x9
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
Lhot64m_block0:
	; v4 = iconst.i64 0
	movz	x13, #0x0
	; v16 = imul v1, v2
	mul	x11, x10, x11
	; v17 = iconst.i64 -7
	movn	x14, #0x6
	; v18 = imul v1, v17
	mul	x10, x10, x14
	; v8 = icmp.ugt v0, v4
	; brif v8, block2(v3, v4, v16), block3(v3)
	cmp	x9, #0
	b.ls	Lhot64m_inst3_else
	mov	x16, x11
	mov	x11, x12
	mov	x12, x13
	mov	x13, x16
	b	Lhot64m_block2
Lhot64m_inst3_else:
	mov	x9, x12
	b	Lhot64m_block3
Lhot64m_block2:
	; v10 = iadd v21, v23
	add	x11, x11, x13
	; v13 = iconst.i64 1
	; v14 = iadd v22, v13
	add	x12, x12, #1
	; v20 = iadd v18, v23
	add	x13, x10, x13
	; v27 = icmp.ugt v0, v14
	; brif v27, block2(v10, v14, v20), block3(v10)
	cmp	x9, x12
	b.hi	Lhot64m_block2
	mov	x9, x11
Lhot64m_block3:
	; return v24
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lhot64m_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_hot64z
	.p2align	2
_hot64z:
Lhot64z_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v10=x12 v14=x13 v16=x12 v17=x13 v18=x9
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
Lhot64z_block0:
	; v4 = iconst.i64 0
	movz	x13, #0x0
	; v8 = icmp.ugt v0, v4
	; brif v8, block2(v3, v4), block3(v3)
	cmp	x9, #0
	b.hi	Lhot64z_block2
	mov	x9, x12
	b	Lhot64z_block3
Lhot64z_block2:
	; v9 = imul v1, v2
	; v10 = iadd v16, v9
	madd	x12, x10, x11, x12
	; v13 = iconst.i64 1
	; v14 = iadd v17, v13
	add	x13, x13, #1
	; v20 = icmp.ugt v0, v14
	; brif v20, block2(v10, v14), block3(v10)
	cmp	x9, x13
	b.hi	Lhot64z_block2
	mov	x9, x12
Lhot64z_block3:
	; return v18
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lhot64z_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_offset32
	.p2align	2
_offset32:
Loffset32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v11=x14 v12=x13 v13=x11 v15=x12 v17=x11 v18=x11 v19=x12 v20=x13 v23=x9
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
Loffset32_block0:
	; v4 = iconst.i32 0
	movz	w13, #0x0
	; v5 = iconst.i64 17
	movz	x14, #0x11
	; v17 = iadd v2, v3
	add	w11, w11, w12
	; v9 = icmp.ugt v0, v4
	; brif v9, block2(v17, v4, v5), block3(v5)
	cmp	w9, #0
	b.ls	Loffset32_inst4_else
	mov	x12, x13
	mov	x13, x14
	b	Loffset32_block2
Loffset32_inst4_else:
	mov	x9, x14
	b	Loffset32_block3
Loffset32_block2:
	; v11 = sext.i64 v18
	sxtw	x14, w11
	; v12 = iadd v20, v11
	add	x13, x13, x14
	; v13 = iadd v1, v18
	add	w11, w10, w11
	; v14 = iconst.i32 1
	; v15 = iadd v19, v14
	add	w12, w12, #1
	; v24 = icmp.ugt v0, v15
	; brif v24, block2(v13, v15, v12), block3(v12)
	cmp	w9, w12
	b.hi	Loffset32_block2
	mov	x9, x13
Loffset32_block3:
	; return v23
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Loffset32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_offset64
	.p2align	2
_offset64:
Loffset64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v11=x13 v12=x11 v14=x12 v16=x11 v17=x11 v18=x12 v19=x13 v22=x9
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
Loffset64_block0:
	; v4 = iconst.i64 0
	movz	x13, #0x0
	; v5 = iconst.i64 17
	movz	x14, #0x11
	; v16 = iadd v2, v3
	add	x11, x11, x12
	; v9 = icmp.ugt v0, v4
	; brif v9, block2(v16, v4, v5), block3(v5)
	cmp	x9, #0
	b.ls	Loffset64_inst4_else
	mov	x12, x13
	mov	x13, x14
	b	Loffset64_block2
Loffset64_inst4_else:
	mov	x9, x14
	b	Loffset64_block3
Loffset64_block2:
	; v11 = iadd v17, v19
	add	x13, x11, x13
	; v12 = iadd v1, v17
	add	x11, x10, x11
	; v13 = iconst.i64 1
	; v14 = iadd v18, v13
	add	x12, x12, #1
	; v23 = icmp.ugt v0, v14
	; brif v23, block2(v12, v14, v11), block3(v11)
	cmp	x9, x12
	b.hi	Loffset64_block2
	mov	x9, x13
Loffset64_block3:
	; return v22
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Loffset64_end:
	.cfi_endproc
	.subsections_via_symbols
