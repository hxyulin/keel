	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_uadd_i8
	.p2align	2
_k_uadd_i8:
Lk_uadd_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_uadd_i8_block0:
	; v3, v4 = uadd_overflow v0, v1
	uxtb	w16, w9
	uxtb	w17, w10
	add	w16, w16, w17
	tst	w16, #4294967040
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_uadd_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sadd_i8
	.p2align	2
_k_sadd_i8:
Lk_sadd_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_sadd_i8_block0:
	; v3, v4 = sadd_overflow v0, v1
	sxtb	w16, w9
	sxtb	w17, w10
	add	w16, w16, w17
	cmp	w16, w16, sxtb
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	sxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_sadd_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_usub_i8
	.p2align	2
_k_usub_i8:
Lk_usub_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_usub_i8_block0:
	; v3, v4 = usub_overflow v0, v1
	uxtb	w16, w9
	uxtb	w17, w10
	sub	w16, w16, w17
	tst	w16, #4294967040
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_usub_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ssub_i8
	.p2align	2
_k_ssub_i8:
Lk_ssub_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_ssub_i8_block0:
	; v3, v4 = ssub_overflow v0, v1
	sxtb	w16, w9
	sxtb	w17, w10
	sub	w16, w16, w17
	cmp	w16, w16, sxtb
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	sxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_ssub_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umul_i8
	.p2align	2
_k_umul_i8:
Lk_umul_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_umul_i8_block0:
	; v3, v4 = umul_overflow v0, v1
	uxtb	w16, w9
	uxtb	w17, w10
	mul	w16, w16, w17
	tst	w16, #4294967040
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_umul_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smul_i8
	.p2align	2
_k_smul_i8:
Lk_smul_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_smul_i8_block0:
	; v3, v4 = smul_overflow v0, v1
	sxtb	w16, w9
	sxtb	w17, w10
	mul	w16, w16, w17
	cmp	w16, w16, sxtb
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	sxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_smul_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_uadd_i16
	.p2align	2
_k_uadd_i16:
Lk_uadd_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_uadd_i16_block0:
	; v3, v4 = uadd_overflow v0, v1
	uxth	w16, w9
	uxth	w17, w10
	add	w16, w16, w17
	tst	w16, #4294901760
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_uadd_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sadd_i16
	.p2align	2
_k_sadd_i16:
Lk_sadd_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_sadd_i16_block0:
	; v3, v4 = sadd_overflow v0, v1
	sxth	w16, w9
	sxth	w17, w10
	add	w16, w16, w17
	cmp	w16, w16, sxth
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	sxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_sadd_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_usub_i16
	.p2align	2
_k_usub_i16:
Lk_usub_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_usub_i16_block0:
	; v3, v4 = usub_overflow v0, v1
	uxth	w16, w9
	uxth	w17, w10
	sub	w16, w16, w17
	tst	w16, #4294901760
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_usub_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ssub_i16
	.p2align	2
_k_ssub_i16:
Lk_ssub_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_ssub_i16_block0:
	; v3, v4 = ssub_overflow v0, v1
	sxth	w16, w9
	sxth	w17, w10
	sub	w16, w16, w17
	cmp	w16, w16, sxth
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	sxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_ssub_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umul_i16
	.p2align	2
_k_umul_i16:
Lk_umul_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_umul_i16_block0:
	; v3, v4 = umul_overflow v0, v1
	uxth	w16, w9
	uxth	w17, w10
	mul	w16, w16, w17
	tst	w16, #4294901760
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_umul_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smul_i16
	.p2align	2
_k_smul_i16:
Lk_smul_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_smul_i16_block0:
	; v3, v4 = smul_overflow v0, v1
	sxth	w16, w9
	sxth	w17, w10
	mul	w16, w16, w17
	cmp	w16, w16, sxth
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	sxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_smul_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_uadd_i32
	.p2align	2
_k_uadd_i32:
Lk_uadd_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_uadd_i32_block0:
	; v3, v4 = uadd_overflow v0, v1
	adds	w16, w9, w10
	cset	w8, hs
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_uadd_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sadd_i32
	.p2align	2
_k_sadd_i32:
Lk_sadd_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_sadd_i32_block0:
	; v3, v4 = sadd_overflow v0, v1
	adds	w16, w9, w10
	cset	w8, vs
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_sadd_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_usub_i32
	.p2align	2
_k_usub_i32:
Lk_usub_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_usub_i32_block0:
	; v3, v4 = usub_overflow v0, v1
	subs	w16, w9, w10
	cset	w8, lo
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_usub_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ssub_i32
	.p2align	2
_k_ssub_i32:
Lk_ssub_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_ssub_i32_block0:
	; v3, v4 = ssub_overflow v0, v1
	subs	w16, w9, w10
	cset	w8, vs
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_ssub_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umul_i32
	.p2align	2
