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
	; v0 = iconst.i32 -2147483648
	movn	x9, #0xffff
	movk	x9, #0x8000, lsl #16
	; v1 = iconst.i32 -1
	movn	x10, #0x0
	; v2 = sdiv v0, v1
	movz	w8, #0x8000, lsl #16
	cmn	w10, #1
	ccmp	w9, w8, #0, eq
	b.eq	Lmain_trap
	sdiv	w9, w9, w10
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmain_trap:
	brk	#1
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
