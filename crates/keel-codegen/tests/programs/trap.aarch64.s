	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lmain_block0:
	; v0 = iconst.i32 1
	movz	x9, #0x1
	; brif v0, block1, block2
	cbnz	w9, Lmain_block1
Lmain_block2:
	; return v0
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmain_block1:
	; trap
	brk	#1
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
