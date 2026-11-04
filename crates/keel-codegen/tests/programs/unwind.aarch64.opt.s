	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_through
	.p2align	2
_k_through:
Lk_through_begin:
	; regalloc: v0=x9 v1=x10 v2=d19 v4=x19 v6=x20 v8=x21 v10=x22 v12=x23 v14=x24 v16=x25 v18=x26 v20=x27 v22=x28 v23=d20 v24=d8 v25=d20 v26=d9 v27=d20 v28=d10 v29=d20 v30=d11 v31=d20 v32=d12 v33=d20 v34=d13 v35=d20 v36=d14 v37=d20 v38=d15 v39=x9 v40=x9 v41=x9 v42=x9 v43=x9 v44=x9 v45=x9 v46=x9 v47=x9 v48=x9 v49=x9 v50=d19 v51=d19 v52=d19 v53=d19 v54=d19 v55=d19 v56=d19 v57=x10 v58=x9
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
	stp	x27, x28, [sp, #-16]!
	.cfi_offset x27, -96
	.cfi_offset x28, -88
	stp	d8, d9, [sp, #-16]!
	.cfi_offset d8, -112
	.cfi_offset d9, -104
	stp	d10, d11, [sp, #-16]!
	.cfi_offset d10, -128
	.cfi_offset d11, -120
	stp	d12, d13, [sp, #-16]!
	.cfi_offset d12, -144
	.cfi_offset d13, -136
	stp	d14, d15, [sp, #-16]!
	.cfi_offset d14, -160
	.cfi_offset d15, -152
	mov	x9, x0
	mov	x10, x1
	fmov	d19, d0
Lk_through_block0:
	; v3 = iconst.i64 1
	; v4 = iadd v1, v3
	add	x19, x10, #1
	; v5 = iconst.i64 2
	; v6 = iadd v1, v5
	add	x20, x10, #2
	; v7 = iconst.i64 3
	; v8 = iadd v1, v7
	add	x21, x10, #3
	; v9 = iconst.i64 4
	; v10 = iadd v1, v9
	add	x22, x10, #4
	; v11 = iconst.i64 5
	; v12 = iadd v1, v11
	add	x23, x10, #5
	; v13 = iconst.i64 6
	; v14 = iadd v1, v13
	add	x24, x10, #6
	; v15 = iconst.i64 7
	; v16 = iadd v1, v15
	add	x25, x10, #7
	; v17 = iconst.i64 8
	; v18 = iadd v1, v17
	add	x26, x10, #8
	; v19 = iconst.i64 9
	; v20 = iadd v1, v19
	add	x27, x10, #9
	; v21 = iconst.i64 10
	; v22 = iadd v1, v21
	add	x28, x10, #10
	; v23 = fconst.f64 1.5
	fmov	d20, #1.5
	; v24 = fadd v2, v23
	fadd	d8, d19, d20
	; v25 = fconst.f64 2.5
	fmov	d20, #2.5
	; v26 = fadd v2, v25
	fadd	d9, d19, d20
	; v27 = fconst.f64 3.5
	fmov	d20, #3.5
	; v28 = fadd v2, v27
	fadd	d10, d19, d20
	; v29 = fconst.f64 4.5
	fmov	d20, #4.5
	; v30 = fadd v2, v29
	fadd	d11, d19, d20
	; v31 = fconst.f64 5.5
	fmov	d20, #5.5
	; v32 = fadd v2, v31
	fadd	d12, d19, d20
	; v33 = fconst.f64 6.5
	fmov	d20, #6.5
	; v34 = fadd v2, v33
	fadd	d13, d19, d20
	; v35 = fconst.f64 7.5
	fmov	d20, #7.5
	; v36 = fadd v2, v35
	fadd	d14, d19, d20
	; v37 = fconst.f64 8.5
	fmov	d20, #8.5
	; v38 = fadd v2, v37
	fadd	d15, d19, d20
	; v39 = call_indirect v0(v1) : (i64) -> i64 cc(c)
	mov	x0, x10
	blr	x9
	mov	x9, x0
	; v40 = iadd v4, v39
	add	x9, x19, x9
	; v41 = iadd v6, v40
	add	x9, x20, x9
	; v42 = iadd v8, v41
	add	x9, x21, x9
	; v43 = iadd v10, v42
	add	x9, x22, x9
	; v44 = iadd v12, v43
	add	x9, x23, x9
	; v45 = iadd v14, v44
	add	x9, x24, x9
	; v46 = iadd v16, v45
	add	x9, x25, x9
	; v47 = iadd v18, v46
	add	x9, x26, x9
	; v48 = iadd v20, v47
	add	x9, x27, x9
	; v49 = iadd v22, v48
	add	x9, x28, x9
	; v50 = fadd v24, v26
	fadd	d19, d8, d9
	; v51 = fadd v50, v28
	fadd	d19, d19, d10
	; v52 = fadd v51, v30
	fadd	d19, d19, d11
	; v53 = fadd v52, v32
	fadd	d19, d19, d12
	; v54 = fadd v53, v34
	fadd	d19, d19, d13
	; v55 = fadd v54, v36
	fadd	d19, d19, d14
	; v56 = fadd v55, v38
	fadd	d19, d19, d15
	; v57 = fptosi.i64 v56
	fcvtzs	x10, d19
	; v58 = iadd v49, v57
	add	x9, x9, x10
	; return v58
	mov	x0, x9
	sub	sp, x29, #144
	ldp	d14, d15, [sp], #16
	ldp	d12, d13, [sp], #16
	ldp	d10, d11, [sp], #16
	ldp	d8, d9, [sp], #16
	ldp	x27, x28, [sp], #16
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_through_end:
	.cfi_endproc
	.subsections_via_symbols
