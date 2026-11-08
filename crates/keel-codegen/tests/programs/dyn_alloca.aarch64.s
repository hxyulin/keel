	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_vla
	.p2align	2
_k_vla:
Lk_vla_begin:
	; regalloc: v0=x20 v1=x21 v2=x22 v3=x23 v4=x9 v5=x24 v6=x9 v7=x10 v8=x25 v10=x11 v11=x11 v12=x11 v13=x11 v14=x12 v15=x10 v16=x10 v17=x11 v18=x21 v19=x9 v20=x10 v21=x10 v22=x9 v23=x10 v24=x9 v25=x9
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
	str	x25, [sp, #-16]!
	.cfi_offset x25, -80
	sub	sp, sp, #64
	mov	x19, sp
	mov	x20, x0
Lk_vla_block0:
	; v1 = iconst.i64 1000
	movz	x21, #0x3e8
	; v2 = imul v0, v1
	mul	x22, x20, x21
	; v3 = iconst.i64 8
	movz	x23, #0x8
	; v4 = imul v0, v3
	mul	x9, x20, x23
	; v5 = intrinsic stack_alloc(v4)
	add	x16, x9, #15
	and	x16, x16, #-16
	sub	sp, sp, x16
	add	x24, sp, #16
	; call @c_fill(v5, v0)
	mov	x0, x24
	mov	x1, x20
	bl	_c_fill
	; v6 = iconst.i64 0
	movz	x9, #0x0
	; jump block1(v6, v6)
	mov	x10, x9
	mov	x25, x9
Lk_vla_block1:
	; v9 = icmp.ult v7, v0
	; brif v9, block2, block3
	cmp	x10, x20
	b.hs	Lk_vla_block3
Lk_vla_block2:
	; v10 = imul v7, v3
	mul	x11, x10, x23
	; v11 = ptradd v5, v10
	add	x11, x24, x11
	; v12 = load.i64 v11
	ldr	x11, [x11]
	; v13 = iadd v8, v12
	add	x11, x25, x11
	; v14 = iconst.i64 1
	; v15 = iadd v7, v14
	add	x10, x10, #1
	; jump block1(v15, v13)
	mov	x25, x11
	b	Lk_vla_block1
Lk_vla_block3:
	; v16 = iconst.i64 2
	movz	x10, #0x2
	; v17 = iconst.i64 3
	movz	x11, #0x3
	; v18 = call @c_many(v5, v0, v1, v3, v6, v16, v17, v2, v0, v1)
	str	x20, [sp, #0]
	str	x21, [sp, #8]
	mov	x0, x24
	mov	x1, x20
	mov	x2, x21
	mov	x3, x23
	mov	x4, x9
	mov	x5, x10
	mov	x6, x11
	mov	x7, x22
	bl	_c_many
	mov	x21, x0
	; v19 = stack_addr ss0
	add	x9, x19, #40
	; store v0, v19
	str	x20, [x19, #40]
	; v20 = ptradd v19, v3
	add	x10, x9, #8
	; store v2, v20
	str	x22, [x10]
	; v21 = ptradd v20, v3
	add	x10, x10, #8
	; store v8, v21
	str	x25, [x10]
	; v22 = call @c_big(v19)
	ldr	x17, [x9, #0]
	str	x17, [x19, #16]
	ldr	x17, [x9, #8]
	str	x17, [x19, #24]
	ldr	x17, [x9, #16]
	str	x17, [x19, #32]
	add	x0, x19, #16
	bl	_c_big
	mov	x9, x0
	; v23 = iadd v8, v18
	add	x10, x25, x21
	; v24 = iadd v23, v22
	add	x9, x10, x9
	; v25 = iadd v24, v2
	add	x9, x9, x22
	; return v25
	mov	x0, x9
	sub	sp, x29, #64
	ldr	x25, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_vla_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_loop
	.p2align	2
_k_loop:
Lk_loop_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v3=x11 v5=x12 v6=x13 v7=x13 v8=x13 v9=x11 v10=x12 v11=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x19, sp
	mov	x9, x0
Lk_loop_block0:
	; v1 = iconst.i64 0
	movz	x10, #0x0
	; jump block1(v1, v1)
	mov	x11, x10
Lk_loop_block1:
	; v4 = icmp.ult v2, v0
	; brif v4, block2, block3
	cmp	x10, x9
	b.hs	Lk_loop_block3
Lk_loop_block2:
	; v5 = intrinsic stack_save()
	mov	x12, sp
	; v6 = iconst.i64 65536
	movz	x13, #0x1, lsl #16
	; v7 = intrinsic stack_alloc(v6)
	add	x16, x13, #15
	and	x16, x16, #-16
	sub	sp, sp, x16
	add	x13, sp, #0
	; store v2, v7
	str	x10, [x13]
	; v8 = load.i64 v7
	ldr	x13, [x13]
	; v9 = iadd v3, v8
	add	x11, x11, x13
	; intrinsic stack_restore(v5)
	mov	sp, x12
	; v10 = iconst.i64 1
	; v11 = iadd v2, v10
	add	x10, x10, #1
	; jump block1(v11, v9)
	b	Lk_loop_block1
Lk_loop_block3:
	; return v3
	mov	x0, x11
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_loop_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_outer
	.p2align	2
_k_outer:
Lk_outer_begin:
	; regalloc: v0=x9 v1=x10 v2=x19 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x9, x0
Lk_outer_block0:
	; v1 = iconst.i64 7
	movz	x10, #0x7
	; v2 = imul v0, v1
	mul	x19, x9, x10
	; v3 = call @k_vla(v0)
	mov	x0, x9
	bl	_k_vla
	mov	x9, x0
	; v4 = iadd v3, v2
	add	x9, x9, x19
	; return v4
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_outer_end:
	.cfi_endproc
	.subsections_via_symbols
