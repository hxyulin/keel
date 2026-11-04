	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lmain_block0:
	; v0 = iconst.i128 7
	movz	x4, #0x7
	movz	x5, #0x0
	mov	x9, x4
	mov	x10, x5
	; v1 = iconst.i128 0
	movz	x4, #0x0
	movz	x5, #0x0
	mov	x11, x4
	mov	x12, x5
	; v2 = udiv v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	orr	x16, x2, x3
	cbz	x16, Lmain_trap
	bl	___udivti3
	mov	x9, x0
	mov	x10, x1
	; v3 = trunc.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmain_trap:
	brk	#1
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
