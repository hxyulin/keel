	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_hot32p
	.p2align	2
_hot32p:
Lhot32p_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x11 v6=x12 v7=x13 v9=x14 v10=x12 v11=x14 v12=x11 v13=x14 v14=x13 v15=x9
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
	; jump block1(v2, v3, v4)
Lhot32p_block1:
	; v8 = icmp.ult v7, v0
	; brif v8, block2, block3(v6)
	cmp	w13, w9
	b.lo	Lhot32p_block2
	mov	x9, x12
	b	Lhot32p_block3
Lhot32p_block2:
	; v9 = imul v5, v1
	mul	w14, w11, w10
	; v10 = iadd v6, v9
	add	w12, w12, w14
	; v11 = iconst.i32 1
	; v12 = iadd v5, v11
	add	w11, w11, #1
	; v13 = iconst.i32 1
	; v14 = iadd v7, v13
	add	w13, w13, #1
	; jump block1(v12, v10, v14)
	b	Lhot32p_block1
Lhot32p_block3:
	; return v15
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
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x11 v6=x12 v7=x13 v9=x14 v10=x12 v11=x14 v12=x11 v13=x14 v14=x13 v15=x9
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
	; jump block1(v2, v3, v4)
Lhot32m_block1:
	; v8 = icmp.ult v7, v0
	; brif v8, block2, block3(v6)
	cmp	w13, w9
	b.lo	Lhot32m_block2
	mov	x9, x12
	b	Lhot32m_block3
Lhot32m_block2:
	; v9 = imul v5, v1
	mul	w14, w11, w10
	; v10 = iadd v6, v9
	add	w12, w12, w14
	; v11 = iconst.i32 -7
	; v12 = iadd v5, v11
	sub	w11, w11, #7
	; v13 = iconst.i32 1
	; v14 = iadd v7, v13
	add	w13, w13, #1
	; jump block1(v12, v10, v14)
	b	Lhot32m_block1
Lhot32m_block3:
	; return v15
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
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x11 v6=x12 v7=x13 v9=x14 v10=x12 v11=x14 v12=x11 v13=x14 v14=x13 v15=x9
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
	; jump block1(v2, v3, v4)
Lhot32z_block1:
	; v8 = icmp.ult v7, v0
	; brif v8, block2, block3(v6)
	cmp	w13, w9
	b.lo	Lhot32z_block2
	mov	x9, x12
	b	Lhot32z_block3
Lhot32z_block2:
	; v9 = imul v5, v1
	mul	w14, w11, w10
	; v10 = iadd v6, v9
	add	w12, w12, w14
	; v11 = iconst.i32 0
	; v12 = iadd v5, v11
	add	w11, w11, #0
	; v13 = iconst.i32 1
	; v14 = iadd v7, v13
	add	w13, w13, #1
	; jump block1(v12, v10, v14)
	b	Lhot32z_block1
Lhot32z_block3:
	; return v15
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
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x11 v6=x12 v7=x13 v9=x14 v10=x12 v11=x14 v12=x11 v13=x14 v14=x13 v15=x9
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
	; jump block1(v2, v3, v4)
Lhot64p_block1:
	; v8 = icmp.ult v7, v0
	; brif v8, block2, block3(v6)
	cmp	x13, x9
	b.lo	Lhot64p_block2
	mov	x9, x12
	b	Lhot64p_block3
Lhot64p_block2:
	; v9 = imul v5, v1
	mul	x14, x11, x10
	; v10 = iadd v6, v9
	add	x12, x12, x14
	; v11 = iconst.i64 1
	; v12 = iadd v5, v11
	add	x11, x11, #1
	; v13 = iconst.i64 1
	; v14 = iadd v7, v13
	add	x13, x13, #1
	; jump block1(v12, v10, v14)
	b	Lhot64p_block1
Lhot64p_block3:
	; return v15
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
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x11 v6=x12 v7=x13 v9=x14 v10=x12 v11=x14 v12=x11 v13=x14 v14=x13 v15=x9
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
	; jump block1(v2, v3, v4)
Lhot64m_block1:
	; v8 = icmp.ult v7, v0
	; brif v8, block2, block3(v6)
	cmp	x13, x9
	b.lo	Lhot64m_block2
	mov	x9, x12
	b	Lhot64m_block3
Lhot64m_block2:
	; v9 = imul v5, v1
	mul	x14, x11, x10
	; v10 = iadd v6, v9
	add	x12, x12, x14
	; v11 = iconst.i64 -7
	; v12 = iadd v5, v11
	sub	x11, x11, #7
	; v13 = iconst.i64 1
	; v14 = iadd v7, v13
	add	x13, x13, #1
	; jump block1(v12, v10, v14)
	b	Lhot64m_block1
Lhot64m_block3:
	; return v15
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
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x11 v6=x12 v7=x13 v9=x14 v10=x12 v11=x14 v12=x11 v13=x14 v14=x13 v15=x9
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
	; jump block1(v2, v3, v4)
Lhot64z_block1:
	; v8 = icmp.ult v7, v0
	; brif v8, block2, block3(v6)
	cmp	x13, x9
	b.lo	Lhot64z_block2
	mov	x9, x12
	b	Lhot64z_block3
Lhot64z_block2:
	; v9 = imul v5, v1
	mul	x14, x11, x10
	; v10 = iadd v6, v9
	add	x12, x12, x14
	; v11 = iconst.i64 0
	; v12 = iadd v5, v11
	add	x11, x11, #0
	; v13 = iconst.i64 1
	; v14 = iadd v7, v13
	add	x13, x13, #1
	; jump block1(v12, v10, v14)
	b	Lhot64z_block1
Lhot64z_block3:
	; return v15
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
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x11 v7=x13 v8=x14 v10=x15 v11=x15 v12=x14 v13=x11 v14=x15 v15=x13 v16=x9
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
	; jump block1(v2, v4, v5)
Loffset32_block1:
	; v9 = icmp.ult v7, v0
	; brif v9, block2, block3(v8)
	cmp	w13, w9
	b.lo	Loffset32_block2
	mov	x9, x14
	b	Loffset32_block3
Loffset32_block2:
	; v10 = iadd v6, v3
	add	w15, w11, w12
	; v11 = sext.i64 v10
	sxtw	x15, w15
	; v12 = iadd v8, v11
	add	x14, x14, x15
	; v13 = iadd v6, v1
	add	w11, w11, w10
	; v14 = iconst.i32 1
	; v15 = iadd v7, v14
	add	w13, w13, #1
	; jump block1(v13, v15, v12)
	b	Loffset32_block1
Loffset32_block3:
	; return v16
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
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x11 v7=x13 v8=x14 v10=x15 v11=x14 v12=x11 v13=x15 v14=x13 v15=x9
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
	; jump block1(v2, v4, v5)
Loffset64_block1:
	; v9 = icmp.ult v7, v0
	; brif v9, block2, block3(v8)
	cmp	x13, x9
	b.lo	Loffset64_block2
	mov	x9, x14
	b	Loffset64_block3
Loffset64_block2:
	; v10 = iadd v6, v3
	add	x15, x11, x12
	; v11 = iadd v8, v10
	add	x14, x14, x15
	; v12 = iadd v6, v1
	add	x11, x11, x10
	; v13 = iconst.i64 1
	; v14 = iadd v7, v13
	add	x13, x13, #1
	; jump block1(v12, v14, v11)
	b	Loffset64_block1
Loffset64_block3:
	; return v15
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Loffset64_end:
	.cfi_endproc
	.subsections_via_symbols
