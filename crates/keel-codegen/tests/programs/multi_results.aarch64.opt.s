	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_swap
	.p2align	2
_k_swap:
Lk_swap_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_swap_block0:
	; return v1, v0
	mov	x0, x10
	mov	x1, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_swap_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_narrow
	.p2align	2
_k_narrow:
Lk_narrow_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_narrow_block0:
	; v2 = trunc.i8 v0
	; return v2, v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_narrow_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_padded
	.p2align	2
_k_padded:
Lk_padded_begin:
	; regalloc: v0=x9 v1=x10 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_padded_block0:
	; v1 = trunc.i32 v0
	mov	x10, x9
	; v2 = iconst.i64 1
	; v3 = iadd v0, v2
	add	x9, x9, #1
	; return v3, v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_padded_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_floats
	.p2align	2
_k_floats:
Lk_floats_begin:
	; regalloc: v0=d19 v1=d20 v2=d21 v3=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
Lk_floats_block0:
	; v2 = fadd v0, v1
	fadd	d21, d19, d20
	; v3 = fsub v0, v1
	fsub	d19, d19, d20
	; return v3, v2
	fmov	d0, d19
	fmov	d1, d21
	ldp	x29, x30, [sp], #16
	ret
Lk_floats_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_twice
	.p2align	2
_k_twice:
Lk_twice_begin:
	; regalloc: v0=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_twice_block0:
	; return v0, v0
	mov	x0, x9
	mov	x1, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_twice_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_call
	.p2align	2
