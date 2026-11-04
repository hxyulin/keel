	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_vla
	.p2align	2
_k_vla:
Lk_vla_begin:
	; regalloc: v0=x20 v1=x10 v2=x23 v4=x21 v5=x22 v6=x9 v7=x9 v8=x21 v12=x10 v13=x10 v15=x9 v16=x9 v18=x22 v19=x9 v20=x10 v22=x9 v23=x10 v24=x9 v25=x9 v31=x10 v37=x9 v38=x9 v39=x10 v40=x11 v41=x10 v42=x9 v45=x9 v47=d19 v48=d19 v49=d20 v50=d21 v51=d22 v52=x11 v53=d23 v54=d19 v57=d23 v58=d20 v61=d23 v62=d21 v65=d23 v66=d22 v68=x12 v70=x11 v73=d19 v74=d19 v75=d19 v76=x10 v88=x11 v89=x10 v91=x12 v92=x13
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
	sub	sp, sp, #80
	mov	x19, sp
	mov	x20, x0
Lk_vla_block0:
	; v26 = iconst.i64 3
	; v4 = shl v0, v26
	lsl	x21, x20, #3
	; v5 = intrinsic stack_alloc(v4)
	add	x16, x21, #15
	and	x16, x16, #-16
	sub	sp, sp, x16
	add	x22, sp, #16
	; call @c_fill(v5, v0)
	mov	x0, x22
	mov	x1, x20
	bl	_c_fill
	; v6 = iconst.i64 0
	movz	x9, #0x0
	; v33 = iconst.i64 8
	; v34 = icmp.uge v0, v33
	; brif v34, block7, block1(v6, v6)
	cmp	x20, #8
	b.hs	Lk_vla_block7
	mov	x21, x9
	b	Lk_vla_block1
Lk_vla_block7:
	; v79 = iconst.i64 2305843009213693951
	; v37 = and v0, v79
	and	x9, x20, #0x1fffffffffffffff
	; v38 = icmp.eq v0, v37
	cmp	x20, x9
	cset	w9, eq
	; v39 = ptrtoint.i64 v5
	mov	x10, x22
	; v40 = iadd v4, v39
	add	x11, x21, x10
	; v41 = icmp.ule v39, v40
	cmp	x10, x11
	cset	w10, ls
	; v42 = and v38, v41
	and	w9, w9, w10
	; v89 = iconst.i64 0
	movz	x10, #0x0
	; brif v42, block5, block1(v89, v89)
	tst	w9, #255
	b.ne	Lk_vla_block5
	mov	x9, x10
	mov	x21, x10
	b	Lk_vla_block1
Lk_vla_block5:
	; v78 = iconst.i64 -8
	; v45 = and v0, v78
	and	x9, x20, #0xfffffffffffffff8
	; v90 = iconst.i64 0
	; v47 = vsplat.i64x2 v90
	movi	v19.2d, #0
	; jump block6(v45, v47, v47, v47, v47, v5)
	mov	x10, x9
	mov	v20.16b, v19.16b
	mov	v21.16b, v19.16b
	mov	v22.16b, v19.16b
	mov	x11, x22
