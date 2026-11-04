	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_forward
	.p2align	2
_forward:
Lforward_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v7=x10 v10=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lforward_block0:
	; brif v2, block3(v1), block3(v0)
	tst	w11, #255
	b.ne	Lforward_block3
	mov	x10, x9
Lforward_block3:
	; v9 = imul v7, v7
	; v10 = iadd v0, v9
	madd	x9, x10, x10, x9
	; return v10
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lforward_end:
	.cfi_endproc
	.subsections_via_symbols
