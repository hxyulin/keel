	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lmain_block0:
	; v0 = iconst.i32 1
	movz	x9, #0x1
	; v1 = iconst.i32 0
	movz	x10, #0x0
	; v2 = udiv v0, v1
	cbz	w10, Lmain_trap
	udiv	w9, w9, w10
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmain_trap:
	brk	#1
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
