	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_many
	.p2align	2
_k_many:
Lk_many_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19 v8=d19 v9=d20 v10=d21 v11=d22 v12=d23 v13=d24 v14=d25 v15=d26 v16=x20 v17=x21 v18=x22 v19=d27 v20=d28 v21=x23 v22=x24 v23=x9 v24=x24 v25=x10 v26=x9 v27=x10 v28=x10 v29=x9 v30=x10 v31=x10 v32=x9 v33=x10 v34=x10 v35=x9 v36=x10 v37=x10 v38=x9 v39=x10 v40=x10 v41=x9 v42=x10 v43=x10 v44=x9 v45=d29 v46=d30 v47=d19 v48=d30 v49=d20 v50=d19 v51=d20 v52=d20 v53=d19 v54=d20 v55=d20 v56=d19 v57=d20 v58=d20 v59=d19 v60=d20 v61=d20 v62=d19 v63=d20 v64=d20 v65=d19 v66=d20 v67=d20 v68=d19 v69=d19 v70=d20 v71=d21 v72=d20 v73=d21 v74=d22 v75=d21 v76=d22 v77=d23 v78=d22 v79=d23 v80=d24 v81=d23 v82=d24 v83=d24 v84=d25 v85=d26 v86=d25 v87=d19 v88=d19 v89=d19 v90=d19 v91=d19 v92=d19
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
	ldr	w20, [x29, #16]
	ldr	x21, [x29, #24]
	ldr	w22, [x29, #32]
	ldr	s27, [x29, #36]
	ldr	d28, [x29, #40]
	ldr	w23, [x29, #48]
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
	mov	x14, x5
	mov	x15, x6
	mov	x19, x7
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
	fmov	d22, d3
	fmov	d23, d4
	fmov	d24, d5
	fmov	d25, d6
	fmov	d26, d7
Lk_many_block0:
	; v22 = iconst.i64 1
	movz	x24, #0x1
	; v23 = imul v0, v22
	mul	x9, x9, x24
	; v24 = iconst.i64 2
	movz	x24, #0x2
	; v25 = imul v1, v24
	mul	x10, x10, x24
	; v26 = iadd v23, v25
	add	x9, x9, x10
	; v27 = iconst.i64 3
	movz	x10, #0x3
	; v28 = imul v2, v27
	mul	x10, x11, x10
	; v29 = iadd v26, v28
	add	x9, x9, x10
	; v30 = iconst.i64 4
	movz	x10, #0x4
	; v31 = imul v3, v30
	mul	x10, x12, x10
	; v32 = iadd v29, v31
	add	x9, x9, x10
	; v33 = iconst.i64 5
	movz	x10, #0x5
	; v34 = imul v4, v33
	mul	x10, x13, x10
	; v35 = iadd v32, v34
	add	x9, x9, x10
	; v36 = iconst.i64 6
	movz	x10, #0x6
	; v37 = imul v5, v36
	mul	x10, x14, x10
	; v38 = iadd v35, v37
	add	x9, x9, x10
	; v39 = iconst.i64 7
	movz	x10, #0x7
	; v40 = imul v6, v39
	mul	x10, x15, x10
	; v41 = iadd v38, v40
	add	x9, x9, x10
	; v42 = iconst.i64 8
	movz	x10, #0x8
	; v43 = imul v7, v42
	mul	x10, x19, x10
	; v44 = iadd v41, v43
	add	x9, x9, x10
	; v45 = sitofp.f64 v44
	scvtf	d29, x9
	; v46 = fconst.f64 11.0
	fmov	d30, #11.0
	; v47 = fmul v8, v46
	fmul	d19, d19, d30
	; v48 = fconst.f64 12.0
	fmov	d30, #12.0
	; v49 = fmul v9, v48
	fmul	d20, d20, d30
	; v50 = fadd v47, v49
	fadd	d19, d19, d20
	; v51 = fconst.f64 13.0
	fmov	d20, #13.0
	; v52 = fmul v10, v51
	fmul	d20, d21, d20
	; v53 = fadd v50, v52
	fadd	d19, d19, d20
	; v54 = fconst.f64 14.0
	fmov	d20, #14.0
	; v55 = fmul v11, v54
	fmul	d20, d22, d20
	; v56 = fadd v53, v55
	fadd	d19, d19, d20
	; v57 = fconst.f64 15.0
	fmov	d20, #15.0
	; v58 = fmul v12, v57
	fmul	d20, d23, d20
	; v59 = fadd v56, v58
	fadd	d19, d19, d20
	; v60 = fconst.f64 16.0
	fmov	d20, #16.0
	; v61 = fmul v13, v60
	fmul	d20, d24, d20
	; v62 = fadd v59, v61
	fadd	d19, d19, d20
	; v63 = fconst.f64 17.0
	fmov	d20, #17.0
	; v64 = fmul v14, v63
	fmul	d20, d25, d20
	; v65 = fadd v62, v64
	fadd	d19, d19, d20
	; v66 = fconst.f64 18.0
	fmov	d20, #18.0
	; v67 = fmul v15, v66
	fmul	d20, d26, d20
	; v68 = fadd v65, v67
	fadd	d19, d19, d20
	; v69 = fadd v45, v68
	fadd	d19, d29, d19
	; v70 = sitofp.f64 v16
	scvtf	d20, w20
	; v71 = fconst.f64 101.0
	movz	x16, #0x4000, lsl #32
	movk	x16, #0x4059, lsl #48
	fmov	d21, x16
	; v72 = fmul v70, v71
	fmul	d20, d20, d21
	; v73 = sitofp.f64 v17
	scvtf	d21, x21
	; v74 = fconst.f64 103.0
	movz	x16, #0xc000, lsl #32
	movk	x16, #0x4059, lsl #48
	fmov	d22, x16
	; v75 = fmul v73, v74
	fmul	d21, d21, d22
	; v76 = sitofp.f64 v18
	scvtf	d22, w22
	; v77 = fconst.f64 107.0
	movz	x16, #0xc000, lsl #32
	movk	x16, #0x405a, lsl #48
	fmov	d23, x16
	; v78 = fmul v76, v77
	fmul	d22, d22, d23
	; v79 = fpext.f64 v19
	fcvt	d23, s27
	; v80 = fconst.f64 109.0
	movz	x16, #0x4000, lsl #32
	movk	x16, #0x405b, lsl #48
	fmov	d24, x16
	; v81 = fmul v79, v80
	fmul	d23, d23, d24
	; v82 = fconst.f64 113.0
	movz	x16, #0x4000, lsl #32
	movk	x16, #0x405c, lsl #48
	fmov	d24, x16
	; v83 = fmul v20, v82
	fmul	d24, d28, d24
	; v84 = sitofp.f64 v21
	scvtf	d25, w23
	; v85 = fconst.f64 127.0
	movz	x16, #0xc000, lsl #32
	movk	x16, #0x405f, lsl #48
	fmov	d26, x16
	; v86 = fmul v84, v85
	fmul	d25, d25, d26
	; v87 = fadd v69, v72
	fadd	d19, d19, d20
	; v88 = fadd v87, v75
	fadd	d19, d19, d21
	; v89 = fadd v88, v78
	fadd	d19, d19, d22
	; v90 = fadd v89, v81
	fadd	d19, d19, d23
	; v91 = fadd v90, v83
	fadd	d19, d19, d24
	; v92 = fadd v91, v86
	fadd	d19, d19, d25
	; return v92
	fmov	d0, d19
	sub	sp, x29, #48
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_many_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_call_many
	.p2align	2
_k_call_many:
Lk_call_many_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19 v8=x20 v9=x11 v10=d19 v11=d20 v12=d20 v13=d21 v14=d22 v15=d23 v16=d24 v17=d25 v18=d26 v19=d27 v20=x21 v21=x22 v22=x23 v23=x23 v24=x24 v25=x24 v26=d28 v27=d29 v28=d28 v29=d29 v30=d19 v31=x25 v32=x21 v33=d19
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
	str	x25, [sp, #-16]!
	.cfi_offset x25, -80
	sub	sp, sp, #48
	mov	x9, x0
	mov	x10, x1
Lk_call_many_block0:
	; v2 = iconst.i64 1
	; v3 = iadd v0, v2
	add	x12, x9, #1
	; v4 = iadd v3, v2
	add	x13, x12, #1
	; v5 = iadd v4, v2
	add	x14, x13, #1
	; v6 = iadd v5, v2
	add	x15, x14, #1
	; v7 = iadd v6, v2
	add	x19, x15, #1
	; v8 = iadd v7, v2
	add	x20, x19, #1
	; v9 = iadd v8, v2
	add	x11, x20, #1
	; v10 = sitofp.f64 v0
	scvtf	d19, x9
	; v11 = fconst.f64 0.25
	fmov	d20, #0.25
	; v12 = fmul v10, v11
	fmul	d20, d19, d20
	; v13 = fadd v12, v12
	fadd	d21, d20, d20
	; v14 = fadd v13, v12
	fadd	d22, d21, d20
	; v15 = fadd v14, v12
	fadd	d23, d22, d20
	; v16 = fadd v15, v12
	fadd	d24, d23, d20
	; v17 = fadd v16, v12
	fadd	d25, d24, d20
	; v18 = fadd v17, v12
	fadd	d26, d25, d20
	; v19 = fadd v18, v12
	fadd	d27, d26, d20
	; v20 = trunc.i32 v0
	mov	x21, x9
	; v21 = ineg v20
	neg	w22, w21
	; v22 = iconst.i64 33
	; v23 = shl v0, v22
	lsl	x23, x9, #33
	; v24 = iconst.i32 7
	movz	w24, #0x7
	; v25 = isub v24, v20
	sub	w24, w24, w21
	; v26 = fptrunc.f32 v10
	fcvt	s28, d19
	; v27 = fconst.f32 0.125
	fmov	s29, #0.125
	; v28 = fmul v26, v27
	fmul	s28, s28, s29
	; v29 = fconst.f64 1.5
	fmov	d29, #1.5
	; v30 = fmul v10, v29
	fmul	d19, d19, d29
	; v31 = iconst.i32 1000
	movz	w25, #0x3e8
	; v32 = imul v20, v31
	mul	w21, w21, w25
	; v33 = call_indirect v1(v0, v3, v4, v5, v6, v7, v8, v9, v12, v13, v14, v15, v16, v17, v18, v19, v21, v23, v25, v28, v30, v32) : (i64, i64, i64, i64, i64, i64, i64, i64, f64, f64, f64, f64, f64, f64, f64, f64, i32, i64, i32, f32, f64, i32) -> f64 cc(c)
	str	w22, [sp, #0]
	str	x23, [sp, #8]
	str	w24, [sp, #16]
	str	s28, [sp, #20]
	str	d19, [sp, #24]
	str	w21, [sp, #32]
	mov	x0, x9
	mov	x1, x12
	mov	x2, x13
	mov	x3, x14
	mov	x4, x15
	mov	x5, x19
	mov	x6, x20
	mov	x7, x11
	fmov	d0, d20
	fmov	d1, d21
	fmov	d2, d22
	fmov	d3, d23
	fmov	d4, d24
	fmov	d5, d25
	fmov	d6, d26
	fmov	d7, d27
	blr	x10
	fmov	d19, d0
	; return v33
	fmov	d0, d19
	sub	sp, x29, #64
	ldr	x25, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_call_many_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_call_c_many
	.p2align	2
_k_call_c_many:
Lk_call_c_many_begin:
	; regalloc: v0=x9 v1=x10 v2=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_call_c_many_block0:
	; v1 = symbol_addr @c_many
	adrp	x10, _c_many@GOTPAGE
	ldr	x10, [x10, _c_many@GOTPAGEOFF]
	; v2 = call @k_call_many(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_k_call_many
	fmov	d19, d0
	; return v2
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lk_call_c_many_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_print
	.p2align	2
_k_print:
Lk_print_begin:
	; regalloc: v0=x9 v1=x10 v2=d19 v3=x11 v4=x12 v5=x13 v6=x14 v7=x15 v8=x19 v9=x20 v10=x21 v11=x22 v12=x23 v13=x9 v14=x9 v15=x9
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
	str	x23, [sp, #-16]!
	.cfi_offset x23, -64
	sub	sp, sp, #96
Lk_print_block0:
	; v0 = symbol_addr @fmt
	adrp	x9, _fmt@PAGE
	add	x9, x9, _fmt@PAGEOFF
	; v1 = iconst.i32 7
	movz	w10, #0x7
	; v2 = fconst.f64 2.5
	fmov	d19, #2.5
	; v3 = symbol_addr @hi
	adrp	x11, _hi@PAGE
	add	x11, x11, _hi@PAGEOFF
	; v4 = iconst.i64 123456789012
	movz	x12, #0x1a14
	movk	x12, #0xbe99, lsl #16
	movk	x12, #0x1c, lsl #32
	; v5 = iconst.i32 -1
	movn	w13, #0x0
	; v6 = iconst.i32 2
	movz	w14, #0x2
	; v7 = iconst.i32 3
	movz	w15, #0x3
	; v8 = iconst.i32 4
	movz	w19, #0x4
	; v9 = iconst.i32 5
	movz	w20, #0x5
	; v10 = iconst.i32 6
	movz	w21, #0x6
	; v11 = iconst.i32 7
	movz	w22, #0x7
	; v12 = iconst.i32 8
	movz	w23, #0x8
	; v13 = call @printf(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12)
	str	w10, [sp, #0]
	str	d19, [sp, #8]
	str	x11, [sp, #16]
	str	x12, [sp, #24]
	str	w13, [sp, #32]
	str	w14, [sp, #40]
	str	w15, [sp, #48]
	str	w19, [sp, #56]
	str	w20, [sp, #64]
	str	w21, [sp, #72]
	str	w22, [sp, #80]
	str	w23, [sp, #88]
	mov	x0, x9
	bl	_printf
	mov	x9, x0
	; v14 = symbol_addr @plain
	adrp	x9, _plain@PAGE
	add	x9, x9, _plain@PAGEOFF
	; v15 = call @printf(v14)
	mov	x0, x9
	bl	_printf
	mov	x9, x0
	; return
	sub	sp, x29, #48
	ldr	x23, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_print_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_vsum
	.p2align	2
_k_vsum:
Lk_vsum_begin:
	; regalloc: v0=x9 v1=x10 v2=d19 v3=x11 v4=x12 v5=d20 v6=d21 v7=x13 v8=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #64
Lk_vsum_block0:
	; v0 = symbol_addr @types
	adrp	x9, _types@PAGE
	add	x9, x9, _types@PAGEOFF
	; v1 = iconst.i32 1
	movz	w10, #0x1
	; v2 = fconst.f64 2.5
	fmov	d19, #2.5
	; v3 = iconst.i64 3
	movz	x11, #0x3
	; v4 = iconst.i32 -4
	movn	w12, #0x3
	; v5 = fconst.f64 0.25
	fmov	d20, #0.25
	; v6 = fconst.f64 -1.0
	fmov	d21, #-1.0
	; v7 = symbol_addr @c_vsum
	adrp	x13, _c_vsum@GOTPAGE
	ldr	x13, [x13, _c_vsum@GOTPAGEOFF]
	; v8 = call_indirect v7(v0, v1, v2, v3, v0, v4, v5, v6) : (ptr, ...) -> i64 cc(c)
	str	w10, [sp, #0]
	str	d19, [sp, #8]
	str	x11, [sp, #16]
	str	x9, [sp, #24]
	str	w12, [sp, #32]
	str	d20, [sp, #40]
	str	d21, [sp, #48]
	mov	x0, x9
	blr	x13
	mov	x9, x0
	; return v8
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_vsum_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_vtail
	.p2align	2
_k_vtail:
Lk_vtail_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=d19 v4=x12 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #32
Lk_vtail_block0:
	; v0 = iconst.i64 1
	movz	x9, #0x1
	; v1 = iconst.i32 -9
	movn	w10, #0x8
	; v2 = iconst.i64 100
	movz	x11, #0x64
	; v3 = fconst.f64 0.5
	fmov	d19, #0.5
	; v4 = iconst.i32 3
	movz	w12, #0x3
	; v5 = call @c_vtail(v0, v0, v0, v0, v0, v0, v0, v0, v1, v2, v3, v4)
	str	w10, [sp, #0]
	str	x11, [sp, #8]
	str	d19, [sp, #16]
	str	w12, [sp, #24]
	mov	x0, x9
	mov	x1, x9
	mov	x2, x9
	mov	x3, x9
	mov	x4, x9
	mov	x5, x9
	mov	x6, x9
	mov	x7, x9
	bl	_c_vtail
	mov	x9, x0
	; return v5
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_vtail_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_small
	.p2align	2
_k_small:
Lk_small_begin:
	; regalloc: v7=x9 v8=x10 v9=x11 v10=x12 v11=x13 v12=x10 v13=x11 v14=x14 v15=x11 v16=x12 v17=x14 v18=x12 v19=x13 v20=x14 v21=x13 v22=x10 v23=x10 v24=x10 v25=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	ldrb	w10, [x29, #16]
	ldrh	w11, [x29, #18]
	ldrb	w12, [x29, #20]
	ldr	w13, [x29, #24]
	mov	x9, x7
Lk_small_block0:
	; v12 = sext.i64 v8
	sxtb	x10, w10
	; v13 = sext.i64 v9
	sxth	x11, w11
	; v14 = iconst.i64 10
	movz	x14, #0xa
	; v15 = imul v13, v14
	mul	x11, x11, x14
	; v16 = sext.i64 v10
	sxtb	x12, w12
	; v17 = iconst.i64 1000
	movz	x14, #0x3e8
	; v18 = imul v16, v17
	mul	x12, x12, x14
	; v19 = sext.i64 v11
	sxtw	x13, w13
	; v20 = iconst.i64 100000
	movz	x14, #0x86a0
	movk	x14, #0x1, lsl #16
	; v21 = imul v19, v20
	mul	x13, x13, x14
	; v22 = iadd v12, v15
	add	x10, x10, x11
	; v23 = iadd v22, v18
	add	x10, x10, x12
	; v24 = iadd v23, v21
	add	x10, x10, x13
	; v25 = iadd v24, v7
	add	x9, x10, x9
	; return v25
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_small_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_call_small
	.p2align	2
_k_call_small:
Lk_call_small_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x9, x0
Lk_call_small_block0:
	; v1 = iconst.i8 -3
	movn	w10, #0x2
	; v2 = iconst.i16 -300
	movn	w11, #0x12b
	; v3 = iconst.i8 5
	movz	w12, #0x5
	; v4 = trunc.i32 v0
	mov	x13, x9
	; v5 = call @k_small(v0, v0, v0, v0, v0, v0, v0, v0, v1, v2, v3, v4)
	strb	w10, [sp, #0]
	strh	w11, [sp, #2]
	strb	w12, [sp, #4]
	str	w13, [sp, #8]
	mov	x0, x9
	mov	x1, x9
	mov	x2, x9
	mov	x3, x9
	mov	x4, x9
	mov	x5, x9
	mov	x6, x9
	mov	x7, x9
	bl	_k_small
	mov	x9, x0
	; return v5
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_call_small_end:
	.cfi_endproc
	.globl	_fmt
	.section	__TEXT,__const
	.p2align	0
_fmt:
	.byte	37, 100, 32, 37, 46, 51, 102, 32, 37, 115, 32, 37, 108, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 32, 37, 100, 10, 0
	.globl	_plain
	.section	__TEXT,__const
	.p2align	0
_plain:
	.byte	110, 111, 32, 97, 114, 103, 117, 109, 101, 110, 116, 115, 10, 0
	.globl	_hi
	.section	__TEXT,__const
	.p2align	0
_hi:
	.byte	104, 105, 0
	.globl	_types
	.section	__TEXT,__const
	.p2align	0
_types:
	.byte	105, 100, 108, 112, 105, 100, 100, 0
	.subsections_via_symbols
