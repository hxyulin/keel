	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x9 v1=x9 v2=x9 v3=x9 v4=x9 v5=x9 v6=x9 v7=x9 v8=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lmain_block0:
	; v0 = iconst.i32 72
	movz	x9, #0x48
	; v1 = call @putchar(v0)
	mov	x0, x9
	bl	_putchar
	mov	x9, x0
	; v2 = iconst.i32 105
	movz	x9, #0x69
	; v3 = call @putchar(v2)
	mov	x0, x9
	bl	_putchar
	mov	x9, x0
	; v4 = iconst.i32 33
	movz	x9, #0x21
	; v5 = call @putchar(v4)
	mov	x0, x9
	bl	_putchar
	mov	x9, x0
	; v6 = iconst.i32 10
	movz	x9, #0xa
	; v7 = call @putchar(v6)
	mov	x0, x9
	bl	_putchar
	mov	x9, x0
	; v8 = iconst.i32 0
	movz	x9, #0x0
	; return v8
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
