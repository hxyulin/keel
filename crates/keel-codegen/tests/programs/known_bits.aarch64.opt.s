	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_known_bits
	.p2align	2
_known_bits:
Lknown_bits_begin:
	; regalloc: v0=x9 v1=x9 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lknown_bits_block0:
	; v1 = trunc.i8 v0
	; v2 = uext.i64 v1
	uxtb	w9, w9
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lknown_bits_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_known_add
	.p2align	2
_known_add:
Lknown_add_begin:
	; regalloc: v0=x9 v2=x9 v3=x9:x10 v4=x11:x12 v5=x9:x10 v8=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lknown_add_block0:
	; v1 = iconst.i64 15
	; v2 = and v0, v1
	and	x9, x9, #0xf
	; v3 = uext.i128 v2
	mov	x0, x9
	mov	x4, x0
	mov	x5, #0
	mov	x9, x4
	mov	x10, x5
	; v4 = iconst.i128 16
	movz	x4, #0x10
	movz	x5, #0x0
	mov	x11, x4
	mov	x12, x5
	; v5 = iadd v3, v4
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v8 = trunc.i64 v5
	; return v8
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lknown_add_end:
	.cfi_endproc
	.subsections_via_symbols
