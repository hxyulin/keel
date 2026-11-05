	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_poly
	.p2align	2
_poly:
Lpoly_begin:
	; regalloc: v0=d19 v1=d20 v2=d21 v3=d20 v4=d21 v5=d19 v6=d19 v7=d20 v8=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
Lpoly_block0:
	; v1 = fconst.f64 3.0
	fmov	d20, #3.0
	; v2 = fmul v0, v0
	fmul	d21, d19, d19
	; v3 = fmul v1, v2
	fmul	d20, d20, d21
	; v4 = fconst.f64 2.0
	fmov	d21, #2.0
	; v5 = fmul v4, v0
	fmul	d19, d21, d19
	; v6 = fsub v3, v5
	fsub	d19, d20, d19
	; v7 = fconst.f64 0.5
	fmov	d20, #0.5
	; v8 = fadd v6, v7
	fadd	d19, d19, d20
	; return v8
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lpoly_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_poly32
	.p2align	2
_poly32:
Lpoly32_begin:
	; regalloc: v0=d19 v1=d20 v2=d21 v3=d20 v4=d21 v5=d19 v6=d19 v7=d20 v8=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
Lpoly32_block0:
	; v1 = fconst.f32 3.0
	fmov	s20, #3.0
	; v2 = fmul v0, v0
	fmul	s21, s19, s19
	; v3 = fmul v1, v2
	fmul	s20, s20, s21
	; v4 = fconst.f32 2.0
	fmov	s21, #2.0
	; v5 = fmul v4, v0
	fmul	s19, s21, s19
	; v6 = fsub v3, v5
	fsub	s19, s20, s19
	; v7 = fconst.f32 0.5
	fmov	s20, #0.5
	; v8 = fadd v6, v7
	fadd	s19, s19, s20
	; return v8
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lpoly32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_mixed
	.p2align	2
_mixed:
Lmixed_begin:
	; regalloc: v0=x9 v1=d19 v2=x10 v3=d20 v4=x11 v5=d21 v6=d22 v7=d19 v8=d19 v9=d21 v10=d22 v11=d21 v12=d19 v13=d20 v14=d21 v15=d20 v16=d19 v17=d20 v18=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	fmov	d19, d0
	mov	x10, x1
	fmov	d20, d1
	mov	x11, x2
Lmixed_block0:
	; v5 = sitofp.f64 v0
	scvtf	d21, x9
	; v6 = fconst.f64 2.0
	fmov	d22, #2.0
	; v7 = fmul v1, v6
	fmul	d19, d19, d22
	; v8 = fadd v5, v7
	fadd	d19, d21, d19
	; v9 = sitofp.f64 v2
	scvtf	d21, w10
	; v10 = fconst.f64 3.0
	fmov	d22, #3.0
	; v11 = fmul v9, v10
	fmul	d21, d21, d22
	; v12 = fadd v8, v11
	fadd	d19, d19, d21
	; v13 = fpext.f64 v3
	fcvt	d20, s20
	; v14 = fconst.f64 4.0
	fmov	d21, #4.0
	; v15 = fmul v13, v14
	fmul	d20, d20, d21
	; v16 = fadd v12, v15
	fadd	d19, d19, d20
	; v17 = load.f64 v4
	ldr	d20, [x11]
	; v18 = fadd v16, v17
	fadd	d19, d19, d20
	; return v18
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lmixed_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_call_c
	.p2align	2
