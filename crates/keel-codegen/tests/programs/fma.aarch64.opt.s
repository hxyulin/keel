	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_fma_f64
	.p2align	2
_k_fma_f64:
Lk_fma_f64_begin:
	; regalloc: v0=d19 v1=d20 v2=d21 v3=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
Lk_fma_f64_block0:
	; v3 = fma v0, v1, v2
	fmadd	d19, d19, d20, d21
	; return v3
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lk_fma_f64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_fma_f32
	.p2align	2
_k_fma_f32:
Lk_fma_f32_begin:
	; regalloc: v0=d19 v1=d20 v2=d21 v3=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
Lk_fma_f32_block0:
	; v3 = fma v0, v1, v2
	fmadd	s19, s19, s20, s21
	; return v3
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lk_fma_f32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_copysign_f64
	.p2align	2
_k_copysign_f64:
Lk_copysign_f64_begin:
	; regalloc: v0=d19 v1=d20 v2=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
Lk_copysign_f64_block0:
	; v2 = fcopysign v0, v1
	fmov	x16, d19
	fmov	x17, d20
	bfxil	x17, x16, #0, #63
	fmov	d19, x17
	; return v2
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lk_copysign_f64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_copysign_f32
	.p2align	2
_k_copysign_f32:
Lk_copysign_f32_begin:
	; regalloc: v0=d19 v1=d20 v2=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
Lk_copysign_f32_block0:
	; v2 = fcopysign v0, v1
	fmov	w16, s19
	fmov	w17, s20
	bfxil	w17, w16, #0, #31
	fmov	s19, w17
	; return v2
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lk_copysign_f32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_const_fma
	.p2align	2
_k_const_fma:
Lk_const_fma_begin:
	; regalloc: v3=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lk_const_fma_block0:
	; v3 = fconst.f64 5.551115123125783e-17
	movz	x16, #0x3c90, lsl #48
	fmov	d19, x16
	; return v3
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lk_const_fma_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_const_copysign
	.p2align	2
_k_const_copysign:
Lk_const_copysign_begin:
	; regalloc: v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lk_const_copysign_block0:
	; v3 = iconst.i64 -2251799813684957
	movz	x9, #0x123
	movk	x9, #0xfff8, lsl #48
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_const_copysign_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_const_copysign_f32
	.p2align	2
_k_const_copysign_f32:
Lk_const_copysign_f32_begin:
	; regalloc: v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lk_const_copysign_f32_block0:
	; v3 = iconst.i32 2143289410
	movz	w9, #0x42
	movk	w9, #0x7fc0, lsl #16
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_const_copysign_f32_end:
	.cfi_endproc
	.subsections_via_symbols
