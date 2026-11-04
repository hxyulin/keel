	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_compact_frame
	.p2align	2
_compact_frame:
Lcompact_frame_begin:
	; regalloc: v0=x9 v1=x20 v2=x21 v3=x9 v4=x10 v5=x22 v7=x9 v8=x10 v9=x11 v10=x12 v11=x10 v12=x10 v13=x9
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
	sub	sp, sp, #32
	mov	x19, sp
	mov	x9, x0
Lcompact_frame_block0:
	; v1 = stack_addr ss1
	add	x20, x19, #0
	; v2 = stack_addr ss3
	add	x21, x19, #24
	; store v0, v1
	str	x9, [x19, #0]
	; v3 = iconst.i64 17
	movz	x9, #0x11
	; store v3, v2
	str	x9, [x19, #24]
	; v4 = iconst.i64 64
	movz	x10, #0x40
	; v5 = intrinsic stack_alloc(v4)
	add	x16, x10, #15
	and	x16, x16, #-16
	sub	sp, sp, x16
	add	x22, sp, #0
	; store v3, v5
	str	x9, [x22]
	; v7 = call @touch_frame(v1, v2)
	mov	x0, x20
	mov	x1, x21
	bl	_touch_frame
	mov	x9, x0
	; v8 = load.i64 v1
	ldr	x10, [x19, #0]
	; v9 = load.i64 v2
	ldr	x11, [x19, #24]
	; v10 = load.i64 v5
	ldr	x12, [x22]
	; v11 = iadd v8, v9
	add	x10, x10, x11
	; v12 = iadd v10, v11
	add	x10, x12, x10
	; v13 = iadd v7, v12
	add	x9, x9, x10
	; return v13
	mov	x0, x9
	sub	sp, x29, #32
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lcompact_frame_end:
	.cfi_endproc
	.subsections_via_symbols
