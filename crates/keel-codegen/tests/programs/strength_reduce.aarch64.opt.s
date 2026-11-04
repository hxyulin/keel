	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_strength
	.p2align	2
_strength:
Lstrength_begin:
	; regalloc: v0=x9 v3=x9 v5=x9 v8=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lstrength_block0:
	; v11 = iconst.i64 -8
	; v8 = and v0, v11
	and	x10, x9, #0xfffffffffffffff8
	; v3 = isub v0, v8
	sub	x9, x9, x10
	; v5 = iadd v3, v8
	add	x9, x9, x10
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lstrength_end:
	.cfi_endproc
	.subsections_via_symbols
