	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_folding_duplicate
	.p2align	2
_folding_duplicate:
Lfolding_duplicate_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lfolding_duplicate_block0:
	; v0 = iconst.i64 7
	movz	x9, #0x7
	; v1 = iadd v0, v0
	add	x9, x9, #7
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lfolding_duplicate_end:
	.cfi_endproc
	.subsections_via_symbols
