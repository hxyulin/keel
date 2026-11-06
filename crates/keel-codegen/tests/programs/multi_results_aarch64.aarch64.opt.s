	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_hfa4
	.p2align	2
_k_hfa4:
Lk_hfa4_begin:
	; regalloc: v0=d19 v1=d20 v2=d21 v3=d22
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
	fmov	d22, d3
Lk_hfa4_block0:
	; return v1, v2, v3, v0
	fmov	d0, d20
	fmov	d1, d21
	fmov	d2, d22
	fmov	d3, d19
	ldp	x29, x30, [sp], #16
	ret
Lk_hfa4_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_call_hfa3
	.p2align	2
_k_call_hfa3:
Lk_call_hfa3_begin:
	; regalloc: v0=d8 v1=d19 v2=d20 v3=d19 v4=d20 v5=d21 v6=d22 v7=d20 v8=d19 v9=d20 v10=d20 v11=d19 v12=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	d8, [sp, #-16]!
	.cfi_offset d8, -32
	fmov	d8, d0
Lk_call_hfa3_block0:
	; v1 = fconst.f64 1.0
	fmov	d19, #1.0
	; v2 = fadd v0, v1
	fadd	d20, d8, d19
	; v3, v4, v5 = call @c_hfa3(v2, v0, v1)
	fmov	d0, d20
	fmov	d1, d8
	fmov	d2, d19
	bl	_c_hfa3
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
	; v6 = fconst.f64 4.0
	fmov	d22, #4.0
	; v7 = fmul v4, v6
	fmul	d20, d20, d22
	; v8 = fadd v3, v7
	fadd	d19, d19, d20
	; v9 = fconst.f64 16.0
	fmov	d20, #16.0
	; v10 = fmul v5, v9
	fmul	d20, d21, d20
	; v11 = fadd v8, v10
	fadd	d19, d19, d20
	; v12 = fadd v11, v0
	fadd	d19, d19, d8
	; return v12
	fmov	d0, d19
	sub	sp, x29, #16
	ldr	d8, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_call_hfa3_end:
	.cfi_endproc
	.subsections_via_symbols
