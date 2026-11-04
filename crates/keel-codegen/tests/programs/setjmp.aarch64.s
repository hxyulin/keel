	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_setjmp
	.p2align	2
_k_setjmp:
Lk_setjmp_begin:
	; regalloc: v0=x19 v1=x9 v2=[sp, #0] v3=x9 v4=[sp, #8] v5=x9 v6=[sp, #16] v7=x9 v8=[sp, #24] v9=x9 v10=[sp, #32] v11=x9 v12=[sp, #40] v13=x9 v14=[sp, #48] v15=x9 v16=[sp, #56] v17=x9 v18=[sp, #64] v19=x9 v20=[sp, #72] v21=x9 v22=[sp, #80] v23=x9 v24=[sp, #88] v25=x9 v26=[sp, #96] v27=x9 v28=[sp, #104] v29=[sp, #112] v30=x9 v31=x10 v32=x9 v33=x9 v34=[sp, #120] v35=x10 v36=[sp, #128] v37=x11 v38=[sp, #136] v39=x12 v40=x12 v41=x13 v42=x13 v43=x14 v44=x14 v45=x15 v46=x15 v47=x27 v48=x27 v49=x26 v50=x26 v51=x25 v52=x25 v53=x24 v54=x24 v55=x23 v56=x23 v57=x22 v58=x22 v59=x21 v60=x21 v61=x20 v62=x20 v63=x28 v64=x28 v65=x9 v66=x9 v67=x10 v68=x10 v69=x11 v70=x11 v71=x11 v72=x11 v73=x11 v74=x11 v75=x11 v76=x11 v77=x11 v78=x11 v79=x11 v80=x11 v81=x11 v82=x11 v83=x11 v84=x9 v85=x9 v86=x10 v87=x9 v88=x9 v89=x9 v90=x9 v91=x9 v92=x9 v93=x9 v94=x9 v95=x9 v96=x9 v97=x9 v98=x9 v99=x9 v100=x9
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
	sub	sp, sp, #336
	mov	x19, x0
Lk_setjmp_block0:
	; v1 = iconst.i64 3
	movz	x9, #0x3
	; v2 = imul v0, v1
	mul	x16, x19, x9
	str	x16, [sp, #0]
	; v3 = iconst.i64 5
	movz	x9, #0x5
	; v4 = imul v0, v3
	mul	x16, x19, x9
	str	x16, [sp, #8]
	; v5 = iconst.i64 7
	movz	x9, #0x7
	; v6 = imul v0, v5
	mul	x16, x19, x9
	str	x16, [sp, #16]
	; v7 = iconst.i64 11
	movz	x9, #0xb
	; v8 = imul v0, v7
	mul	x16, x19, x9
	str	x16, [sp, #24]
	; v9 = iconst.i64 13
	movz	x9, #0xd
	; v10 = imul v0, v9
	mul	x16, x19, x9
	str	x16, [sp, #32]
	; v11 = iconst.i64 17
	movz	x9, #0x11
	; v12 = imul v0, v11
	mul	x16, x19, x9
	str	x16, [sp, #40]
	; v13 = iconst.i64 19
	movz	x9, #0x13
	; v14 = imul v0, v13
	mul	x16, x19, x9
	str	x16, [sp, #48]
	; v15 = iconst.i64 23
	movz	x9, #0x17
	; v16 = imul v0, v15
	mul	x16, x19, x9
	str	x16, [sp, #56]
	; v17 = iconst.i64 29
	movz	x9, #0x1d
	; v18 = imul v0, v17
	mul	x16, x19, x9
	str	x16, [sp, #64]
	; v19 = iconst.i64 31
	movz	x9, #0x1f
	; v20 = imul v0, v19
	mul	x16, x19, x9
	str	x16, [sp, #72]
	; v21 = iconst.i64 37
	movz	x9, #0x25
	; v22 = imul v0, v21
	mul	x16, x19, x9
	str	x16, [sp, #80]
	; v23 = iconst.i64 41
	movz	x9, #0x29
	; v24 = imul v0, v23
	mul	x16, x19, x9
	str	x16, [sp, #88]
	; v25 = iconst.i64 43
	movz	x9, #0x2b
	; v26 = imul v0, v25
	mul	x16, x19, x9
	str	x16, [sp, #96]
	; v27 = iconst.i64 47
	movz	x9, #0x2f
	; v28 = imul v0, v27
	mul	x16, x19, x9
	str	x16, [sp, #104]
	; v29 = stack_addr ss0
	add	x16, sp, #144
	str	x16, [sp, #112]
	; v30 = call @setjmp(v29)
	ldr	x0, [sp, #112]
	bl	_setjmp
	mov	x9, x0
	; v31 = iconst.i32 0
	; v32 = icmp.ne v30, v31
	cmp	w9, #0
	cset	w9, ne
	; brif v32, block2, block1
	tst	w9, #255
	b.ne	Lk_setjmp_block2
Lk_setjmp_block1:
	; v33 = iconst.i64 101
	movz	x9, #0x65
	; v34 = imul v0, v33
	mul	x16, x19, x9
	str	x16, [sp, #120]
	; v35 = iconst.i64 103
	movz	x10, #0x67
	; v36 = imul v0, v35
	mul	x16, x19, x10
	str	x16, [sp, #128]
	; v37 = iconst.i64 105
	movz	x11, #0x69
	; v38 = imul v0, v37
	mul	x16, x19, x11
	str	x16, [sp, #136]
	; v39 = iconst.i64 107
	movz	x12, #0x6b
	; v40 = imul v0, v39
	mul	x12, x19, x12
	; v41 = iconst.i64 109
	movz	x13, #0x6d
	; v42 = imul v0, v41
	mul	x13, x19, x13
	; v43 = iconst.i64 111
	movz	x14, #0x6f
	; v44 = imul v0, v43
	mul	x14, x19, x14
	; v45 = iconst.i64 113
	movz	x15, #0x71
	; v46 = imul v0, v45
	mul	x15, x19, x15
	; v47 = iconst.i64 115
	movz	x27, #0x73
	; v48 = imul v0, v47
	mul	x27, x19, x27
	; v49 = iconst.i64 117
	movz	x26, #0x75
	; v50 = imul v0, v49
	mul	x26, x19, x26
	; v51 = iconst.i64 119
	movz	x25, #0x77
	; v52 = imul v0, v51
	mul	x25, x19, x25
	; v53 = iconst.i64 121
	movz	x24, #0x79
	; v54 = imul v0, v53
	mul	x24, x19, x24
	; v55 = iconst.i64 123
	movz	x23, #0x7b
	; v56 = imul v0, v55
	mul	x23, x19, x23
	; v57 = iconst.i64 125
	movz	x22, #0x7d
	; v58 = imul v0, v57
	mul	x22, x19, x22
	; v59 = iconst.i64 127
	movz	x21, #0x7f
	; v60 = imul v0, v59
	mul	x21, x19, x21
	; v61 = iconst.i64 129
	movz	x20, #0x81
	; v62 = imul v0, v61
	mul	x20, x19, x20
	; v63 = iconst.i64 131
	movz	x28, #0x83
	; v64 = imul v0, v63
	mul	x28, x19, x28
	; v65 = iconst.i64 133
	movz	x9, #0x85
	; v66 = imul v0, v65
	mul	x9, x19, x9
	; v67 = iconst.i64 135
	movz	x10, #0x87
	; v68 = imul v0, v67
	mul	x10, x19, x10
	; v69 = iadd v34, v36
	ldr	x16, [sp, #120]
	ldr	x17, [sp, #128]
	add	x11, x16, x17
	; v70 = iadd v69, v38
	ldr	x17, [sp, #136]
	add	x11, x11, x17
	; v71 = iadd v70, v40
	add	x11, x11, x12
	; v72 = iadd v71, v42
	add	x11, x11, x13
	; v73 = iadd v72, v44
	add	x11, x11, x14
	; v74 = iadd v73, v46
	add	x11, x11, x15
	; v75 = iadd v74, v48
	add	x11, x11, x27
	; v76 = iadd v75, v50
	add	x11, x11, x26
	; v77 = iadd v76, v52
	add	x11, x11, x25
	; v78 = iadd v77, v54
	add	x11, x11, x24
	; v79 = iadd v78, v56
	add	x11, x11, x23
	; v80 = iadd v79, v58
	add	x11, x11, x22
	; v81 = iadd v80, v60
	add	x11, x11, x21
	; v82 = iadd v81, v62
	add	x11, x11, x20
	; v83 = iadd v82, v64
	add	x11, x11, x28
	; v84 = iadd v83, v66
	add	x9, x11, x9
	; v85 = iadd v84, v68
	add	x9, x9, x10
	; v86 = symbol_addr @sink
	adrp	x10, _sink@PAGE
	add	x10, x10, _sink@PAGEOFF
	; store v85, v86
	str	x9, [x10]
	; v87 = iconst.i32 1
	movz	x9, #0x1
	; call @longjmp(v29, v87)
	ldr	x0, [sp, #112]
	mov	x1, x9
	bl	_longjmp
	; trap
	brk	#1
Lk_setjmp_block2:
	; v88 = iadd v2, v4
	ldr	x16, [sp, #0]
	ldr	x17, [sp, #8]
	add	x9, x16, x17
	; v89 = iadd v88, v6
	ldr	x17, [sp, #16]
	add	x9, x9, x17
	; v90 = iadd v89, v8
	ldr	x17, [sp, #24]
	add	x9, x9, x17
	; v91 = iadd v90, v10
	ldr	x17, [sp, #32]
	add	x9, x9, x17
	; v92 = iadd v91, v12
	ldr	x17, [sp, #40]
	add	x9, x9, x17
	; v93 = iadd v92, v14
	ldr	x17, [sp, #48]
	add	x9, x9, x17
	; v94 = iadd v93, v16
	ldr	x17, [sp, #56]
	add	x9, x9, x17
	; v95 = iadd v94, v18
	ldr	x17, [sp, #64]
	add	x9, x9, x17
	; v96 = iadd v95, v20
	ldr	x17, [sp, #72]
	add	x9, x9, x17
	; v97 = iadd v96, v22
	ldr	x17, [sp, #80]
	add	x9, x9, x17
	; v98 = iadd v97, v24
	ldr	x17, [sp, #88]
	add	x9, x9, x17
	; v99 = iadd v98, v26
	ldr	x17, [sp, #96]
	add	x9, x9, x17
	; v100 = iadd v99, v28
	ldr	x17, [sp, #104]
	add	x9, x9, x17
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
	; regalloc: v0=x19 v1=x9 v2=[sp, #0] v3=x9 v4=[sp, #8] v5=x9 v6=[sp, #16] v7=x9 v8=[sp, #24] v9=x9 v10=[sp, #32] v11=x9 v12=[sp, #40] v13=x9 v14=[sp, #48] v15=x9 v16=[sp, #56] v17=x9 v18=[sp, #64] v19=x9 v20=[sp, #72] v21=x9 v22=[sp, #80] v23=x9 v24=[sp, #88] v25=x9 v26=[sp, #96] v27=x9 v28=[sp, #104] v29=[sp, #112] v30=x9 v31=x10 v32=x9 v33=x9 v34=[sp, #120] v35=x10 v36=[sp, #128] v37=x11 v38=[sp, #136] v39=x12 v40=x12 v41=x13 v42=x13 v43=x14 v44=x14 v45=x15 v46=x15 v47=x27 v48=x27 v49=x26 v50=x26 v51=x25 v52=x25 v53=x24 v54=x24 v55=x23 v56=x23 v57=x22 v58=x22 v59=x21 v60=x21 v61=x20 v62=x20 v63=x28 v64=x28 v65=x9 v66=x9 v67=x10 v68=x10 v69=x11 v70=x11 v71=x11 v72=x11 v73=x11 v74=x11 v75=x11 v76=x11 v77=x11 v78=x11 v79=x11 v80=x11 v81=x11 v82=x11 v83=x11 v84=x9 v85=x9 v86=x10 v87=x9 v88=x9 v89=x9 v90=x9 v91=x9 v92=x9 v93=x9 v94=x9 v95=x9 v96=x9 v97=x9 v98=x9 v99=x9 v100=x9
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
	sub	sp, sp, #336
	mov	x19, x0
Lk_setjmp_swapped_block0:
	; v1 = iconst.i64 3
	movz	x9, #0x3
	; v2 = imul v0, v1
	mul	x16, x19, x9
	str	x16, [sp, #0]
	; v3 = iconst.i64 5
	movz	x9, #0x5
	; v4 = imul v0, v3
	mul	x16, x19, x9
	str	x16, [sp, #8]
	; v5 = iconst.i64 7
	movz	x9, #0x7
	; v6 = imul v0, v5
	mul	x16, x19, x9
	str	x16, [sp, #16]
	; v7 = iconst.i64 11
	movz	x9, #0xb
	; v8 = imul v0, v7
	mul	x16, x19, x9
	str	x16, [sp, #24]
	; v9 = iconst.i64 13
	movz	x9, #0xd
	; v10 = imul v0, v9
	mul	x16, x19, x9
	str	x16, [sp, #32]
	; v11 = iconst.i64 17
	movz	x9, #0x11
	; v12 = imul v0, v11
	mul	x16, x19, x9
	str	x16, [sp, #40]
	; v13 = iconst.i64 19
	movz	x9, #0x13
	; v14 = imul v0, v13
	mul	x16, x19, x9
	str	x16, [sp, #48]
	; v15 = iconst.i64 23
	movz	x9, #0x17
	; v16 = imul v0, v15
	mul	x16, x19, x9
	str	x16, [sp, #56]
	; v17 = iconst.i64 29
	movz	x9, #0x1d
	; v18 = imul v0, v17
	mul	x16, x19, x9
	str	x16, [sp, #64]
	; v19 = iconst.i64 31
	movz	x9, #0x1f
	; v20 = imul v0, v19
	mul	x16, x19, x9
	str	x16, [sp, #72]
	; v21 = iconst.i64 37
	movz	x9, #0x25
	; v22 = imul v0, v21
	mul	x16, x19, x9
	str	x16, [sp, #80]
	; v23 = iconst.i64 41
	movz	x9, #0x29
	; v24 = imul v0, v23
	mul	x16, x19, x9
	str	x16, [sp, #88]
	; v25 = iconst.i64 43
	movz	x9, #0x2b
	; v26 = imul v0, v25
	mul	x16, x19, x9
	str	x16, [sp, #96]
	; v27 = iconst.i64 47
	movz	x9, #0x2f
	; v28 = imul v0, v27
	mul	x16, x19, x9
	str	x16, [sp, #104]
	; v29 = stack_addr ss0
	add	x16, sp, #144
	str	x16, [sp, #112]
	; v30 = call @setjmp(v29)
	ldr	x0, [sp, #112]
	bl	_setjmp
	mov	x9, x0
	; v31 = iconst.i32 0
	; v32 = icmp.eq v30, v31
	cmp	w9, #0
	cset	w9, eq
	; brif v32, block1, block2
	tst	w9, #255
	b.ne	Lk_setjmp_swapped_block1
Lk_setjmp_swapped_block2:
	; v88 = iadd v2, v4
	ldr	x16, [sp, #0]
	ldr	x17, [sp, #8]
	add	x9, x16, x17
	; v89 = iadd v88, v6
	ldr	x17, [sp, #16]
	add	x9, x9, x17
	; v90 = iadd v89, v8
	ldr	x17, [sp, #24]
	add	x9, x9, x17
	; v91 = iadd v90, v10
	ldr	x17, [sp, #32]
	add	x9, x9, x17
	; v92 = iadd v91, v12
	ldr	x17, [sp, #40]
	add	x9, x9, x17
	; v93 = iadd v92, v14
	ldr	x17, [sp, #48]
	add	x9, x9, x17
	; v94 = iadd v93, v16
	ldr	x17, [sp, #56]
	add	x9, x9, x17
	; v95 = iadd v94, v18
	ldr	x17, [sp, #64]
	add	x9, x9, x17
	; v96 = iadd v95, v20
	ldr	x17, [sp, #72]
	add	x9, x9, x17
	; v97 = iadd v96, v22
	ldr	x17, [sp, #80]
	add	x9, x9, x17
	; v98 = iadd v97, v24
	ldr	x17, [sp, #88]
	add	x9, x9, x17
	; v99 = iadd v98, v26
	ldr	x17, [sp, #96]
	add	x9, x9, x17
	; v100 = iadd v99, v28
	ldr	x17, [sp, #104]
	add	x9, x9, x17
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
	str	x16, [sp, #120]
	; v35 = iconst.i64 103
	movz	x10, #0x67
	; v36 = imul v0, v35
	mul	x16, x19, x10
	str	x16, [sp, #128]
	; v37 = iconst.i64 105
	movz	x11, #0x69
	; v38 = imul v0, v37
	mul	x16, x19, x11
	str	x16, [sp, #136]
	; v39 = iconst.i64 107
	movz	x12, #0x6b
	; v40 = imul v0, v39
	mul	x12, x19, x12
	; v41 = iconst.i64 109
	movz	x13, #0x6d
	; v42 = imul v0, v41
	mul	x13, x19, x13
	; v43 = iconst.i64 111
	movz	x14, #0x6f
	; v44 = imul v0, v43
	mul	x14, x19, x14
	; v45 = iconst.i64 113
	movz	x15, #0x71
	; v46 = imul v0, v45
	mul	x15, x19, x15
	; v47 = iconst.i64 115
	movz	x27, #0x73
	; v48 = imul v0, v47
	mul	x27, x19, x27
	; v49 = iconst.i64 117
	movz	x26, #0x75
	; v50 = imul v0, v49
	mul	x26, x19, x26
	; v51 = iconst.i64 119
	movz	x25, #0x77
	; v52 = imul v0, v51
	mul	x25, x19, x25
	; v53 = iconst.i64 121
	movz	x24, #0x79
	; v54 = imul v0, v53
	mul	x24, x19, x24
	; v55 = iconst.i64 123
	movz	x23, #0x7b
	; v56 = imul v0, v55
	mul	x23, x19, x23
	; v57 = iconst.i64 125
	movz	x22, #0x7d
	; v58 = imul v0, v57
	mul	x22, x19, x22
	; v59 = iconst.i64 127
	movz	x21, #0x7f
	; v60 = imul v0, v59
	mul	x21, x19, x21
	; v61 = iconst.i64 129
	movz	x20, #0x81
	; v62 = imul v0, v61
	mul	x20, x19, x20
	; v63 = iconst.i64 131
	movz	x28, #0x83
	; v64 = imul v0, v63
	mul	x28, x19, x28
	; v65 = iconst.i64 133
	movz	x9, #0x85
	; v66 = imul v0, v65
	mul	x9, x19, x9
	; v67 = iconst.i64 135
	movz	x10, #0x87
	; v68 = imul v0, v67
	mul	x10, x19, x10
	; v69 = iadd v34, v36
	ldr	x16, [sp, #120]
	ldr	x17, [sp, #128]
	add	x11, x16, x17
	; v70 = iadd v69, v38
	ldr	x17, [sp, #136]
	add	x11, x11, x17
	; v71 = iadd v70, v40
	add	x11, x11, x12
	; v72 = iadd v71, v42
	add	x11, x11, x13
	; v73 = iadd v72, v44
	add	x11, x11, x14
	; v74 = iadd v73, v46
	add	x11, x11, x15
	; v75 = iadd v74, v48
	add	x11, x11, x27
	; v76 = iadd v75, v50
	add	x11, x11, x26
	; v77 = iadd v76, v52
	add	x11, x11, x25
	; v78 = iadd v77, v54
	add	x11, x11, x24
	; v79 = iadd v78, v56
	add	x11, x11, x23
	; v80 = iadd v79, v58
	add	x11, x11, x22
	; v81 = iadd v80, v60
	add	x11, x11, x21
	; v82 = iadd v81, v62
	add	x11, x11, x20
	; v83 = iadd v82, v64
	add	x11, x11, x28
	; v84 = iadd v83, v66
	add	x9, x11, x9
	; v85 = iadd v84, v68
	add	x9, x9, x10
	; v86 = symbol_addr @sink
	adrp	x10, _sink@PAGE
	add	x10, x10, _sink@PAGEOFF
	; store v85, v86
	str	x9, [x10]
	; v87 = iconst.i32 1
	movz	x9, #0x1
	; call @longjmp(v29, v87)
	ldr	x0, [sp, #112]
	mov	x1, x9
	bl	_longjmp
	; trap
	brk	#1
Lk_setjmp_swapped_end:
	.cfi_endproc
	.globl	_sink
	.zerofill	__DATA,__bss,_sink,8,3
	.subsections_via_symbols
