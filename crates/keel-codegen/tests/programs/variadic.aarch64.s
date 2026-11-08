	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_sum
	.p2align	2
_k_sum:
Lk_sum_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x11 v5=x12 v7=x13 v8=x12 v9=x13 v10=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #32
	mov	x9, x0
Lk_sum_block0:
	; v1 = stack_addr ss0
	add	x10, sp, #0
	; intrinsic va_start(v1)
	add	x16, x29, #16
	str	x16, [x10]
	; v2 = iconst.i64 0
	movz	x11, #0x0
	; v3 = iconst.i32 0
	movz	w12, #0x0
	; jump block1(v3, v2)
	mov	x16, x11
	mov	x11, x12
	mov	x12, x16
Lk_sum_block1:
	; v6 = icmp.slt v4, v0
	; brif v6, block2, block3
	cmp	w11, w9
	b.ge	Lk_sum_block3
Lk_sum_block2:
	; v7 = va_arg.i64 v1
	ldr	x16, [x10]
	add	x17, x16, #8
	str	x17, [x10]
	ldr	x13, [x16]
	; v8 = iadd v5, v7
	add	x12, x12, x13
	; v9 = iconst.i32 1
	; v10 = iadd v4, v9
	add	w11, w11, #1
	; jump block1(v10, v8)
	b	Lk_sum_block1
Lk_sum_block3:
	; return v5
	mov	x0, x12
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_sum_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pairs
	.p2align	2
_k_pairs:
Lk_pairs_begin:
	; regalloc: v0=x9 v1=x10 v2=d19 v3=x11 v4=x11 v5=d19 v7=x12 v8=d20 v9=d21 v10=d20 v11=d19 v12=x12 v13=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #32
	mov	x9, x0
Lk_pairs_block0:
	; v1 = stack_addr ss0
	add	x10, sp, #0
	; intrinsic va_start(v1)
	add	x16, x29, #16
	str	x16, [x10]
	; v2 = fconst.f64 0.0
	fmov	d19, xzr
	; v3 = iconst.i32 0
	movz	w11, #0x0
	; jump block1(v3, v2)
Lk_pairs_block1:
	; v6 = icmp.slt v4, v0
	; brif v6, block2, block3
	cmp	w11, w9
	b.ge	Lk_pairs_block3
Lk_pairs_block2:
	; v7 = va_arg.i32 v1
	ldr	x16, [x10]
	add	x17, x16, #8
	str	x17, [x10]
	ldr	w12, [x16]
	; v8 = va_arg.f64 v1
	ldr	x16, [x10]
	add	x17, x16, #8
	str	x17, [x10]
	ldr	d20, [x16]
	; v9 = sitofp.f64 v7
	scvtf	d21, w12
	; v10 = fmul v9, v8
	fmul	d20, d21, d20
	; v11 = fadd v5, v10
	fadd	d19, d19, d20
	; v12 = iconst.i32 1
	; v13 = iadd v4, v12
	add	w11, w11, #1
	; jump block1(v13, v11)
	b	Lk_pairs_block1
Lk_pairs_block3:
	; return v5
	fmov	d0, d19
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_pairs_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_tail
	.p2align	2
_k_tail:
Lk_tail_begin:
	; regalloc: v7=x9 v8=x10 v9=x11 v10=x12 v11=x12 v12=x11 v13=x10 v14=x10 v15=x10 v16=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #32
	ldr	w10, [x29, #16]
	mov	x9, x7
Lk_tail_block0:
	; v9 = stack_addr ss0
	add	x11, sp, #0
	; intrinsic va_start(v9)
	add	x16, x29, #24
	str	x16, [x11]
	; v10 = va_arg.ptr v9
	ldr	x16, [x11]
	add	x17, x16, #8
	str	x17, [x11]
	ldr	x12, [x16]
	; v11 = load.i64 v10
	ldr	x12, [x12]
	; v12 = va_arg.i64 v9
	ldr	x16, [x11]
	add	x17, x16, #8
	str	x17, [x11]
	ldr	x11, [x16]
	; v13 = sext.i64 v8
	sxtw	x10, w10
	; v14 = iadd v13, v11
	add	x10, x10, x12
	; v15 = iadd v14, v12
	add	x10, x10, x11
	; v16 = iadd v15, v7
	add	x9, x10, x9
	; return v16
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_tail_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sum_three
	.p2align	2
_k_sum_three:
Lk_sum_three_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #32
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_sum_three_block0:
	; v3 = iconst.i32 3
	movz	w12, #0x3
	; v4 = call @k_sum(v3, v0, v1, v2)
	str	x9, [sp, #0]
	str	x10, [sp, #8]
	str	x11, [sp, #16]
	mov	x0, x12
	bl	_k_sum
	mov	x9, x0
	; return v4
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_sum_three_end:
	.cfi_endproc
	.subsections_via_symbols
