	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_sum
	.p2align	2
_k_sum:
Lk_sum_begin:
	; regalloc: v0=x9 v1=x10 v4=x10 v6=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_sum_block0:
	; v2 = iconst.i64 0
	; v3 = icmp.eq v0, v2
	; brif v3, block1, block2
	cmp	x9, #0
	b.eq	Lk_sum_block1
Lk_sum_block2:
	; v4 = iadd v0, v1
	add	x10, x9, x10
	; v5 = iconst.i64 1
	; v6 = isub v0, v5
	sub	x9, x9, #1
	; return_call @k_sum(v6, v4)
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	b	_k_sum
Lk_sum_block1:
	; return v1
	mov	x0, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_sum_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_even
	.p2align	2
_k_even:
Lk_even_begin:
	; regalloc: v0=x9 v3=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_even_block0:
	; v1 = iconst.i64 0
	; v2 = icmp.eq v0, v1
	; brif v2, block1, block2
	cmp	x9, #0
	b.eq	Lk_even_block1
Lk_even_block2:
	; v4 = iconst.i64 1
	; v5 = isub v0, v4
	sub	x9, x9, #1
	; return_call @k_odd(v5)
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	b	_k_odd
Lk_even_block1:
	; v3 = iconst.i32 1
	movz	w9, #0x1
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_even_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_odd
	.p2align	2
_k_odd:
Lk_odd_begin:
	; regalloc: v0=x9 v3=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_odd_block0:
	; v1 = iconst.i64 0
	; v2 = icmp.eq v0, v1
	; brif v2, block1, block2
	cmp	x9, #0
	b.eq	Lk_odd_block1
Lk_odd_block2:
	; v4 = iconst.i64 1
	; v5 = isub v0, v4
	sub	x9, x9, #1
	; return_call @k_even(v5)
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	b	_k_even
Lk_odd_block1:
	; v3 = iconst.i32 0
	movz	w9, #0x0
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_odd_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_indirect
	.p2align	2
_k_indirect:
Lk_indirect_begin:
	; regalloc: v0=x9 v1=x10 v4=x9 v6=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_indirect_block0:
	; v2 = iconst.i64 0
	; v3 = icmp.eq v1, v2
	; brif v3, block1, block2
	cmp	x10, #0
	b.eq	Lk_indirect_block1
Lk_indirect_block2:
	; v5 = iconst.i64 1
	; v6 = isub v1, v5
	sub	x10, x10, #1
	; return_call_indirect v0(v0, v6) : (ptr, i64) -> i64 cc(c)
	mov	x0, x9
	mov	x1, x10
	mov	x16, x9
	ldp	x29, x30, [sp], #16
	br	x16
Lk_indirect_block1:
	; v4 = iconst.i64 77
	movz	x9, #0x4d
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_indirect_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_to_c
	.p2align	2
_k_to_c:
Lk_to_c_begin:
	; regalloc: v0=x19 v9=x20 v10=x21 v11=x22 v12=x23 v13=x24 v14=x25 v15=x26
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
	mov	x19, x0
Lk_to_c_block0:
	; v8 = iconst.i64 1
	; v9 = iadd v0, v8
	add	x20, x19, #1
	; v10 = iadd v9, v8
	add	x21, x20, #1
	; v11 = iadd v10, v8
	add	x22, x21, #1
	; v12 = iadd v11, v8
	add	x23, x22, #1
	; v13 = iadd v12, v8
	add	x24, x23, #1
	; v14 = iadd v13, v8
	add	x25, x24, #1
	; v15 = iadd v14, v8
	add	x26, x25, #1
	; call @clobber()
	bl	_clobber
	; return_call @c_sum8(v0, v9, v10, v11, v12, v13, v14, v15)
	mov	x0, x19
	mov	x1, x20
	mov	x2, x21
	mov	x3, x22
	mov	x4, x23
	mov	x5, x24
	mov	x6, x25
	mov	x7, x26
	sub	sp, x29, #64
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	b	_c_sum8
Lk_to_c_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_to_c_indirect
	.p2align	2
_k_to_c_indirect:
Lk_to_c_indirect_begin:
	; regalloc: v0=x19 v1=x20
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	mov	x19, x0
	mov	x20, x1
Lk_to_c_indirect_block0:
	; call @clobber()
	bl	_clobber
	; return_call_indirect v0(v1, v1, v1, v1, v1, v1, v1, v1) : (i64, i64, i64, i64, i64, i64, i64, i64) -> i64 cc(c)
	mov	x0, x20
	mov	x1, x20
	mov	x2, x20
	mov	x3, x20
	mov	x4, x20
	mov	x5, x20
	mov	x6, x20
	mov	x7, x20
	mov	x16, x19
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	br	x16
Lk_to_c_indirect_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_narrow
	.p2align	2
_k_narrow:
Lk_narrow_begin:
	; regalloc: v0=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_narrow_block0:
	; return_call @k_negate(v0)
	mov	x0, x9
	sxtb	w0, w0
	ldp	x29, x30, [sp], #16
	b	_k_negate
Lk_narrow_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_negate
	.p2align	2
_k_negate:
Lk_negate_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_negate_block0:
	; v1 = ineg v0
	neg	w9, w9
	; return v1
	mov	x0, x9
	sxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_negate_end:
	.cfi_endproc
	.subsections_via_symbols
