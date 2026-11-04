	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_pressure
	.p2align	2
_pressure:
Lpressure_begin:
	; regalloc: v0=x9 v2=[sp, #0] v4=[sp, #8] v6=[sp, #16] v8=[sp, #24] v10=[sp, #32] v12=[sp, #40] v14=[sp, #48] v16=[sp, #56] v18=x21 v20=x22 v22=x23 v24=x24 v26=x25 v28=x26 v30=x27 v32=x28 v34=x10 v36=x11 v38=x12 v40=x13 v42=x14 v44=x15 v46=x19 v48=x9 v49=x20 v54=x9 v56=x9 v58=x9 v60=x9 v62=x9 v64=x9 v66=x9 v68=x9 v70=x9 v72=x9 v74=x9 v76=x9 v78=x9 v80=x9 v82=x9 v84=x9 v86=x9 v88=x9 v90=x9 v92=x9 v94=x9 v96=x9 v98=x9
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
	sub	sp, sp, #64
	mov	x9, x0
Lpressure_block0:
	; v1 = iconst.i64 1
	; v2 = iadd v0, v1
	add	x16, x9, #1
	str	x16, [sp, #0]
	; v3 = iconst.i64 2
	; v4 = iadd v0, v3
	add	x16, x9, #2
	str	x16, [sp, #8]
	; v5 = iconst.i64 5
	; v6 = iadd v0, v5
	add	x16, x9, #5
	str	x16, [sp, #16]
	; v7 = iconst.i64 10
	; v8 = iadd v0, v7
	add	x16, x9, #10
	str	x16, [sp, #24]
	; v9 = iconst.i64 17
	; v10 = iadd v0, v9
	add	x16, x9, #17
	str	x16, [sp, #32]
	; v11 = iconst.i64 26
	; v12 = iadd v0, v11
	add	x16, x9, #26
	str	x16, [sp, #40]
	; v13 = iconst.i64 37
	; v14 = iadd v0, v13
	add	x16, x9, #37
	str	x16, [sp, #48]
	; v15 = iconst.i64 50
	; v16 = iadd v0, v15
	add	x16, x9, #50
	str	x16, [sp, #56]
	; v17 = iconst.i64 65
	; v18 = iadd v0, v17
	add	x21, x9, #65
	; v19 = iconst.i64 82
	; v20 = iadd v0, v19
	add	x22, x9, #82
	; v21 = iconst.i64 101
	; v22 = iadd v0, v21
	add	x23, x9, #101
	; v23 = iconst.i64 122
	; v24 = iadd v0, v23
	add	x24, x9, #122
	; v25 = iconst.i64 145
	; v26 = iadd v0, v25
	add	x25, x9, #145
	; v27 = iconst.i64 170
	; v28 = iadd v0, v27
	add	x26, x9, #170
	; v29 = iconst.i64 197
	; v30 = iadd v0, v29
	add	x27, x9, #197
	; v31 = iconst.i64 226
	; v32 = iadd v0, v31
	add	x28, x9, #226
	; v33 = iconst.i64 257
	; v34 = iadd v0, v33
	add	x10, x9, #257
	; v35 = iconst.i64 290
	; v36 = iadd v0, v35
	add	x11, x9, #290
	; v37 = iconst.i64 325
	; v38 = iadd v0, v37
	add	x12, x9, #325
	; v39 = iconst.i64 362
	; v40 = iadd v0, v39
	add	x13, x9, #362
	; v41 = iconst.i64 401
	; v42 = iadd v0, v41
	add	x14, x9, #401
	; v43 = iconst.i64 442
	; v44 = iadd v0, v43
	add	x15, x9, #442
	; v45 = iconst.i64 485
	; v46 = iadd v0, v45
	add	x19, x9, #485
	; v47 = iconst.i64 530
	; v48 = iadd v0, v47
	add	x9, x9, #530
	; v49 = iconst.i64 3
	movz	x20, #0x3
	; v53 = imul v48, v49
	; v54 = iadd v46, v53
	madd	x9, x9, x20, x19
	; v55 = imul v54, v49
	; v56 = iadd v44, v55
	madd	x9, x9, x20, x15
	; v57 = imul v56, v49
	; v58 = iadd v42, v57
	madd	x9, x9, x20, x14
	; v59 = imul v58, v49
	; v60 = iadd v40, v59
	madd	x9, x9, x20, x13
	; v61 = imul v60, v49
	; v62 = iadd v38, v61
	madd	x9, x9, x20, x12
	; v63 = imul v62, v49
	; v64 = iadd v36, v63
	madd	x9, x9, x20, x11
	; v65 = imul v64, v49
	; v66 = iadd v34, v65
	madd	x9, x9, x20, x10
	; v67 = imul v66, v49
	; v68 = iadd v32, v67
	madd	x9, x9, x20, x28
	; v69 = imul v68, v49
	; v70 = iadd v30, v69
	madd	x9, x9, x20, x27
	; v71 = imul v70, v49
	; v72 = iadd v28, v71
	madd	x9, x9, x20, x26
	; v73 = imul v72, v49
	; v74 = iadd v26, v73
	madd	x9, x9, x20, x25
	; v75 = imul v74, v49
	; v76 = iadd v24, v75
	madd	x9, x9, x20, x24
	; v77 = imul v76, v49
	; v78 = iadd v22, v77
	madd	x9, x9, x20, x23
	; v79 = imul v78, v49
	; v80 = iadd v20, v79
	madd	x9, x9, x20, x22
	; v81 = imul v80, v49
	; v82 = iadd v18, v81
	madd	x9, x9, x20, x21
	; v83 = imul v82, v49
	; v84 = iadd v16, v83
	ldr	x16, [sp, #56]
	madd	x9, x9, x20, x16
	; v85 = imul v84, v49
	; v86 = iadd v14, v85
	ldr	x16, [sp, #48]
	madd	x9, x9, x20, x16
	; v87 = imul v86, v49
	; v88 = iadd v12, v87
	ldr	x16, [sp, #40]
	madd	x9, x9, x20, x16
	; v89 = imul v88, v49
	; v90 = iadd v10, v89
	ldr	x16, [sp, #32]
	madd	x9, x9, x20, x16
	; v91 = imul v90, v49
	; v92 = iadd v8, v91
	ldr	x16, [sp, #24]
	madd	x9, x9, x20, x16
	; v93 = imul v92, v49
	; v94 = iadd v6, v93
	ldr	x16, [sp, #16]
	madd	x9, x9, x20, x16
	; v95 = imul v94, v49
	; v96 = iadd v4, v95
	ldr	x16, [sp, #8]
	madd	x9, x9, x20, x16
	; v97 = imul v96, v49
	; v98 = iadd v2, v97
	ldr	x16, [sp, #0]
	madd	x9, x9, x20, x16
	; return v98
	mov	x0, x9
	sub	sp, x29, #80
	ldp	x27, x28, [sp], #16
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lpressure_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_pressure_call
	.p2align	2
_pressure_call:
Lpressure_call_begin:
	; regalloc: v0=x19 v2=[sp, #0] v4=[sp, #8] v6=[sp, #16] v8=[sp, #24] v10=[sp, #32] v12=[sp, #40] v14=[sp, #48] v16=[sp, #56] v18=[sp, #64] v20=[sp, #72] v22=[sp, #80] v24=[sp, #88] v26=[sp, #96] v28=[sp, #104] v30=x25 v32=x26 v34=x27 v36=x28 v38=x20 v40=x21 v42=x22 v44=x23 v46=x24 v48=x19 v49=x9 v54=x10 v56=x10 v58=x10 v60=x10 v62=x10 v64=x10 v66=x10 v68=x10 v70=x10 v72=x10 v74=x10 v76=x10 v78=x10 v80=x10 v82=x10 v84=x10 v86=x10 v88=x10 v90=x10 v92=x10 v94=x10 v96=x10 v98=x9
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
	sub	sp, sp, #112
	mov	x19, x0
Lpressure_call_block0:
	; v1 = iconst.i64 1
	; v2 = iadd v0, v1
	add	x16, x19, #1
	str	x16, [sp, #0]
	; v3 = iconst.i64 2
	; v4 = iadd v0, v3
	add	x16, x19, #2
	str	x16, [sp, #8]
	; v5 = iconst.i64 5
	; v6 = iadd v0, v5
	add	x16, x19, #5
	str	x16, [sp, #16]
	; v7 = iconst.i64 10
	; v8 = iadd v0, v7
	add	x16, x19, #10
	str	x16, [sp, #24]
	; v9 = iconst.i64 17
	; v10 = iadd v0, v9
	add	x16, x19, #17
	str	x16, [sp, #32]
	; v11 = iconst.i64 26
	; v12 = iadd v0, v11
	add	x16, x19, #26
	str	x16, [sp, #40]
	; v13 = iconst.i64 37
	; v14 = iadd v0, v13
	add	x16, x19, #37
	str	x16, [sp, #48]
	; v15 = iconst.i64 50
	; v16 = iadd v0, v15
	add	x16, x19, #50
	str	x16, [sp, #56]
	; v17 = iconst.i64 65
	; v18 = iadd v0, v17
	add	x16, x19, #65
	str	x16, [sp, #64]
	; v19 = iconst.i64 82
	; v20 = iadd v0, v19
	add	x16, x19, #82
	str	x16, [sp, #72]
	; v21 = iconst.i64 101
	; v22 = iadd v0, v21
	add	x16, x19, #101
	str	x16, [sp, #80]
	; v23 = iconst.i64 122
	; v24 = iadd v0, v23
	add	x16, x19, #122
	str	x16, [sp, #88]
	; call @clobber()
	bl	_clobber
	; v25 = iconst.i64 145
	; v26 = iadd v0, v25
	add	x16, x19, #145
	str	x16, [sp, #96]
	; v27 = iconst.i64 170
	; v28 = iadd v0, v27
	add	x16, x19, #170
	str	x16, [sp, #104]
	; v29 = iconst.i64 197
	; v30 = iadd v0, v29
	add	x25, x19, #197
	; v31 = iconst.i64 226
	; v32 = iadd v0, v31
	add	x26, x19, #226
	; v33 = iconst.i64 257
	; v34 = iadd v0, v33
	add	x27, x19, #257
	; v35 = iconst.i64 290
	; v36 = iadd v0, v35
	add	x28, x19, #290
	; v37 = iconst.i64 325
	; v38 = iadd v0, v37
	add	x20, x19, #325
	; v39 = iconst.i64 362
	; v40 = iadd v0, v39
	add	x21, x19, #362
	; v41 = iconst.i64 401
	; v42 = iadd v0, v41
	add	x22, x19, #401
	; v43 = iconst.i64 442
	; v44 = iadd v0, v43
	add	x23, x19, #442
	; v45 = iconst.i64 485
	; v46 = iadd v0, v45
	add	x24, x19, #485
	; v47 = iconst.i64 530
	; v48 = iadd v0, v47
	add	x19, x19, #530
	; call @clobber()
	bl	_clobber
	; v49 = iconst.i64 3
	movz	x9, #0x3
	; v53 = imul v48, v49
	; v54 = iadd v46, v53
	madd	x10, x19, x9, x24
	; v55 = imul v54, v49
	; v56 = iadd v44, v55
	madd	x10, x10, x9, x23
	; v57 = imul v56, v49
	; v58 = iadd v42, v57
	madd	x10, x10, x9, x22
	; v59 = imul v58, v49
	; v60 = iadd v40, v59
	madd	x10, x10, x9, x21
	; v61 = imul v60, v49
	; v62 = iadd v38, v61
	madd	x10, x10, x9, x20
	; v63 = imul v62, v49
	; v64 = iadd v36, v63
	madd	x10, x10, x9, x28
	; v65 = imul v64, v49
	; v66 = iadd v34, v65
	madd	x10, x10, x9, x27
	; v67 = imul v66, v49
	; v68 = iadd v32, v67
	madd	x10, x10, x9, x26
	; v69 = imul v68, v49
	; v70 = iadd v30, v69
	madd	x10, x10, x9, x25
	; v71 = imul v70, v49
	; v72 = iadd v28, v71
	ldr	x16, [sp, #104]
	madd	x10, x10, x9, x16
	; v73 = imul v72, v49
	; v74 = iadd v26, v73
	ldr	x16, [sp, #96]
	madd	x10, x10, x9, x16
	; v75 = imul v74, v49
	; v76 = iadd v24, v75
	ldr	x16, [sp, #88]
	madd	x10, x10, x9, x16
	; v77 = imul v76, v49
	; v78 = iadd v22, v77
	ldr	x16, [sp, #80]
	madd	x10, x10, x9, x16
	; v79 = imul v78, v49
	; v80 = iadd v20, v79
	ldr	x16, [sp, #72]
	madd	x10, x10, x9, x16
	; v81 = imul v80, v49
	; v82 = iadd v18, v81
	ldr	x16, [sp, #64]
	madd	x10, x10, x9, x16
	; v83 = imul v82, v49
	; v84 = iadd v16, v83
	ldr	x16, [sp, #56]
	madd	x10, x10, x9, x16
	; v85 = imul v84, v49
	; v86 = iadd v14, v85
	ldr	x16, [sp, #48]
	madd	x10, x10, x9, x16
	; v87 = imul v86, v49
	; v88 = iadd v12, v87
	ldr	x16, [sp, #40]
	madd	x10, x10, x9, x16
	; v89 = imul v88, v49
	; v90 = iadd v10, v89
	ldr	x16, [sp, #32]
	madd	x10, x10, x9, x16
	; v91 = imul v90, v49
	; v92 = iadd v8, v91
	ldr	x16, [sp, #24]
	madd	x10, x10, x9, x16
	; v93 = imul v92, v49
	; v94 = iadd v6, v93
	ldr	x16, [sp, #16]
	madd	x10, x10, x9, x16
	; v95 = imul v94, v49
	; v96 = iadd v4, v95
	ldr	x16, [sp, #8]
	madd	x10, x10, x9, x16
	; v97 = imul v96, v49
	; v98 = iadd v2, v97
	ldr	x16, [sp, #0]
	madd	x9, x10, x9, x16
	; return v98
	mov	x0, x9
	sub	sp, x29, #80
	ldp	x27, x28, [sp], #16
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lpressure_call_end:
	.cfi_endproc
	.subsections_via_symbols