_k_call:
Lk_call_begin:
	; regalloc: v0=x19 v1=x9 v2=x9 v3=x10 v4=x20 v5=x21 v6=x9 v7=x10 v8=x9 v9=x9 v10=x11 v12=x9 v13=x9 v14=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	str	x21, [sp, #-16]!
	.cfi_offset x21, -48
	mov	x19, x0
	mov	x9, x1
Lk_call_block0:
	; v2, v3 = call @c_pair(v0, v1)
	mov	x0, x19
	mov	x1, x9
	bl	_c_pair
	mov	x9, x0
	mov	x10, x1
	; v4, v5 = call @c_pair(v3, v2)
	mov	x0, x10
	mov	x1, x9
	bl	_c_pair
	mov	x20, x0
	mov	x21, x1
	; v6, v7 = call @c_pair(v4, v0)
	mov	x0, x20
	mov	x1, x19
	bl	_c_pair
	mov	x9, x0
	mov	x10, x1
	; v8 = iconst.i64 7
	movz	x9, #0x7
	; v9 = imul v4, v8
	mul	x9, x20, x9
	; v10 = iconst.i64 11
	movz	x11, #0xb
	; v11 = imul v5, v10
	; v12 = iadd v9, v11
	madd	x9, x21, x11, x9
	; v13 = iadd v7, v12
	add	x9, x10, x9
	; v14 = iadd v0, v13
	add	x9, x19, x9
	; return v14
	mov	x0, x9
	sub	sp, x29, #32
	ldr	x21, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_call_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_call_sized
	.p2align	2
_k_call_sized:
Lk_call_sized_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9 v5=x11 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_call_sized_block0:
	; v2, v3 = call @c_sized(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_c_sized
	mov	x9, x0
	mov	x10, x1
	; v4 = sext.i64 v2
	sxtw	x9, w9
	; v5 = iconst.i64 100
	movz	x11, #0x64
	; v6 = imul v4, v5
	; v7 = iadd v3, v6
	madd	x9, x9, x11, x10
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_call_sized_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_call_floats
	.p2align	2
_k_call_floats:
Lk_call_floats_begin:
	; regalloc: v0=d19 v1=d20 v2=d19 v3=d20 v4=d21 v5=d19 v6=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
Lk_call_floats_block0:
	; v2, v3 = call @c_floats(v0, v1)
	fmov	d0, d19
	fmov	d1, d20
	bl	_c_floats
	fmov	d19, d0
	fmov	d20, d1
	; v4 = fconst.f64 3.0
	fmov	d21, #3.0
	; v5 = fmul v2, v4
	fmul	d19, d19, d21
	; v6 = fadd v5, v3
	fadd	d19, d19, d20
	; return v6
	fmov	d0, d19
	ldp	x29, x30, [sp], #16
	ret
Lk_call_floats_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_indirect
	.p2align	2
_k_indirect:
Lk_indirect_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10 v5=x11 v6=x9 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_indirect_block0:
	; v3, v4 = call_indirect v0(v1, v2) : (i64, i64) -> (i64, i64) cc(c)
	mov	x0, x10
	mov	x1, x11
	blr	x9
	mov	x9, x0
	mov	x10, x1
	; v5 = iconst.i64 5
	movz	x11, #0x5
	; v6 = imul v3, v5
	mul	x9, x9, x11
	; v7 = isub v6, v4
	sub	x9, x9, x10
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_indirect_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_forward
	.p2align	2
_k_forward:
Lk_forward_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	.cfi_def_cfa_offset 0
	b	_c_pair
Lk_forward_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_forward_swapped
	.p2align	2
_k_forward_swapped:
Lk_forward_swapped_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_forward_swapped_block0:
	; return_call @c_pair(v1, v0)
	mov	x0, x10
	mov	x1, x9
	ldp	x29, x30, [sp], #16
	b	_c_pair
Lk_forward_swapped_end:
	.cfi_endproc
	.section	__TEXT,__text
	.p2align	2
_fast_many:
Lfast_many_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=d19 v8=d20 v9=d21 v10=d22 v11=d23 v12=d24 v13=d25 v14=d26
	.cfi_startproc
	.cfi_def_cfa_offset 0
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
	mov	x14, x5
	mov	x15, x6
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
	fmov	d22, d3
	fmov	d23, d4
	fmov	d24, d5
	fmov	d25, d6
	fmov	d26, d7
Lfast_many_block0:
	; return v6, v5, v4, v3, v2, v1, v0, v14, v13, v12, v11, v10, v9, v8, v7
	mov	x0, x15
	mov	x1, x14
	mov	x2, x13
	mov	x3, x12
	mov	x4, x11
	mov	x5, x10
	mov	x6, x9
	fmov	d0, d26
	fmov	d1, d25
	fmov	d2, d24
	fmov	d3, d23
	fmov	d4, d22
	fmov	d5, d21
	fmov	d6, d20
	fmov	d7, d19
	ret
Lfast_many_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_fast
	.p2align	2
_k_fast:
Lk_fast_begin:
	; regalloc: v0=x9 v1=d19 v4=x19 v6=x20 v7=x21 v8=x22 v9=x10 v10=x11 v11=x12 v12=x13 v13=x14 v14=x15 v15=x9 v16=d20 v17=d8 v18=d9 v19=d10 v20=d11 v21=d12 v22=d13 v23=d14 v24=d20 v25=d21 v26=d22 v27=d23 v28=d24 v29=d25 v30=d26 v31=d19 v32=x9 v33=x10 v34=x11 v35=x12 v36=x13 v37=x14 v38=x15 v39=d19 v40=d20 v41=d21 v42=d22 v43=d23 v44=d24 v45=d25 v46=d26 v53=x9 v54=x9 v55=x9 v56=x9 v57=x9 v58=x9 v59=d20 v60=d21 v61=d22 v62=d23 v63=d24 v64=d25 v65=d26 v66=d19 v67=d19 v68=d19 v69=d19 v70=d19 v71=d19 v72=d19 v73=x10 v74=x11 v76=x9
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
	stp	d8, d9, [sp, #-16]!
	.cfi_offset d8, -64
	.cfi_offset d9, -56
	stp	d10, d11, [sp, #-16]!
	.cfi_offset d10, -80
	.cfi_offset d11, -72
	stp	d12, d13, [sp, #-16]!
	.cfi_offset d12, -96
	.cfi_offset d13, -88
	str	d14, [sp, #-16]!
	.cfi_offset d14, -112
	mov	x9, x0
	fmov	d19, d0
Lk_fast_block0:
	; v2 = iconst.i64 1
	; v3 = iconst.i64 2
	; v4 = iconst.i64 3
	movz	x19, #0x3
	; v5 = iconst.i64 4
	; v6 = iconst.i64 5
	movz	x20, #0x5
	; v7 = iconst.i64 6
	movz	x21, #0x6
	; v8 = iconst.i64 7
	movz	x22, #0x7
	; v9 = iadd v0, v2
	add	x10, x9, #1
	; v10 = iadd v0, v3
	add	x11, x9, #2
	; v11 = iadd v0, v4
	add	x12, x9, #3
	; v12 = iadd v0, v5
	add	x13, x9, #4
	; v13 = iadd v0, v6
	add	x14, x9, #5
	; v14 = iadd v0, v7
	add	x15, x9, #6
	; v15 = iadd v0, v8
	add	x9, x9, #7
	; v16 = fconst.f64 1.0
	fmov	d20, #1.0
	; v17 = fconst.f64 2.0
	fmov	d8, #2.0
	; v18 = fconst.f64 3.0
	fmov	d9, #3.0
	; v19 = fconst.f64 4.0
	fmov	d10, #4.0
	; v20 = fconst.f64 5.0
	fmov	d11, #5.0
	; v21 = fconst.f64 6.0
	fmov	d12, #6.0
	; v22 = fconst.f64 7.0
	fmov	d13, #7.0
	; v23 = fconst.f64 8.0
	fmov	d14, #8.0
	; v24 = fadd v1, v16
	fadd	d20, d19, d20
	; v25 = fadd v1, v17
	fadd	d21, d19, d8
	; v26 = fadd v1, v18
	fadd	d22, d19, d9
	; v27 = fadd v1, v19
	fadd	d23, d19, d10
	; v28 = fadd v1, v20
	fadd	d24, d19, d11
	; v29 = fadd v1, v21
	fadd	d25, d19, d12
	; v30 = fadd v1, v22
	fadd	d26, d19, d13
	; v31 = fadd v1, v23
	fadd	d19, d19, d14
	; v32, v33, v34, v35, v36, v37, v38, v39, v40, v41, v42, v43, v44, v45, v46 = call @fast_many(v9, v10, v11, v12, v13, v14, v15, v24, v25, v26, v27, v28, v29, v30, v31)
	mov	x0, x10
	mov	x1, x11
	mov	x2, x12
	mov	x3, x13
	mov	x4, x14
	mov	x5, x15
	mov	x6, x9
	fmov	d0, d20
	fmov	d1, d21
	fmov	d2, d22
	fmov	d3, d23
	fmov	d4, d24
	fmov	d5, d25
	fmov	d6, d26
	fmov	d7, d19
	bl	_fast_many
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
	mov	x14, x5
	mov	x15, x6
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
	fmov	d22, d3
	fmov	d23, d4
	fmov	d24, d5
	fmov	d25, d6
	fmov	d26, d7
	; v47 = shl v33, v2
	; v48 = imul v34, v4
	; v49 = shl v35, v3
	; v50 = imul v36, v6
	; v51 = imul v37, v7
	; v52 = imul v38, v8
	; v53 = iadd v32, v47
	add	x9, x9, x10, lsl #1
	; v54 = iadd v48, v53
	madd	x9, x11, x19, x9
	; v55 = iadd v49, v54
	add	x9, x9, x12, lsl #2
	; v56 = iadd v50, v55
	madd	x9, x13, x20, x9
	; v57 = iadd v51, v56
	madd	x9, x14, x21, x9
	; v58 = iadd v52, v57
	madd	x9, x15, x22, x9
	; v59 = fmul v40, v17
	fmul	d20, d20, d8
	; v60 = fmul v41, v18
	fmul	d21, d21, d9
	; v61 = fmul v42, v19
	fmul	d22, d22, d10
	; v62 = fmul v43, v20
	fmul	d23, d23, d11
	; v63 = fmul v44, v21
	fmul	d24, d24, d12
	; v64 = fmul v45, v22
	fmul	d25, d25, d13
	; v65 = fmul v46, v23
	fmul	d26, d26, d14
	; v66 = fadd v39, v59
	fadd	d19, d19, d20
	; v67 = fadd v66, v60
	fadd	d19, d19, d21
	; v68 = fadd v67, v61
	fadd	d19, d19, d22
	; v69 = fadd v68, v62
	fadd	d19, d19, d23
	; v70 = fadd v69, v63
	fadd	d19, d19, d24
	; v71 = fadd v70, v64
	fadd	d19, d19, d25
	; v72 = fadd v71, v65
	fadd	d19, d19, d26
	; v73 = fptosi.i64 v72
	fcvtzs	x10, d19
	; v74 = iconst.i64 1000000
	movz	x11, #0x4240
	movk	x11, #0xf, lsl #16
	; v75 = imul v58, v74
	; v76 = iadd v73, v75
	madd	x9, x9, x11, x10
	; return v76
	mov	x0, x9
	sub	sp, x29, #96
	ldr	d14, [sp], #16
	ldp	d12, d13, [sp], #16
	ldp	d10, d11, [sp], #16
	ldp	d8, d9, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_fast_end:
	.cfi_endproc
	.subsections_via_symbols
