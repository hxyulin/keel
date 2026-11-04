	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x9:x10 v1=x11 v2=x9:x10 v3=x11:x12 v4=x9:x10 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lmain_block0:
	; v0 = iconst.i128 1
	movz	x4, #0x1
	movz	x5, #0x0
	mov	x9, x4
	mov	x10, x5
	; v1 = iconst.i32 127
	movz	x11, #0x7f
	; v2 = shl v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	and	x2, x2, #127
	lsl	x3, x1, x2
	mvn	w6, w2
	lsr	x7, x0, #1
	lsr	x7, x7, x6
	orr	x3, x3, x7
	lsl	x7, x0, x2
	tst	x2, #64
	csel	x5, x7, x3, ne
	csel	x4, xzr, x7, ne
	mov	x9, x4
	mov	x10, x5
	; v3 = iconst.i128 -1
	movn	x4, #0x0
	movn	x5, #0x0
	mov	x11, x4
	mov	x12, x5
	; v4 = sdiv v2, v3
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	orr	x16, x2, x3
	cbz	x16, Lmain_trap
	and	x16, x2, x3
	cmn	x16, #1
	b.ne	Lmain_inst4_divok
	cbnz	x0, Lmain_inst4_divok
	movz	x16, #0x8000, lsl #48
	cmp	x1, x16
	b.eq	Lmain_trap
Lmain_inst4_divok:
	bl	___divti3
	mov	x9, x0
	mov	x10, x1
	; v5 = trunc.i32 v4
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmain_trap:
	brk	#1
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