Lk_vla_block6:
	; v53 = load.i64x2 v52
	ldr	q23, [x11]
	; v54 = viadd v48, v53
	add	v19.2d, v19.2d, v23.2d
	; v55 = iconst.i64 16
	; v56 = ptradd v52, v55
	; v57 = load.i64x2 v56
	ldr	q23, [x11, #16]
	; v58 = viadd v49, v57
	add	v20.2d, v20.2d, v23.2d
	; v59 = iconst.i64 32
	; v60 = ptradd v52, v59
	; v61 = load.i64x2 v60
	ldr	q23, [x11, #32]
	; v62 = viadd v50, v61
	add	v21.2d, v21.2d, v23.2d
	; v63 = iconst.i64 48
	; v64 = ptradd v52, v63
	; v65 = load.i64x2 v64
	ldr	q23, [x11, #48]
	; v66 = viadd v51, v65
	add	v22.2d, v22.2d, v23.2d
	; v87 = iconst.i64 8
	; v68 = isub v31, v87
	sub	x12, x10, #8
	; v69 = iconst.i64 64
	; v70 = ptradd v52, v69
	add	x11, x11, #64
	; v72 = icmp.ne v31, v87
	; brif v72, block6(v68, v54, v58, v62, v66, v70), block8
	cmp	x10, #8
	b.eq	Lk_vla_block8
	mov	x10, x12
	b	Lk_vla_block6
Lk_vla_block8:
	; v73 = viadd v54, v58
	add	v19.2d, v19.2d, v20.2d
	; v74 = viadd v73, v62
	add	v19.2d, v19.2d, v21.2d
	; v75 = viadd v74, v66
	add	v19.2d, v19.2d, v22.2d
	; v76 = vreduce_add v75
	addp	d18, v19.2d
	umov	x10, v18.d[0]
	; jump block1(v45, v76)
	mov	x21, x10
Lk_vla_block1:
	; v9 = icmp.ugt v0, v7
	; brif v9, block2, block3
	cmp	x20, x9
	b.hi	Lk_vla_block2
Lk_vla_block3:
	; v16 = iconst.i64 2
	movz	x9, #0x2
	; v1 = iconst.i64 1000
	movz	x10, #0x3e8
	; v2 = imul v0, v1
	mul	x23, x20, x10
	; v88 = iconst.i64 8
	movz	x11, #0x8
	; v91 = iconst.i64 0
	movz	x12, #0x0
	; v92 = iconst.i64 3
	movz	x13, #0x3
	; v18 = call @c_many(v5, v0, v1, v88, v91, v16, v92, v2, v0, v1)
	str	x20, [sp, #0]
	str	x10, [sp, #8]
	mov	x0, x22
	mov	x1, x20
	mov	x2, x10
	mov	x3, x11
	mov	x4, x12
	mov	x5, x9
	mov	x6, x13
	mov	x7, x23
	bl	_c_many
	mov	x22, x0
	; v19 = stack_addr ss0
	add	x9, x19, #56
	; store v0, v19
	str	x20, [x19, #56]
	; v20 = ptradd v19, v88
	add	x10, x9, #8
	; store v2, v20
	str	x23, [x10]
	; v21 = ptradd v20, v88
	; store v8, v21
	str	x21, [x10, #8]
	; v22 = call @c_big(v19)
	ldr	x17, [x9, #0]
	str	x17, [x19, #16]
	ldr	x17, [x9, #8]
	str	x17, [x19, #24]
	ldr	x17, [x9, #16]
	str	x17, [x19, #32]
	add	x0, x19, #16
	bl	_c_big
	mov	x9, x0
	; v23 = iadd v8, v18
	add	x10, x21, x22
	; v24 = iadd v22, v23
	add	x9, x9, x10
	; v25 = iadd v2, v24
	add	x9, x23, x9
	; return v25
	mov	x0, x9
	sub	sp, x29, #48
	ldr	x23, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_vla_block2:
	; v93 = iconst.i64 3
	; v10 = shl v7, v93
	; v11 = ptradd v5, v10
	; v12 = load.i64 v11
	ldr	x10, [x22, x9, lsl #3]
	; v13 = iadd v8, v12
	add	x10, x21, x10
	; v14 = iconst.i64 1
	; v15 = iadd v7, v14
	add	x9, x9, #1
	; jump block1(v15, v13)
	mov	x21, x10
	b	Lk_vla_block1
Lk_vla_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_loop
	.p2align	2
_k_loop:
Lk_loop_begin:
	; regalloc: v0=x9 v1=x10 v5=x12 v6=x13 v7=x13 v8=x13 v9=x11 v11=x10 v12=x10 v13=x11 v15=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x19, sp
	mov	x9, x0
Lk_loop_block0:
	; v1 = iconst.i64 0
	movz	x10, #0x0
	; v4 = icmp.ugt v0, v1
	; brif v4, block2(v1, v1), block3(v1)
	cmp	x9, #0
	b.ls	Lk_loop_inst3_else
	mov	x11, x10
	b	Lk_loop_block2
Lk_loop_inst3_else:
	mov	x9, x10
	b	Lk_loop_block3
Lk_loop_block2:
	; v5 = intrinsic stack_save()
	mov	x12, sp
	; v6 = iconst.i64 65536
	movz	x13, #0x1, lsl #16
	; v7 = intrinsic stack_alloc(v6)
	add	x16, x13, #15
	and	x16, x16, #-16
	sub	sp, sp, x16
	add	x13, sp, #0
	; store v12, v7
	str	x10, [x13]
	; v8 = load.i64 v7
	ldr	x13, [x13]
	; v9 = iadd v13, v8
	add	x11, x11, x13
	; intrinsic stack_restore(v5)
	mov	sp, x12
	; v10 = iconst.i64 1
	; v11 = iadd v12, v10
	add	x10, x10, #1
	; v16 = icmp.ugt v0, v11
	; brif v16, block2(v11, v9), block3(v9)
	cmp	x9, x10
	b.hi	Lk_loop_block2
	mov	x9, x11
Lk_loop_block3:
	; return v15
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_loop_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_outer
	.p2align	2
_k_outer:
Lk_outer_begin:
	; regalloc: v0=x19 v1=x20 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	mov	x19, x0
Lk_outer_block0:
	; v1 = iconst.i64 7
	movz	x20, #0x7
	; v2 = imul v0, v1
	; v3 = call @k_vla(v0)
	mov	x0, x19
	bl	_k_vla
	mov	x9, x0
	; v4 = iadd v2, v3
	madd	x9, x19, x20, x9
	; return v4
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_outer_end:
	.cfi_endproc
	.subsections_via_symbols
