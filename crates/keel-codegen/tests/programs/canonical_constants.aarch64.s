	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_canonical_constants
	.p2align	2
_canonical_constants:
Lcanonical_constants_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lcanonical_constants_block0:
	; v1 = iconst.i64 37
	; v2 = iadd v1, v0
	add	x9, x9, #37
	; v3 = iconst.i64 63
	; v4 = xor v3, v2
	eor	x9, x9, #0x3f
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lcanonical_constants_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_canonical_compare
	.p2align	2
_canonical_compare:
Lcanonical_compare_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10 v5=x9 v6=x11 v7=x9 v8=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lcanonical_compare_block0:
	; v1 = iconst.i64 37
	movz	x10, #0x25
	; v2 = icmp.slt v1, v0
	cmp	x10, x9
	cset	w11, lt
	; v3 = icmp.uge v1, v0
	cmp	x10, x9
	cset	w9, hs
	; v4 = uext.i64 v2
	mov	w10, w11
	; v5 = uext.i64 v3
	; v6 = iconst.i64 1
	; v7 = shl v5, v6
	lsl	x9, x9, #1
	; v8 = or v7, v4
	orr	x9, x9, x10
	; return v8
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lcanonical_compare_end:
	.cfi_endproc
	.subsections_via_symbols