_call_c:
Lcall_c_begin:
	; regalloc: v0=d8 v1=d19 v2=d9 v3=x9 v4=d19 v5=x10 v6=d10 v7=d19 v8=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	d8, d9, [sp, #-16]!
	.cfi_offset d8, -32
	.cfi_offset d9, -24
	str	d10, [sp, #-16]!
	.cfi_offset d10, -48
	fmov	d8, d0
Lcall_c_block0:
	; v1 = fconst.f64 10.0
	fmov	d19, #10.0
	; v2 = fmul v0, v1
	fmul	d9, d8, d19
	; call @fclobber()
	bl	_fclobber
	; v3 = iconst.i64 7
	movz	x9, #0x7
	; v4 = fconst.f32 1.5
	fmov	s19, #1.5
	; v5 = iconst.i32 -2
	movn	w10, #0x1
	; v6 = call @c_mix(v3, v0, v4, v5)
	mov	x0, x9
	fmov	d0, d8
	fmov	d1, d19
	mov	x1, x10
	bl	_c_mix
	fmov	d10, d0
	; call @fclobber()
	bl	_fclobber
	; v7 = fadd v0, v6
	fadd	d19, d8, d10
	; v8 = fadd v7, v2
	fadd	d19, d19, d9
	; return v8
	fmov	d0, d19
	sub	sp, x29, #32
	ldr	d10, [sp], #16
	ldp	d8, d9, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lcall_c_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_unary
	.p2align	2
_unary:
Lunary_begin:
	; regalloc: v0=d19 v1=x9 v2=x10 v3=d20 v4=x9 v5=d20 v6=x9 v7=d20 v8=x9 v9=d20 v10=x9 v11=d20 v12=x9 v13=d20 v14=x9 v15=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	mov	x9, x0
Lunary_block0:
	; v2 = iconst.i64 8
	; v3 = fsqrt v0
	fsqrt	d20, d19
	; store v3, v1
	str	d20, [x9]
	; v4 = ptradd v1, v2
	add	x9, x9, #8
	; v5 = fceil v0
	frintp	d20, d19
	; store v5, v4
	str	d20, [x9]
	; v6 = ptradd v4, v2
	add	x9, x9, #8
	; v7 = ffloor v0
	frintm	d20, d19
	; store v7, v6
	str	d20, [x9]
	; v8 = ptradd v6, v2
	add	x9, x9, #8
	; v9 = ftrunc v0
	frintz	d20, d19
	; store v9, v8
	str	d20, [x9]
	; v10 = ptradd v8, v2
	add	x9, x9, #8
	; v11 = fnearest v0
	frintn	d20, d19
	; store v11, v10
	str	d20, [x9]
	; v12 = ptradd v10, v2
	add	x9, x9, #8
	; v13 = fneg v0
	fneg	d20, d19
	; store v13, v12
	str	d20, [x9]
	; v14 = ptradd v12, v2
	add	x9, x9, #8
	; v15 = fabs v0
	fabs	d19, d19
	; store v15, v14
	str	d19, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lunary_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_unary32
	.p2align	2
_unary32:
Lunary32_begin:
	; regalloc: v0=d19 v1=d20 v2=x9 v3=x10 v4=d21 v5=x9 v6=d21 v7=x9 v8=d21 v9=x9 v10=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
	mov	x9, x0
Lunary32_block0:
	; v3 = iconst.i64 4
	; v4 = fsqrt v0
	fsqrt	s21, s19
	; store v4, v2
	str	s21, [x9]
	; v5 = ptradd v2, v3
	add	x9, x9, #4
	; v6 = fnearest v0
	frintn	s21, s19
	; store v6, v5
	str	s21, [x9]
	; v7 = ptradd v5, v3
	add	x9, x9, #4
	; v8 = fneg v0
	fneg	s21, s19
	; store v8, v7
	str	s21, [x9]
	; v9 = ptradd v7, v3
	add	x9, x9, #4
	; v10 = fdiv v0, v1
	fdiv	s19, s19, s20
	; store v10, v9
	str	s19, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lunary32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_minmax
	.p2align	2
_minmax:
Lminmax_begin:
	; regalloc: v0=d19 v1=d20 v2=x9 v3=d21 v4=x10 v5=x9 v6=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
	mov	x9, x0
Lminmax_block0:
	; v3 = fmin v0, v1
	fmin	d21, d19, d20
	; store v3, v2
	str	d21, [x9]
	; v4 = iconst.i64 8
	; v5 = ptradd v2, v4
	add	x9, x9, #8
	; v6 = fmax v0, v1
	fmax	d19, d19, d20
	; store v6, v5
	str	d19, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lminmax_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_compare
	.p2align	2
_compare:
Lcompare_begin:
	; regalloc: v0=d19 v1=d20 v2=x9 v3=x10 v4=x11 v5=x12 v6=x13 v7=x14 v8=x15 v9=x19 v10=x9 v11=x10 v12=x11 v13=x12 v14=x13 v15=x14 v16=x15 v17=x19 v18=x20 v19=x21 v20=x22 v21=x23 v22=x24 v23=x25 v24=x26 v25=x10 v26=x11 v27=x12 v28=x13 v29=x14 v30=x15 v31=x19 v32=x9 v33=x9 v34=x9 v35=x9 v36=x9 v37=x9 v38=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	stp	x21, x22, [sp, #-16]!
	.cfi_offset x21, -48
	.cfi_offset x22, -40
	stp	x23, x24, [sp, #-16]!
	.cfi_offset x23, -64
	.cfi_offset x24, -56
	stp	x25, x26, [sp, #-16]!
	.cfi_offset x25, -80
	.cfi_offset x26, -72
	fmov	d19, d0
	fmov	d20, d1
Lcompare_block0:
	; v2 = fcmp.eq v0, v1
	fcmp	d19, d20
	cset	w9, eq
	; v3 = fcmp.ne v0, v1
	fcmp	d19, d20
	cset	w10, ne
	; v4 = fcmp.lt v0, v1
	fcmp	d19, d20
	cset	w11, mi
	; v5 = fcmp.le v0, v1
	fcmp	d19, d20
	cset	w12, ls
	; v6 = fcmp.gt v0, v1
	fcmp	d19, d20
	cset	w13, gt
	; v7 = fcmp.ge v0, v1
	fcmp	d19, d20
	cset	w14, ge
	; v8 = fcmp.ord v0, v1
	fcmp	d19, d20
	cset	w15, vc
	; v9 = fcmp.uno v0, v1
	fcmp	d19, d20
	cset	w19, vs
	; v10 = uext.i32 v2
	; v11 = uext.i32 v3
	; v12 = uext.i32 v4
	; v13 = uext.i32 v5
	; v14 = uext.i32 v6
	; v15 = uext.i32 v7
	; v16 = uext.i32 v8
	; v17 = uext.i32 v9
	; v18 = iconst.i32 1
	; v19 = iconst.i32 2
	; v20 = iconst.i32 3
	; v21 = iconst.i32 4
	; v22 = iconst.i32 5
	; v23 = iconst.i32 6
	; v24 = iconst.i32 7
	; v25 = shl v11, v18
	lsl	w10, w10, #1
	; v26 = shl v12, v19
	lsl	w11, w11, #2
	; v27 = shl v13, v20
	lsl	w12, w12, #3
	; v28 = shl v14, v21
	lsl	w13, w13, #4
	; v29 = shl v15, v22
	lsl	w14, w14, #5
	; v30 = shl v16, v23
	lsl	w15, w15, #6
	; v31 = shl v17, v24
	lsl	w19, w19, #7
	; v32 = or v10, v25
	orr	w9, w9, w10
	; v33 = or v32, v26
	orr	w9, w9, w11
	; v34 = or v33, v27
	orr	w9, w9, w12
	; v35 = or v34, v28
	orr	w9, w9, w13
	; v36 = or v35, v29
	orr	w9, w9, w14
	; v37 = or v36, v30
	orr	w9, w9, w15
	; v38 = or v37, v31
	orr	w9, w9, w19
	; return v38
	mov	x0, x9
	sub	sp, x29, #64
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lcompare_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_compare32
	.p2align	2
_compare32:
Lcompare32_begin:
	; regalloc: v0=d19 v1=d20 v2=x9 v3=x10 v4=x9 v5=x10 v6=x11 v7=x10 v8=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
Lcompare32_block0:
	; v2 = fcmp.lt v0, v1
	fcmp	s19, s20
	cset	w9, mi
	; v3 = fcmp.uno v0, v1
	fcmp	s19, s20
	cset	w10, vs
	; v4 = uext.i32 v2
	; v5 = uext.i32 v3
	; v6 = iconst.i32 1
	; v7 = shl v5, v6
	lsl	w10, w10, #1
	; v8 = or v4, v7
	orr	w9, w9, w10
	; return v8
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lcompare32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_to_int
	.p2align	2
_to_int:
Lto_int_begin:
	; regalloc: v0=d19 v1=x9 v2=x10 v3=x11 v4=x11 v5=x9 v6=x11 v7=x9 v8=x11 v9=x11 v10=x9 v11=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	mov	x9, x0
Lto_int_block0:
	; v2 = iconst.i64 8
	; v3 = fptosi.i32 v0
	fcvtzs	w11, d19
	; v4 = sext.i64 v3
	sxtw	x11, w11
	; store v4, v1
	str	x11, [x9]
	; v5 = ptradd v1, v2
	add	x9, x9, #8
	; v6 = fptosi.i64 v0
	fcvtzs	x11, d19
	; store v6, v5
	str	x11, [x9]
	; v7 = ptradd v5, v2
	add	x9, x9, #8
	; v8 = fptoui.i32 v0
	fcvtzu	w11, d19
	; v9 = uext.i64 v8
	mov	w11, w11
	; store v9, v7
	str	x11, [x9]
	; v10 = ptradd v7, v2
	add	x9, x9, #8
	; v11 = fptoui.i64 v0
	fcvtzu	x10, d19
	; store v11, v10
	str	x10, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lto_int_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_to_int32
	.p2align	2
_to_int32:
Lto_int32_begin:
	; regalloc: v0=d19 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
Lto_int32_block0:
	; v1 = fptosi.i32 v0
	fcvtzs	w9, s19
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lto_int32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_from_int
	.p2align	2
_from_int:
Lfrom_int_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=d19 v5=x11 v6=d19 v7=x9 v8=d19 v9=x9 v10=d19 v11=x9 v12=d19 v13=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lfrom_int_block0:
	; v3 = iconst.i64 8
	; v4 = sitofp.f64 v0
	sxtb	w16, w9
	scvtf	d19, w16
	; store v4, v2
	str	d19, [x11]
	; v5 = ptradd v2, v3
	add	x11, x11, #8
	; v6 = uitofp.f64 v0
	uxtb	w16, w9
	ucvtf	d19, w16
	; store v6, v5
	str	d19, [x11]
	; v7 = ptradd v5, v3
	add	x9, x11, #8
	; v8 = sitofp.f64 v1
	scvtf	d19, x10
	; store v8, v7
	str	d19, [x9]
	; v9 = ptradd v7, v3
	add	x9, x9, #8
	; v10 = uitofp.f64 v1
	ucvtf	d19, x10
	; store v10, v9
	str	d19, [x9]
	; v11 = ptradd v9, v3
	add	x9, x9, #8
	; v12 = uitofp.f32 v1
	ucvtf	s19, x10
	; v13 = fpext.f64 v12
	fcvt	d19, s19
	; store v13, v11
	str	d19, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lfrom_int_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_bits
	.p2align	2
_bits:
Lbits_begin:
	; regalloc: v0=d19 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
Lbits_block0:
	; v1 = bitcast.i64 v0
	fmov	x9, d19
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lbits_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_from_bits32
	.p2align	2
_from_bits32:
Lfrom_bits32_begin:
	; regalloc: v0=x9 v1=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lfrom_bits32_block0:
	; v1 = bitcast.f32 v0
	fmov	s19, w9
	; return v1
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lfrom_bits32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_narrow
	.p2align	2
_narrow:
Lnarrow_begin:
	; regalloc: v0=d19 v1=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
Lnarrow_block0:
	; v1 = fptrunc.f32 v0
	fcvt	s19, d19
	; return v1
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lnarrow_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_neg_zero
	.p2align	2
_neg_zero:
Lneg_zero_begin:
	; regalloc: v0=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lneg_zero_block0:
	; v0 = fconst.f64 -0.0
	movz	x16, #0x8000, lsl #48
	fmov	d19, x16
	; return v0
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lneg_zero_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_snan
	.p2align	2
_snan:
Lsnan_begin:
	; regalloc: v0=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lsnan_block0:
	; v0 = fconst.f64 0x7ff0000000000001
	movz	x16, #0x1
	movk	x16, #0x7ff0, lsl #48
	fmov	d19, x16
	; return v0
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lsnan_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tiny
	.p2align	2
_tiny:
Ltiny_begin:
	; regalloc: v0=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Ltiny_block0:
	; v0 = fconst.f64 5e-324
	movz	x16, #0x1
	fmov	d19, x16
	; return v0
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Ltiny_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tenth32
	.p2align	2
_tenth32:
Ltenth32_begin:
	; regalloc: v0=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Ltenth32_block0:
	; v0 = fconst.f32 0.1
	movz	w16, #0xcccd
	movk	w16, #0x3dcc, lsl #16
	fmov	s19, w16
	; return v0
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Ltenth32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_select_f
	.p2align	2
_select_f:
Lselect_f_begin:
	; regalloc: v0=x9 v1=d19 v2=d20 v3=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	fmov	d19, d0
	fmov	d20, d1
Lselect_f_block0:
	; v3 = select v0, v1, v2
	cmp	w9, #0
	fcsel	d19, d19, d20, ne
	; return v3
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lselect_f_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_harmonic
	.p2align	2
_harmonic:
Lharmonic_begin:
	; regalloc: v0=x9 v1=d19 v2=x10 v3=x11 v4=d19 v6=d20 v7=d21 v8=d20 v9=d19 v10=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lharmonic_block0:
	; v1 = fconst.f64 0.0
	fmov	d19, xzr
	; v2 = iconst.i64 1
	movz	x10, #0x1
	; jump block1(v2, v1)
	mov	x11, x10
Lharmonic_block1:
	; v5 = icmp.sle v3, v0
	; brif v5, block2, block3
	cmp	x11, x9
	b.le	Lharmonic_block2
Lharmonic_block3:
	; return v4
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lharmonic_block2:
	; v6 = sitofp.f64 v3
	scvtf	d20, x11
	; v7 = fconst.f64 1.0
	fmov	d21, #1.0
	; v8 = fdiv v7, v6
	fdiv	d20, d21, d20
	; v9 = fadd v4, v8
	fadd	d19, d19, d20
	; v10 = iadd v3, v2
	add	x11, x11, #1
	; jump block1(v10, v9)
	b	Lharmonic_block1
Lharmonic_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_swaps
	.p2align	2
_swaps:
Lswaps_begin:
	; regalloc: v0=d19 v1=d20 v2=x9 v3=x10 v4=d19 v5=d20 v6=x10 v8=x11 v9=x10 v10=d21 v11=d19 v12=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
	mov	x9, x0
Lswaps_block0:
	; v3 = iconst.i64 0
	movz	x10, #0x0
	; jump block1(v0, v1, v3)
Lswaps_block1:
	; v7 = icmp.ult v6, v2
	; brif v7, block2, block3
	cmp	x10, x9
	b.lo	Lswaps_block2
Lswaps_block3:
	; v10 = fconst.f64 10.0
	fmov	d21, #10.0
	; v11 = fmul v4, v10
	fmul	d19, d19, d21
	; v12 = fadd v11, v5
	fadd	d19, d19, d20
	; return v12
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lswaps_block2:
	; v8 = iconst.i64 1
	; v9 = iadd v6, v8
	add	x10, x10, #1
	; jump block1(v5, v4, v9)
	fmov	x16, d19
	fmov	d19, d20
	fmov	d20, x16
	b	Lswaps_block1
Lswaps_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_pressure
	.p2align	2
_pressure:
Lpressure_begin:
	; regalloc: v0=[sp, #0] v1=d19 v2=[sp, #8] v3=d10 v4=d11 v5=d12 v6=d13 v7=d14 v8=d15 v9=d8 v10=d9 v11=[sp, #16] v12=[sp, #24] v13=[sp, #32] v14=[sp, #40] v15=[sp, #48] v16=[sp, #56] v17=[sp, #64] v18=[sp, #72] v19=[sp, #80] v20=[sp, #88] v21=d19 v22=d19 v23=d19 v24=d19 v25=d19 v26=d19 v27=d19 v28=d19 v29=d19 v30=d19 v31=d19 v32=d19 v33=d19 v34=d19 v35=d19 v36=d19 v37=d19 v38=d19 v39=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	d8, d9, [sp, #-16]!
	.cfi_offset d8, -32
	.cfi_offset d9, -24
	stp	d10, d11, [sp, #-16]!
	.cfi_offset d10, -48
	.cfi_offset d11, -40
	stp	d12, d13, [sp, #-16]!
	.cfi_offset d12, -64
	.cfi_offset d13, -56
	stp	d14, d15, [sp, #-16]!
	.cfi_offset d14, -80
	.cfi_offset d15, -72
	sub	sp, sp, #96
	str	d0, [sp, #0]
Lpressure_block0:
	; v1 = fconst.f64 1.0
	fmov	d19, #1.0
	; v2 = fadd v0, v1
	ldr	d16, [sp, #0]
	fadd	d16, d16, d19
	str	d16, [sp, #8]
	; v3 = fadd v2, v1
	ldr	d16, [sp, #8]
	fadd	d10, d16, d19
	; v4 = fadd v3, v1
	fadd	d11, d10, d19
	; v5 = fadd v4, v1
	fadd	d12, d11, d19
	; v6 = fadd v5, v1
	fadd	d13, d12, d19
	; v7 = fadd v6, v1
	fadd	d14, d13, d19
	; v8 = fadd v7, v1
	fadd	d15, d14, d19
	; v9 = fadd v8, v1
	fadd	d8, d15, d19
	; v10 = fadd v9, v1
	fadd	d9, d8, d19
	; v11 = fadd v10, v1
	fadd	d16, d9, d19
	str	d16, [sp, #16]
	; v12 = fadd v11, v1
	ldr	d16, [sp, #16]
	fadd	d16, d16, d19
	str	d16, [sp, #24]
	; v13 = fadd v12, v1
	ldr	d16, [sp, #24]
	fadd	d16, d16, d19
	str	d16, [sp, #32]
	; v14 = fadd v13, v1
	ldr	d16, [sp, #32]
	fadd	d16, d16, d19
	str	d16, [sp, #40]
	; v15 = fadd v14, v1
	ldr	d16, [sp, #40]
	fadd	d16, d16, d19
	str	d16, [sp, #48]
	; v16 = fadd v15, v1
	ldr	d16, [sp, #48]
	fadd	d16, d16, d19
	str	d16, [sp, #56]
	; v17 = fadd v16, v1
	ldr	d16, [sp, #56]
	fadd	d16, d16, d19
	str	d16, [sp, #64]
	; v18 = fadd v17, v1
	ldr	d16, [sp, #64]
	fadd	d16, d16, d19
	str	d16, [sp, #72]
	; v19 = fadd v18, v1
	ldr	d16, [sp, #72]
	fadd	d16, d16, d19
	str	d16, [sp, #80]
	; v20 = fadd v19, v1
	ldr	d16, [sp, #80]
	fadd	d16, d16, d19
	str	d16, [sp, #88]
	; call @fclobber()
	bl	_fclobber
	; v21 = fmul v2, v3
	ldr	d16, [sp, #8]
	fmul	d19, d16, d10
	; v22 = fadd v21, v4
	fadd	d19, d19, d11
	; v23 = fmul v22, v5
	fmul	d19, d19, d12
	; v24 = fadd v23, v6
	fadd	d19, d19, d13
	; v25 = fsub v24, v7
	fsub	d19, d19, d14
	; v26 = fadd v25, v8
	fadd	d19, d19, d15
	; v27 = fsub v26, v9
	fsub	d19, d19, d8
	; v28 = fadd v27, v10
	fadd	d19, d19, d9
	; v29 = fmul v28, v11
	ldr	d17, [sp, #16]
	fmul	d19, d19, d17
	; v30 = fadd v29, v12
	ldr	d17, [sp, #24]
	fadd	d19, d19, d17
	; v31 = fsub v30, v13
	ldr	d17, [sp, #32]
	fsub	d19, d19, d17
	; v32 = fadd v31, v14
	ldr	d17, [sp, #40]
	fadd	d19, d19, d17
	; v33 = fadd v32, v15
	ldr	d17, [sp, #48]
	fadd	d19, d19, d17
	; v34 = fsub v33, v16
	ldr	d17, [sp, #56]
	fsub	d19, d19, d17
	; v35 = fadd v34, v17
	ldr	d17, [sp, #64]
	fadd	d19, d19, d17
	; v36 = fadd v35, v18
	ldr	d17, [sp, #72]
	fadd	d19, d19, d17
	; v37 = fsub v36, v19
	ldr	d17, [sp, #80]
	fsub	d19, d19, d17
	; v38 = fadd v37, v20
	ldr	d17, [sp, #88]
	fadd	d19, d19, d17
	; v39 = fadd v38, v0
	ldr	d17, [sp, #0]
	fadd	d19, d19, d17
	; return v39
	fmov	d0, d19
	sub	sp, x29, #64
	ldp	d14, d15, [sp], #16
	ldp	d12, d13, [sp], #16
	ldp	d10, d11, [sp], #16
	ldp	d8, d9, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lpressure_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_consts
	.p2align	2
_consts:
Lconsts_begin:
	; regalloc: v0=x9 v1=x10 v2=d19 v3=d20 v4=d19 v5=x9 v6=d19 v7=d20 v8=d19 v9=x9 v10=d19 v11=d19 v12=x9 v13=d19 v14=d19 v15=x9 v16=d19 v17=d20 v18=d21 v19=x9 v20=d21 v21=d21 v22=x9 v23=x11 v24=x9 v25=d22 v26=d23 v27=d22 v28=d22 v29=x9 v30=x11 v31=d22 v32=x9 v33=x10 v34=x11 v35=x10 v36=x11 v37=x12 v38=x11 v39=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lconsts_block0:
	; v1 = iconst.i64 8
	; v2 = fconst.f64 0.1
	movz	x16, #0x999a
	movk	x16, #0x9999, lsl #16
	movk	x16, #0x9999, lsl #32
	movk	x16, #0x3fb9, lsl #48
	fmov	d19, x16
	; v3 = fconst.f64 0.2
	movz	x16, #0x999a
	movk	x16, #0x9999, lsl #16
	movk	x16, #0x9999, lsl #32
	movk	x16, #0x3fc9, lsl #48
	fmov	d20, x16
	; v4 = fadd v2, v3
	fadd	d19, d19, d20
	; store v4, v0
	str	d19, [x9]
	; v5 = ptradd v0, v1
	add	x9, x9, #8
	; v6 = fconst.f64 1.0
	fmov	d19, #1.0
	; v7 = fconst.f64 3.0
	fmov	d20, #3.0
	; v8 = fdiv v6, v7
	fdiv	d19, d19, d20
	; store v8, v5
	str	d19, [x9]
	; v9 = ptradd v5, v1
	add	x9, x9, #8
	; v10 = fconst.f64 2.0
	fmov	d19, #2.0
	; v11 = fsqrt v10
	fsqrt	d19, d19
	; store v11, v9
	str	d19, [x9]
	; v12 = ptradd v9, v1
	add	x9, x9, #8
	; v13 = fconst.f64 -2.5
	fmov	d19, #-2.5
	; v14 = fnearest v13
	frintn	d19, d19
	; store v14, v12
	str	d19, [x9]
	; v15 = ptradd v12, v1
	add	x9, x9, #8
	; v16 = fconst.f64 0.0
	fmov	d19, xzr
	; v17 = fconst.f64 -0.0
	movz	x16, #0x8000, lsl #48
	fmov	d20, x16
	; v18 = fmin v16, v17
	fmin	d21, d19, d20
	; store v18, v15
	str	d21, [x9]
	; v19 = ptradd v15, v1
	add	x9, x9, #8
	; v20 = fconst.f64 1e300
	movz	x16, #0x759c
	movk	x16, #0x8800, lsl #16
	movk	x16, #0xe43c, lsl #32
	movk	x16, #0x7e37, lsl #48
	fmov	d21, x16
	; v21 = fmul v20, v20
	fmul	d21, d21, d21
	; store v21, v19
	str	d21, [x9]
	; v22 = ptradd v19, v1
	add	x9, x9, #8
	; v23 = fptosi.i64 v21
	fcvtzs	x11, d21
	; store v23, v22
	str	x11, [x9]
	; v24 = ptradd v22, v1
	add	x9, x9, #8
	; v25 = fconst.f32 16777216.0
	movz	w16, #0x4b80, lsl #16
	fmov	s22, w16
	; v26 = fconst.f32 1.0
	fmov	s23, #1.0
	; v27 = fadd v25, v26
	fadd	s22, s22, s23
	; v28 = fpext.f64 v27
	fcvt	d22, s22
	; store v28, v24
	str	d22, [x9]
	; v29 = ptradd v24, v1
	add	x9, x9, #8
	; v30 = iconst.i64 -1
	movn	x11, #0x0
	; v31 = uitofp.f64 v30
	ucvtf	d22, x11
	; store v31, v29
	str	d22, [x9]
	; v32 = ptradd v29, v1
	add	x9, x9, #8
	; v33 = fcmp.lt v16, v17
	fcmp	d19, d20
	cset	w10, mi
	; v34 = fcmp.ne v21, v21
	fcmp	d21, d21
	cset	w11, ne
	; v35 = uext.i64 v33
	; v36 = uext.i64 v34
	; v37 = iconst.i64 2
	movz	x12, #0x2
	; v38 = imul v36, v37
	mul	x11, x11, x12
	; v39 = iadd v35, v38
	add	x10, x10, x11
	; store v39, v32
	str	x10, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lconsts_end:
	.cfi_endproc
	.subsections_via_symbols
