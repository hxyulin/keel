	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_sparse_default
	.p2align	2
_sparse_default:
Lsparse_default_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x9 v6=x12 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lsparse_default_block0:
	; v3 = iconst.i32 17
	movz	w12, #0x11
	; v4 = iconst.i32 29
	movz	w13, #0x1d
	; switch v0, block1, [259: block2(v3), 260: block2(v4), 288: block2(v0)]
	cmp	w9, #259
	b.eq	Lsparse_default_block2
	cmp	w9, #260
	b.eq	Lsparse_default_inst2_edge2
	cmp	w9, #288
	b.eq	Lsparse_default_inst2_edge3
	b	Lsparse_default_block1
Lsparse_default_inst2_edge2:
	mov	x12, x13
	b	Lsparse_default_block2
Lsparse_default_inst2_edge3:
	mov	x12, x9
	b	Lsparse_default_block2
Lsparse_default_block1:
	; v5 = iconst.i32 41
	movz	w9, #0x29
	; jump block2(v5)
	mov	x12, x9
Lsparse_default_block2:
	; store v6, v1
	str	w12, [x10]
	; v7 = load.i32 v2
	ldr	w9, [x11]
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsparse_default_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_trap_default
	.p2align	2
_trap_default:
Ltrap_default_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v3=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Ltrap_default_block0:
	; v1 = iconst.i32 1
	movz	w10, #0x1
	; switch v0, block1, [259: block2(v1), 288: block2(v1)]
	cmp	w9, #259
	b.eq	Ltrap_default_block2
	cmp	w9, #288
	b.eq	Ltrap_default_block2
Ltrap_default_block1:
	; v2 = iconst.i32 0
	movz	w10, #0x0
	; jump block2(v2)
Ltrap_default_block2:
	; brif v3, block3, block4
	cbz	w10, Ltrap_default_block4
Ltrap_default_block3:
	; return v0
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltrap_default_block4:
	; trap
	brk	#1
Ltrap_default_end:
	.cfi_endproc
	.subsections_via_symbols
