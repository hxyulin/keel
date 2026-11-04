	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_forward
	.p2align	2
_forward:
Lforward_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10 v5=x9 v6=x10 v7=x9 v8=x10 v9=x9 v10=x9
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
	; brif v2, block1(v0, v1), block2(v0, v1)
	tst	w11, #255
	b.ne	Lforward_block1
Lforward_block2:
	; jump block3(v5, v5)
	mov	x10, x9
	b	Lforward_block3
Lforward_block1:
	; jump block3(v4, v3)
	mov	x16, x9
	mov	x9, x10
	mov	x10, x16
Lforward_block3:
	; v9 = imul v7, v7
	mul	x9, x9, x9
	; v10 = iadd v9, v8
	add	x9, x9, x10
	; return v10
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lforward_end:
	.cfi_endproc
	.subsections_via_symbols
