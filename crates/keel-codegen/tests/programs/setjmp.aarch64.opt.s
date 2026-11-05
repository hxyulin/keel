	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_setjmp
	.p2align	2
_k_setjmp:
Lk_setjmp_begin:
	; regalloc: v0=x19 v1=x10 v2=x10 v3=x9 v5=x10 v7=x10 v9=x10 v11=x10 v13=x10 v15=x10 v17=x10 v19=x10 v21=x10 v23=x10 v25=x10 v27=x10 v29=x9 v30=x9 v33=x9 v34=[sp, #0] v35=[sp, #8] v37=[sp, #16] v39=x12 v41=x13 v43=x14 v45=x15 v47=x20 v49=x21 v51=x22 v53=x23 v55=x24 v57=x25 v59=x26 v61=x27 v63=x28 v65=x9 v67=x10 v69=x11 v70=x11 v71=x11 v72=x11 v73=x11 v74=x11 v75=x11 v76=x11 v77=x11 v78=x11 v79=x11 v80=x11 v81=x11 v82=x11 v83=x11 v84=x9 v85=x9 v86=x10 v87=x9 v88=x9 v89=x9 v90=x9 v91=x9 v92=x9 v93=x9 v94=x9 v95=x9 v96=x9 v97=x9 v98=x9 v99=x9 v100=x9 v103=x10
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
	sub	sp, sp, #224
	mov	x19, x0
Lk_setjmp_block0:
	; v29 = stack_addr ss0
	add	x9, sp, #32
	; v30 = call @setjmp(v29)
	mov	x0, x9
	bl	_setjmp
	mov	x9, x0
	; v31 = iconst.i32 0
	; v32 = icmp.ne v30, v31
	; brif v32, block2, block1
	cmp	w9, #0
	b.ne	Lk_setjmp_block2
Lk_setjmp_block1:
	; v33 = iconst.i64 101
	movz	x9, #0x65
	; v34 = imul v0, v33
	mul	x16, x19, x9
	str	x16, [sp, #0]
	; v35 = iconst.i64 103
	movz	x16, #0x67
	str	x16, [sp, #8]
	; v36 = imul v0, v35
	; v37 = iconst.i64 105
	movz	x16, #0x69
	str	x16, [sp, #16]
	; v38 = imul v0, v37
	; v39 = iconst.i64 107
	movz	x12, #0x6b
	; v40 = imul v0, v39
	; v41 = iconst.i64 109
	movz	x13, #0x6d
	; v42 = imul v0, v41
	; v43 = iconst.i64 111
	movz	x14, #0x6f
	; v44 = imul v0, v43
	; v45 = iconst.i64 113
	movz	x15, #0x71
	; v46 = imul v0, v45
	; v47 = iconst.i64 115
	movz	x20, #0x73
	; v48 = imul v0, v47
	; v49 = iconst.i64 117
	movz	x21, #0x75
	; v50 = imul v0, v49
	; v51 = iconst.i64 119
	movz	x22, #0x77
	; v52 = imul v0, v51
	; v53 = iconst.i64 121
	movz	x23, #0x79
	; v54 = imul v0, v53
	; v55 = iconst.i64 123
	movz	x24, #0x7b
	; v56 = imul v0, v55
	; v57 = iconst.i64 125
	movz	x25, #0x7d
	; v58 = imul v0, v57
	; v59 = iconst.i64 127
	movz	x26, #0x7f
	; v60 = imul v0, v59
	; v61 = iconst.i64 129
	movz	x27, #0x81
	; v62 = imul v0, v61
	; v63 = iconst.i64 131
	movz	x28, #0x83
	; v64 = imul v0, v63
	; v65 = iconst.i64 133
	movz	x9, #0x85
	; v66 = imul v0, v65
	; v67 = iconst.i64 135
	movz	x10, #0x87
	; v68 = imul v0, v67
	; v69 = iadd v34, v36
	ldr	x16, [sp, #0]
	ldr	x8, [sp, #8]
	madd	x11, x19, x8, x16
	; v70 = iadd v38, v69
	ldr	x8, [sp, #16]
	madd	x11, x19, x8, x11
	; v71 = iadd v40, v70
	madd	x11, x19, x12, x11
	; v72 = iadd v42, v71
	madd	x11, x19, x13, x11
	; v73 = iadd v44, v72
	madd	x11, x19, x14, x11
	; v74 = iadd v46, v73
	madd	x11, x19, x15, x11
	; v75 = iadd v48, v74
	madd	x11, x19, x20, x11
	; v76 = iadd v50, v75
	madd	x11, x19, x21, x11
	; v77 = iadd v52, v76
	madd	x11, x19, x22, x11
	; v78 = iadd v54, v77
	madd	x11, x19, x23, x11
	; v79 = iadd v56, v78
	madd	x11, x19, x24, x11
	; v80 = iadd v58, v79
	madd	x11, x19, x25, x11
	; v81 = iadd v60, v80
	madd	x11, x19, x26, x11
	; v82 = iadd v62, v81
	madd	x11, x19, x27, x11
	; v83 = iadd v64, v82
	madd	x11, x19, x28, x11
	; v84 = iadd v66, v83
	madd	x9, x19, x9, x11
	; v85 = iadd v68, v84
	madd	x9, x19, x10, x9
	; v86 = symbol_addr @sink
	adrp	x10, _sink@PAGE
	add	x10, x10, _sink@PAGEOFF
	; store v85, v86
	str	x9, [x10]
	; v87 = iconst.i32 1
	movz	w9, #0x1
	; v103 = stack_addr ss0
	add	x10, sp, #32
	; call @longjmp(v103, v87)
	mov	x0, x10
	mov	x1, x9
	bl	_longjmp
	; trap
	brk	#1
Lk_setjmp_block2:
	; v3 = iconst.i64 5
	movz	x9, #0x5
	; v4 = imul v0, v3
	; v1 = iconst.i64 3
	movz	x10, #0x3
	; v2 = imul v0, v1
	mul	x10, x19, x10
	; v88 = iadd v2, v4
	madd	x9, x19, x9, x10
	; v5 = iconst.i64 7
	movz	x10, #0x7
	; v6 = imul v0, v5
	; v89 = iadd v6, v88
	madd	x9, x19, x10, x9
	; v7 = iconst.i64 11
	movz	x10, #0xb
	; v8 = imul v0, v7
	; v90 = iadd v8, v89
	madd	x9, x19, x10, x9
	; v9 = iconst.i64 13
	movz	x10, #0xd
	; v10 = imul v0, v9
	; v91 = iadd v10, v90
	madd	x9, x19, x10, x9
	; v11 = iconst.i64 17
	movz	x10, #0x11
	; v12 = imul v0, v11
	; v92 = iadd v12, v91
	madd	x9, x19, x10, x9
	; v13 = iconst.i64 19
	movz	x10, #0x13
	; v14 = imul v0, v13
	; v93 = iadd v14, v92
	madd	x9, x19, x10, x9
	; v15 = iconst.i64 23
	movz	x10, #0x17
	; v16 = imul v0, v15
	; v94 = iadd v16, v93
	madd	x9, x19, x10, x9
	; v17 = iconst.i64 29
	movz	x10, #0x1d
	; v18 = imul v0, v17
	; v95 = iadd v18, v94
	madd	x9, x19, x10, x9
	; v19 = iconst.i64 31
	movz	x10, #0x1f
	; v20 = imul v0, v19
	; v96 = iadd v20, v95
	madd	x9, x19, x10, x9
	; v21 = iconst.i64 37
	movz	x10, #0x25
	; v22 = imul v0, v21
	; v97 = iadd v22, v96
	madd	x9, x19, x10, x9
	; v23 = iconst.i64 41
	movz	x10, #0x29
	; v24 = imul v0, v23
	; v98 = iadd v24, v97
	madd	x9, x19, x10, x9
	; v25 = iconst.i64 43
	movz	x10, #0x2b
	; v26 = imul v0, v25
	; v99 = iadd v26, v98
	madd	x9, x19, x10, x9
	; v27 = iconst.i64 47
	movz	x10, #0x2f
	; v28 = imul v0, v27
	; v100 = iadd v28, v99
	madd	x9, x19, x10, x9
	; return v100
	mov	x0, x9
	sub	sp, x29, #80
	ldp	x27, x28, [sp], #16
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_setjmp_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_setjmp_swapped
	.p2align	2
_k_setjmp_swapped:
Lk_setjmp_swapped_begin:
	; regalloc: v0=x19 v1=x10 v2=x10 v3=x9 v5=x10 v7=x10 v9=x10 v11=x10 v13=x10 v15=x10 v17=x10 v19=x10 v21=x10 v23=x10 v25=x10 v27=x10 v29=x9 v30=x9 v33=x9 v34=[sp, #0] v35=[sp, #8] v37=[sp, #16] v39=x12 v41=x13 v43=x14 v45=x15 v47=x20 v49=x21 v51=x22 v53=x23 v55=x24 v57=x25 v59=x26 v61=x27 v63=x28 v65=x9 v67=x10 v69=x11 v70=x11 v71=x11 v72=x11 v73=x11 v74=x11 v75=x11 v76=x11 v77=x11 v78=x11 v79=x11 v80=x11 v81=x11 v82=x11 v83=x11 v84=x9 v85=x9 v86=x10 v87=x9 v88=x9 v89=x9 v90=x9 v91=x9 v92=x9 v93=x9 v94=x9 v95=x9 v96=x9 v97=x9 v98=x9 v99=x9 v100=x9 v103=x10
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
	sub	sp, sp, #224
	mov	x19, x0
Lk_setjmp_swapped_block0:
	; v29 = stack_addr ss0
	add	x9, sp, #32
	; v30 = call @setjmp(v29)
	mov	x0, x9
	bl	_setjmp
	mov	x9, x0
	; v31 = iconst.i32 0
	; v32 = icmp.eq v30, v31
	; brif v32, block1, block2
	cmp	w9, #0
	b.eq	Lk_setjmp_swapped_block1
Lk_setjmp_swapped_block2:
	; v3 = iconst.i64 5
	movz	x9, #0x5
	; v4 = imul v0, v3
	; v1 = iconst.i64 3
	movz	x10, #0x3
	; v2 = imul v0, v1
	mul	x10, x19, x10
	; v88 = iadd v2, v4
	madd	x9, x19, x9, x10
	; v5 = iconst.i64 7
	movz	x10, #0x7
	; v6 = imul v0, v5
	; v89 = iadd v6, v88
	madd	x9, x19, x10, x9
	; v7 = iconst.i64 11
	movz	x10, #0xb
	; v8 = imul v0, v7
	; v90 = iadd v8, v89
	madd	x9, x19, x10, x9
	; v9 = iconst.i64 13
	movz	x10, #0xd
	; v10 = imul v0, v9
	; v91 = iadd v10, v90
	madd	x9, x19, x10, x9
	; v11 = iconst.i64 17
	movz	x10, #0x11
	; v12 = imul v0, v11
	; v92 = iadd v12, v91
	madd	x9, x19, x10, x9
	; v13 = iconst.i64 19
	movz	x10, #0x13
	; v14 = imul v0, v13
	; v93 = iadd v14, v92
	madd	x9, x19, x10, x9
	; v15 = iconst.i64 23
	movz	x10, #0x17
	; v16 = imul v0, v15
	; v94 = iadd v16, v93
	madd	x9, x19, x10, x9
	; v17 = iconst.i64 29
	movz	x10, #0x1d
	; v18 = imul v0, v17
	; v95 = iadd v18, v94
	madd	x9, x19, x10, x9
	; v19 = iconst.i64 31
	movz	x10, #0x1f
	; v20 = imul v0, v19
	; v96 = iadd v20, v95
	madd	x9, x19, x10, x9
	; v21 = iconst.i64 37
	movz	x10, #0x25
	; v22 = imul v0, v21
	; v97 = iadd v22, v96
	madd	x9, x19, x10, x9
	; v23 = iconst.i64 41
	movz	x10, #0x29
	; v24 = imul v0, v23
	; v98 = iadd v24, v97
	madd	x9, x19, x10, x9
	; v25 = iconst.i64 43
	movz	x10, #0x2b
	; v26 = imul v0, v25
	; v99 = iadd v26, v98
	madd	x9, x19, x10, x9
	; v27 = iconst.i64 47
	movz	x10, #0x2f
	; v28 = imul v0, v27
	; v100 = iadd v28, v99
	madd	x9, x19, x10, x9
	; return v100
	mov	x0, x9
	sub	sp, x29, #80
	ldp	x27, x28, [sp], #16
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_setjmp_swapped_block1:
	; v33 = iconst.i64 101
	movz	x9, #0x65
	; v34 = imul v0, v33
	mul	x16, x19, x9
	str	x16, [sp, #0]
	; v35 = iconst.i64 103
	movz	x16, #0x67
	str	x16, [sp, #8]
	; v36 = imul v0, v35
	; v37 = iconst.i64 105
	movz	x16, #0x69
	str	x16, [sp, #16]
	; v38 = imul v0, v37
	; v39 = iconst.i64 107
	movz	x12, #0x6b
	; v40 = imul v0, v39
	; v41 = iconst.i64 109
	movz	x13, #0x6d
	; v42 = imul v0, v41
	; v43 = iconst.i64 111
	movz	x14, #0x6f
	; v44 = imul v0, v43
	; v45 = iconst.i64 113
	movz	x15, #0x71
	; v46 = imul v0, v45
	; v47 = iconst.i64 115
	movz	x20, #0x73
	; v48 = imul v0, v47
	; v49 = iconst.i64 117
	movz	x21, #0x75
	; v50 = imul v0, v49
	; v51 = iconst.i64 119
	movz	x22, #0x77
	; v52 = imul v0, v51
	; v53 = iconst.i64 121
	movz	x23, #0x79
	; v54 = imul v0, v53
	; v55 = iconst.i64 123
	movz	x24, #0x7b
	; v56 = imul v0, v55
	; v57 = iconst.i64 125
	movz	x25, #0x7d
	; v58 = imul v0, v57
	; v59 = iconst.i64 127
	movz	x26, #0x7f
	; v60 = imul v0, v59
	; v61 = iconst.i64 129
	movz	x27, #0x81
	; v62 = imul v0, v61
	; v63 = iconst.i64 131
	movz	x28, #0x83
	; v64 = imul v0, v63
	; v65 = iconst.i64 133
	movz	x9, #0x85
	; v66 = imul v0, v65
	; v67 = iconst.i64 135
	movz	x10, #0x87
	; v68 = imul v0, v67
	; v69 = iadd v34, v36
	ldr	x16, [sp, #0]
	ldr	x8, [sp, #8]
	madd	x11, x19, x8, x16
	; v70 = iadd v38, v69
	ldr	x8, [sp, #16]
	madd	x11, x19, x8, x11
	; v71 = iadd v40, v70
	madd	x11, x19, x12, x11
	; v72 = iadd v42, v71
	madd	x11, x19, x13, x11
	; v73 = iadd v44, v72
	madd	x11, x19, x14, x11
	; v74 = iadd v46, v73
	madd	x11, x19, x15, x11
	; v75 = iadd v48, v74
	madd	x11, x19, x20, x11
	; v76 = iadd v50, v75
	madd	x11, x19, x21, x11
	; v77 = iadd v52, v76
	madd	x11, x19, x22, x11
	; v78 = iadd v54, v77
	madd	x11, x19, x23, x11
	; v79 = iadd v56, v78
	madd	x11, x19, x24, x11
	; v80 = iadd v58, v79
	madd	x11, x19, x25, x11
	; v81 = iadd v60, v80
	madd	x11, x19, x26, x11
	; v82 = iadd v62, v81
	madd	x11, x19, x27, x11
	; v83 = iadd v64, v82
	madd	x11, x19, x28, x11
	; v84 = iadd v66, v83
	madd	x9, x19, x9, x11
	; v85 = iadd v68, v84
	madd	x9, x19, x10, x9
	; v86 = symbol_addr @sink
	adrp	x10, _sink@PAGE
	add	x10, x10, _sink@PAGEOFF
	; store v85, v86
	str	x9, [x10]
	; v87 = iconst.i32 1
	movz	w9, #0x1
	; v103 = stack_addr ss0
	add	x10, sp, #32
	; call @longjmp(v103, v87)
	mov	x0, x10
	mov	x1, x9
	bl	_longjmp
	; trap
	brk	#1
Lk_setjmp_swapped_end:
	.cfi_endproc
	.globl	_sink
	.zerofill	__DATA,__bss,_sink,8,3
	.subsections_via_symbols
