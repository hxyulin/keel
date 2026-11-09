	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_rotate3
	.p2align	2
_rotate3:
Lrotate3_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x9 v5=x10 v6=x11 v7=x12 v9=x13 v11=x12 v12=x12 v13=x9 v14=x12 v16=x9 v17=x9
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
Lrotate3_block0:
	; jump block1(v0, v1, v2, v3)
Lrotate3_block1:
	; v8 = iconst.i64 0
	; v9 = icmp.eq v7, v8
	cmp	x12, #0
	cset	w13, eq
	; v10 = iconst.i64 1
	; v11 = isub v7, v10
	sub	x12, x12, #1
	; brif v9, block2, block1(v5, v6, v4, v11)
	tst	w13, #255
	b.ne	Lrotate3_block2
	mov	x16, x9
	mov	x9, x10
	mov	x10, x11
	mov	x11, x16
	b	Lrotate3_block1
Lrotate3_block2:
	; v12 = iconst.i64 100
	movz	x12, #0x64
	; v13 = imul v4, v12
	mul	x9, x9, x12
	; v14 = iconst.i64 10
	movz	x12, #0xa
	; v15 = imul v5, v14
	; v16 = iadd v13, v15
	madd	x9, x10, x12, x9
	; v17 = iadd v6, v16
	add	x9, x11, x9
	; return v17
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lrotate3_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_rotate_many
	.p2align	2
