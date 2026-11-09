	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_option_code
	.p2align	2
_option_code:
Loption_code_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Loption_code_block0:
	; switch v0, block2, [1: block1(v1)]
	cmp	x9, #1
	b.eq	Loption_code_block1
Loption_code_block2:
	; brif v0, block3, block4
	cbnz	x9, Loption_code_block3
Loption_code_block4:
	; v3 = iconst.i64 -1
	movn	x9, #0x0
	; jump block1(v3)
	mov	x10, x9
Loption_code_block1:
	; return v2
	mov	x0, x10
	ldp	x29, x30, [sp], #16
	ret
Loption_code_block3:
	; trap
	brk	#1
Loption_code_end:
	.cfi_endproc
	.subsections_via_symbols