_k_umul_i32:
Lk_umul_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_umul_i32_block0:
	; v3, v4 = umul_overflow v0, v1
	umull	x16, w9, w10
	cmp	xzr, x16, lsr #32
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_umul_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smul_i32
	.p2align	2
_k_smul_i32:
Lk_smul_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_smul_i32_block0:
	; v3, v4 = smul_overflow v0, v1
	smull	x16, w9, w10
	cmp	x16, w16, sxtw
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_smul_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_uadd_i64
	.p2align	2
_k_uadd_i64:
Lk_uadd_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_uadd_i64_block0:
	; v3, v4 = uadd_overflow v0, v1
	adds	x16, x9, x10
	cset	w8, hs
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_uadd_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sadd_i64
	.p2align	2
_k_sadd_i64:
Lk_sadd_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_sadd_i64_block0:
	; v3, v4 = sadd_overflow v0, v1
	adds	x16, x9, x10
	cset	w8, vs
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_sadd_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_usub_i64
	.p2align	2
_k_usub_i64:
Lk_usub_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_usub_i64_block0:
	; v3, v4 = usub_overflow v0, v1
	subs	x16, x9, x10
	cset	w8, lo
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_usub_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ssub_i64
	.p2align	2
_k_ssub_i64:
Lk_ssub_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_ssub_i64_block0:
	; v3, v4 = ssub_overflow v0, v1
	subs	x16, x9, x10
	cset	w8, vs
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_ssub_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umul_i64
	.p2align	2
_k_umul_i64:
Lk_umul_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_umul_i64_block0:
	; v3, v4 = umul_overflow v0, v1
	umulh	x8, x9, x10
	mul	x16, x9, x10
	cmp	x8, #0
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_umul_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smul_i64
	.p2align	2
_k_smul_i64:
Lk_smul_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_smul_i64_block0:
	; v3, v4 = smul_overflow v0, v1
	smulh	x8, x9, x10
	mul	x16, x9, x10
	cmp	x8, x16, asr #63
	cset	w8, ne
	mov	x9, x16
	mov	x10, x8
	; store v4, v2
	strb	w10, [x11]
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_smul_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_uadd_i128
	.p2align	2
_k_uadd_i128:
Lk_uadd_i128_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x13 v3=x9:x10 v4=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
Lk_uadd_i128_block0:
	; v3, v4 = uadd_overflow v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	adds	x4, x0, x2
	adcs	x5, x1, x3
	cset	w6, hs
	mov	x9, x4
	mov	x10, x5
	mov	x11, x6
	; store v4, v2
	strb	w11, [x13]
	; return v3
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_uadd_i128_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sadd_i128
	.p2align	2
_k_sadd_i128:
Lk_sadd_i128_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x13 v3=x9:x10 v4=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
Lk_sadd_i128_block0:
	; v3, v4 = sadd_overflow v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	adds	x4, x0, x2
	adcs	x5, x1, x3
	cset	w6, vs
	mov	x9, x4
	mov	x10, x5
	mov	x11, x6
	; store v4, v2
	strb	w11, [x13]
	; return v3
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_sadd_i128_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_usub_i128
	.p2align	2
_k_usub_i128:
Lk_usub_i128_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x13 v3=x9:x10 v4=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
Lk_usub_i128_block0:
	; v3, v4 = usub_overflow v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	subs	x4, x0, x2
	sbcs	x5, x1, x3
	cset	w6, lo
	mov	x9, x4
	mov	x10, x5
	mov	x11, x6
	; store v4, v2
	strb	w11, [x13]
	; return v3
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_usub_i128_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ssub_i128
	.p2align	2
_k_ssub_i128:
Lk_ssub_i128_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x13 v3=x9:x10 v4=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
Lk_ssub_i128_block0:
	; v3, v4 = ssub_overflow v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	subs	x4, x0, x2
	sbcs	x5, x1, x3
	cset	w6, vs
	mov	x9, x4
	mov	x10, x5
	mov	x11, x6
	; store v4, v2
	strb	w11, [x13]
	; return v3
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_ssub_i128_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_live
	.p2align	2
_k_live:
Lk_live_begin:
	; regalloc: v0=x9 v1=x10 v2=x19 v3=x20 v4=x9 v5=x10 v6=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	mov	x9, x0
	mov	x10, x1
Lk_live_block0:
	; v2, v3 = uadd_overflow v0, v1
	adds	x16, x9, x10
	cset	w8, hs
	mov	x19, x16
	mov	x20, x8
	; call @clobber()
	bl	_clobber
	; v4 = uext.i64 v3
	uxtb	w9, w20
	; v5 = iadd v2, v2
	add	x10, x19, x19
	; v6 = iadd v4, v5
	add	x9, x9, x10
	; return v6
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_live_end:
	.cfi_endproc
	.subsections_via_symbols
