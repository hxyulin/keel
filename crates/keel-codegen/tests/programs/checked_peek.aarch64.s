	.section	__TEXT,__text
	.section	__TEXT,__text
	.p2align	2
_checked_peek:
Lchecked_peek_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x11 v4=x11 v5=x10 v9=x11 v10=x11 v11=x11 v13=x9 v14=x10 v15=x9 v17=x9 v18=x9 v19=x9 v20=x9 v21=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #160
	mov	x9, x0
	mov	x10, x1
Lchecked_peek_block0:
	; v2 = iconst.i64 16
	; v3 = ptradd v0, v2
	add	x11, x9, #16
	; v4 = load.i64 v3
	ldr	x11, [x11]
	; v5, v6 = uadd_overflow v1, v4
	adds	x10, x10, x11
	; v7 = iconst.i8 1
	; v8 = xor v6, v7
	; brif v8, block1, block2
	b.hs	Lchecked_peek_block2
Lchecked_peek_block1:
	; v9 = iconst.i64 8
	; v10 = ptradd v0, v9
	add	x11, x9, #8
	; v11 = load.i64 v10
	ldr	x11, [x11]
	; v12 = icmp.uge v5, v11
	; brif v12, block3, block4
	cmp	x10, x11
	b.hs	Lchecked_peek_block3
Lchecked_peek_block4:
	; v16 = icmp.ult v5, v11
	; brif v16, block5, block6
	cmp	x10, x11
	b.hs	Lchecked_peek_block6
Lchecked_peek_block5:
	; v17 = load.ptr v0
	ldr	x9, [x9]
	; v18 = ptradd v17, v5
	add	x9, x9, x10
	; v19 = load.i8 v18
	ldrb	w9, [x9]
	; return v19
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lchecked_peek_block6:
	; v20 = stack_addr ss1
	add	x9, sp, #64
	; v21 = iconst.i64 2
	movz	x10, #0x2
	; store v21, v20
	str	x10, [sp, #64]
	; call @checked_panic(v20)
	mov	x0, x9
	bl	_checked_panic
	; trap
	brk	#1
Lchecked_peek_block3:
	; v15 = iconst.i8 0
	movz	w9, #0x0
	; return v15
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lchecked_peek_block2:
	; v13 = stack_addr ss0
	add	x9, sp, #0
	; v14 = iconst.i64 1
	movz	x10, #0x1
	; store v14, v13
	str	x10, [sp, #0]
	; call @checked_panic(v13)
	mov	x0, x9
	bl	_checked_panic
	; trap
	brk	#1
Lchecked_peek_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_checked_peek_public
	.p2align	2
_checked_peek_public:
Lchecked_peek_public_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lchecked_peek_public_block0:
	; v2 = call @checked_peek(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_checked_peek
	mov	x9, x0
	; return v2
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lchecked_peek_public_end:
	.cfi_endproc
	.subsections_via_symbols