_rotate_many:
Lrotate_many_begin:
	; regalloc: v0=x9 v1=[sp, #0] v5=[sp, #8] v7=[sp, #16] v9=[sp, #24] v11=x14 v13=x15 v15=x19 v17=x20 v19=x21 v21=x22 v23=x23 v25=x24 v27=x25 v29=x26 v31=x27 v33=x28 v35=x10 v37=x11 v39=x12 v41=x13 v42=x9 v43=x10 v44=x11 v45=x12 v46=x14 v47=x15 v48=x19 v49=x20 v50=x21 v51=x22 v52=x23 v53=x24 v54=x25 v55=x26 v56=x27 v57=[sp, #0] v58=[sp, #8] v59=[sp, #16] v60=[sp, #24] v61=[sp, #32] v62=x13 v64=x28 v66=x13 v67=x13 v72=x9 v74=x9 v76=x9 v78=x9 v80=x9 v82=x9 v84=x9 v86=x9 v88=x9 v90=x9 v92=x9 v94=x9 v96=x9 v98=x9 v100=x9 v102=x9 v104=x9 v106=x9 v108=x9
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
	sub	sp, sp, #48
	mov	x9, x0
	str	x1, [sp, #0]
Lrotate_many_block0:
	; v4 = iconst.i64 1
	; v5 = iadd v0, v4
	add	x16, x9, #1
	str	x16, [sp, #8]
	; v6 = iconst.i64 2
	; v7 = iadd v0, v6
	add	x16, x9, #2
	str	x16, [sp, #16]
	; v8 = iconst.i64 3
	; v9 = iadd v0, v8
	add	x16, x9, #3
	str	x16, [sp, #24]
	; v10 = iconst.i64 4
	; v11 = iadd v0, v10
	add	x14, x9, #4
	; v12 = iconst.i64 5
	; v13 = iadd v0, v12
	add	x15, x9, #5
	; v14 = iconst.i64 6
	; v15 = iadd v0, v14
	add	x19, x9, #6
	; v16 = iconst.i64 7
	; v17 = iadd v0, v16
	add	x20, x9, #7
	; v18 = iconst.i64 8
	; v19 = iadd v0, v18
	add	x21, x9, #8
	; v20 = iconst.i64 9
	; v21 = iadd v0, v20
	add	x22, x9, #9
	; v22 = iconst.i64 10
	; v23 = iadd v0, v22
	add	x23, x9, #10
	; v24 = iconst.i64 11
	; v25 = iadd v0, v24
	add	x24, x9, #11
	; v26 = iconst.i64 12
	; v27 = iadd v0, v26
	add	x25, x9, #12
	; v28 = iconst.i64 13
	; v29 = iadd v0, v28
	add	x26, x9, #13
	; v30 = iconst.i64 14
	; v31 = iadd v0, v30
	add	x27, x9, #14
	; v32 = iconst.i64 15
	; v33 = iadd v0, v32
	add	x28, x9, #15
	; v34 = iconst.i64 16
	; v35 = iadd v0, v34
	add	x10, x9, #16
	; v36 = iconst.i64 17
	; v37 = iadd v0, v36
	add	x11, x9, #17
	; v38 = iconst.i64 18
	; v39 = iadd v0, v38
	add	x12, x9, #18
	; v40 = iconst.i64 19
	; v41 = iadd v0, v40
	add	x13, x9, #19
	; jump block1(v0, v5, v7, v9, v11, v13, v15, v17, v19, v21, v23, v25, v27, v29, v31, v33, v35, v37, v39, v41, v1)
	str	x13, [sp, #32]
	ldr	x13, [sp, #0]
	str	x28, [sp, #0]
	mov	x16, x10
	ldr	x10, [sp, #8]
	str	x16, [sp, #8]
	mov	x16, x11
	ldr	x11, [sp, #16]
	str	x16, [sp, #16]
	mov	x16, x12
	ldr	x12, [sp, #24]
	str	x16, [sp, #24]
Lrotate_many_block1:
	; v2 = iconst.i64 0
	; v64 = icmp.eq v62, v2
	cmp	x13, #0
	cset	w28, eq
	; v111 = iconst.i64 1
	; v66 = isub v62, v111
	sub	x13, x13, #1
	; brif v64, block2, block1(v43, v44, v45, v46, v47, v48, v49, v50, v51, v52, v53, v54, v55, v56, v57, v58, v59, v60, v61, v42, v66)
	tst	w28, #255
	b.ne	Lrotate_many_block2
	mov	x16, x9
	mov	x9, x10
	mov	x10, x11
	mov	x11, x12
	mov	x12, x14
	mov	x14, x15
	mov	x15, x19
	mov	x19, x20
	mov	x20, x21
	mov	x21, x22
	mov	x22, x23
	mov	x23, x24
	mov	x24, x25
	mov	x25, x26
	mov	x26, x27
	ldr	x27, [sp, #0]
	ldr	x17, [sp, #8]
	str	x17, [sp, #0]
	ldr	x17, [sp, #16]
	str	x17, [sp, #8]
	ldr	x17, [sp, #24]
	str	x17, [sp, #16]
	ldr	x17, [sp, #32]
	str	x17, [sp, #24]
	str	x16, [sp, #32]
	b	Lrotate_many_block1
Lrotate_many_block2:
	; v67 = iconst.i64 31
	movz	x13, #0x1f
	; v71 = imul v42, v67
	; v72 = iadd v43, v71
	madd	x9, x9, x13, x10
	; v73 = imul v72, v67
	; v74 = iadd v44, v73
	madd	x9, x9, x13, x11
	; v75 = imul v74, v67
	; v76 = iadd v45, v75
	madd	x9, x9, x13, x12
	; v77 = imul v76, v67
	; v78 = iadd v46, v77
	madd	x9, x9, x13, x14
	; v79 = imul v78, v67
	; v80 = iadd v47, v79
	madd	x9, x9, x13, x15
	; v81 = imul v80, v67
	; v82 = iadd v48, v81
	madd	x9, x9, x13, x19
	; v83 = imul v82, v67
	; v84 = iadd v49, v83
	madd	x9, x9, x13, x20
	; v85 = imul v84, v67
	; v86 = iadd v50, v85
	madd	x9, x9, x13, x21
	; v87 = imul v86, v67
	; v88 = iadd v51, v87
	madd	x9, x9, x13, x22
	; v89 = imul v88, v67
	; v90 = iadd v52, v89
	madd	x9, x9, x13, x23
	; v91 = imul v90, v67
	; v92 = iadd v53, v91
	madd	x9, x9, x13, x24
	; v93 = imul v92, v67
	; v94 = iadd v54, v93
	madd	x9, x9, x13, x25
	; v95 = imul v94, v67
	; v96 = iadd v55, v95
	madd	x9, x9, x13, x26
	; v97 = imul v96, v67
	; v98 = iadd v56, v97
	madd	x9, x9, x13, x27
	; v99 = imul v98, v67
	; v100 = iadd v57, v99
	ldr	x16, [sp, #0]
	madd	x9, x9, x13, x16
	; v101 = imul v100, v67
	; v102 = iadd v58, v101
	ldr	x16, [sp, #8]
	madd	x9, x9, x13, x16
	; v103 = imul v102, v67
	; v104 = iadd v59, v103
	ldr	x16, [sp, #16]
	madd	x9, x9, x13, x16
	; v105 = imul v104, v67
	; v106 = iadd v60, v105
	ldr	x16, [sp, #24]
	madd	x9, x9, x13, x16
	; v107 = imul v106, v67
	; v108 = iadd v61, v107
	ldr	x16, [sp, #32]
	madd	x9, x9, x13, x16
	; return v108
	mov	x0, x9
	sub	sp, x29, #80
	ldp	x27, x28, [sp], #16
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lrotate_many_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_invariant
	.p2align	2
_invariant:
Linvariant_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x11 v5=x9 v6=x12 v7=x10 v8=x12 v11=x12 v12=x12 v14=x10 v15=x13 v17=x14 v18=x13 v19=x14 v20=x14 v22=x12
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Linvariant_block0:
	; v2 = iconst.i64 3
	movz	x11, #0x3
	; v3 = imul v0, v2
	mul	x11, x9, x11
	; v4 = iconst.i64 7
	; v5 = iadd v0, v4
	add	x9, x9, #7
	; v6 = iconst.i64 0
	movz	x12, #0x0
	; jump block1(v1, v6)
Linvariant_block1:
	; v25 = iconst.i64 0
	; v10 = icmp.eq v7, v25
	; brif v10, block3, block2
	cmp	x10, #0
	b.eq	Linvariant_block3
Linvariant_block2:
	; v11 = iadd v3, v8
	add	x12, x11, x12
	; v12 = xor v5, v11
	eor	x12, x9, x12
	; v13 = iconst.i64 1
	; v14 = isub v7, v13
	sub	x10, x10, #1
	; v15 = iconst.i64 5
	movz	x13, #0x5
	; v16 = imul v14, v15
	; v17 = iconst.i64 11
	movz	x14, #0xb
	; v18 = iadd v16, v17
	madd	x13, x10, x13, x14
	; v19 = iconst.i64 13
	movz	x14, #0xd
	; v20 = xor v18, v19
	eor	x14, x13, x14
	; v21 = imul v18, v20
	; v22 = iadd v12, v21
	madd	x12, x13, x14, x12
	; jump block1(v14, v22)
	b	Linvariant_block1
Linvariant_block3:
	; return v8
	mov	x0, x12
	ldp	x29, x30, [sp], #16
	ret
Linvariant_end:
	.cfi_endproc
	.subsections_via_symbols
