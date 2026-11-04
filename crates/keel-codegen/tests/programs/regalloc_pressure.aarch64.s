	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_pressure
	.p2align	2
_pressure:
Lpressure_begin:
	; regalloc: v0=x9 v1=x10 v2=[sp, #0] v3=x11 v4=[sp, #8] v5=x12 v6=[sp, #16] v7=x13 v8=[sp, #24] v9=x14 v10=[sp, #32] v11=x15 v12=[sp, #40] v13=x19 v14=[sp, #48] v15=x20 v16=[sp, #56] v17=x21 v18=[sp, #64] v19=x22 v20=x22 v21=x23 v22=x23 v23=x24 v24=x24 v25=x25 v26=x25 v27=x26 v28=x26 v29=x27 v30=x27 v31=x28 v32=x28 v33=x10 v34=x10 v35=x11 v36=x11 v37=x12 v38=x12 v39=x13 v40=x13 v41=x14 v42=x14 v43=x15 v44=x15 v45=x19 v46=x19 v47=x20 v48=x9 v49=x20 v50=x21 v51=x21 v52=x9 v53=x9 v54=x9 v55=x9 v56=x9 v57=x9 v58=x9 v59=x9 v60=x9 v61=x9 v62=x9 v63=x9 v64=x9 v65=x9 v66=x9 v67=x9 v68=x9 v69=x9 v70=x9 v71=x9 v72=x9 v73=x9 v74=x9 v75=x9 v76=x9 v77=x9 v78=x9 v79=x9 v80=x9 v81=x9 v82=x9 v83=x9 v84=x9 v85=x9 v86=x9 v87=x9 v88=x9 v89=x9 v90=x9 v91=x9 v92=x9 v93=x9 v94=x9 v95=x9 v96=x9 v97=x9 v98=x9
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
	sub	sp, sp, #80
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
	add	x16, x9, #65
	str	x16, [sp, #64]
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
	; v50 = iconst.i64 0
	movz	x21, #0x0
	; v51 = imul v50, v49
	mul	x21, x21, x20
	; v52 = iadd v51, v48
	add	x9, x21, x9
	; v53 = imul v52, v49
	mul	x9, x9, x20
	; v54 = iadd v53, v46
	add	x9, x9, x19
	; v55 = imul v54, v49
	mul	x9, x9, x20
	; v56 = iadd v55, v44
	add	x9, x9, x15
	; v57 = imul v56, v49
	mul	x9, x9, x20
	; v58 = iadd v57, v42
	add	x9, x9, x14
	; v59 = imul v58, v49
	mul	x9, x9, x20
	; v60 = iadd v59, v40
	add	x9, x9, x13
	; v61 = imul v60, v49
	mul	x9, x9, x20
	; v62 = iadd v61, v38
	add	x9, x9, x12
	; v63 = imul v62, v49
	mul	x9, x9, x20
	; v64 = iadd v63, v36
	add	x9, x9, x11
	; v65 = imul v64, v49
	mul	x9, x9, x20
	; v66 = iadd v65, v34
	add	x9, x9, x10
	; v67 = imul v66, v49
	mul	x9, x9, x20
	; v68 = iadd v67, v32
	add	x9, x9, x28
	; v69 = imul v68, v49
	mul	x9, x9, x20
	; v70 = iadd v69, v30
	add	x9, x9, x27
	; v71 = imul v70, v49
	mul	x9, x9, x20
	; v72 = iadd v71, v28
	add	x9, x9, x26
	; v73 = imul v72, v49
	mul	x9, x9, x20
	; v74 = iadd v73, v26
	add	x9, x9, x25
	; v75 = imul v74, v49
	mul	x9, x9, x20
	; v76 = iadd v75, v24
	add	x9, x9, x24
	; v77 = imul v76, v49
	mul	x9, x9, x20
	; v78 = iadd v77, v22
	add	x9, x9, x23
	; v79 = imul v78, v49
	mul	x9, x9, x20
	; v80 = iadd v79, v20
	add	x9, x9, x22
	; v81 = imul v80, v49
	mul	x9, x9, x20
	; v82 = iadd v81, v18
	ldr	x17, [sp, #64]
	add	x9, x9, x17
	; v83 = imul v82, v49
	mul	x9, x9, x20
	; v84 = iadd v83, v16
	ldr	x17, [sp, #56]
	add	x9, x9, x17
	; v85 = imul v84, v49
	mul	x9, x9, x20
	; v86 = iadd v85, v14
	ldr	x17, [sp, #48]
	add	x9, x9, x17
	; v87 = imul v86, v49
	mul	x9, x9, x20
	; v88 = iadd v87, v12
	ldr	x17, [sp, #40]
	add	x9, x9, x17
	; v89 = imul v88, v49
	mul	x9, x9, x20
	; v90 = iadd v89, v10
	ldr	x17, [sp, #32]
	add	x9, x9, x17
	; v91 = imul v90, v49
	mul	x9, x9, x20
	; v92 = iadd v91, v8
	ldr	x17, [sp, #24]
	add	x9, x9, x17
	; v93 = imul v92, v49
	mul	x9, x9, x20
	; v94 = iadd v93, v6
	ldr	x17, [sp, #16]
	add	x9, x9, x17
	; v95 = imul v94, v49
	mul	x9, x9, x20
	; v96 = iadd v95, v4
	ldr	x17, [sp, #8]
	add	x9, x9, x17
	; v97 = imul v96, v49
	mul	x9, x9, x20
	; v98 = iadd v97, v2
	ldr	x17, [sp, #0]
	add	x9, x9, x17
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
	; regalloc: v0=x19 v1=x9 v2=[sp, #0] v3=x9 v4=[sp, #8] v5=x9 v6=[sp, #16] v7=x9 v8=[sp, #24] v9=x9 v10=[sp, #32] v11=x9 v12=[sp, #40] v13=x9 v14=[sp, #48] v15=x9 v16=[sp, #56] v17=x9 v18=[sp, #64] v19=x9 v20=[sp, #72] v21=x9 v22=[sp, #80] v23=x9 v24=[sp, #88] v25=x9 v26=[sp, #96] v27=x9 v28=[sp, #104] v29=x9 v30=x25 v31=x9 v32=x26 v33=x9 v34=x27 v35=x9 v36=x28 v37=x9 v38=x20 v39=x9 v40=x21 v41=x9 v42=x22 v43=x9 v44=x23 v45=x9 v46=x24 v47=x9 v48=x19 v49=x9 v50=x10 v51=x10 v52=x10 v53=x10 v54=x10 v55=x10 v56=x10 v57=x10 v58=x10 v59=x10 v60=x10 v61=x10 v62=x10 v63=x10 v64=x10 v65=x10 v66=x10 v67=x10 v68=x10 v69=x10 v70=x10 v71=x10 v72=x10 v73=x10 v74=x10 v75=x10 v76=x10 v77=x10 v78=x10 v79=x10 v80=x10 v81=x10 v82=x10 v83=x10 v84=x10 v85=x10 v86=x10 v87=x10 v88=x10 v89=x10 v90=x10 v91=x10 v92=x10 v93=x10 v94=x10 v95=x10 v96=x10 v97=x9 v98=x9
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
	; v50 = iconst.i64 0
	movz	x10, #0x0
	; v51 = imul v50, v49
	mul	x10, x10, x9
	; v52 = iadd v51, v48
	add	x10, x10, x19
	; v53 = imul v52, v49
	mul	x10, x10, x9
	; v54 = iadd v53, v46
	add	x10, x10, x24
	; v55 = imul v54, v49
	mul	x10, x10, x9
	; v56 = iadd v55, v44
	add	x10, x10, x23
	; v57 = imul v56, v49
	mul	x10, x10, x9
	; v58 = iadd v57, v42
	add	x10, x10, x22
	; v59 = imul v58, v49
	mul	x10, x10, x9
	; v60 = iadd v59, v40
	add	x10, x10, x21
	; v61 = imul v60, v49
	mul	x10, x10, x9
	; v62 = iadd v61, v38
	add	x10, x10, x20
	; v63 = imul v62, v49
	mul	x10, x10, x9
	; v64 = iadd v63, v36
	add	x10, x10, x28
	; v65 = imul v64, v49
	mul	x10, x10, x9
	; v66 = iadd v65, v34
	add	x10, x10, x27
	; v67 = imul v66, v49
	mul	x10, x10, x9
	; v68 = iadd v67, v32
	add	x10, x10, x26
	; v69 = imul v68, v49
	mul	x10, x10, x9
	; v70 = iadd v69, v30
	add	x10, x10, x25
	; v71 = imul v70, v49
	mul	x10, x10, x9
	; v72 = iadd v71, v28
	ldr	x17, [sp, #104]
	add	x10, x10, x17
	; v73 = imul v72, v49
	mul	x10, x10, x9
	; v74 = iadd v73, v26
	ldr	x17, [sp, #96]
	add	x10, x10, x17
	; v75 = imul v74, v49
	mul	x10, x10, x9
	; v76 = iadd v75, v24
	ldr	x17, [sp, #88]
	add	x10, x10, x17
	; v77 = imul v76, v49
	mul	x10, x10, x9
	; v78 = iadd v77, v22
	ldr	x17, [sp, #80]
	add	x10, x10, x17
	; v79 = imul v78, v49
	mul	x10, x10, x9
	; v80 = iadd v79, v20
	ldr	x17, [sp, #72]
	add	x10, x10, x17
	; v81 = imul v80, v49
	mul	x10, x10, x9
	; v82 = iadd v81, v18
	ldr	x17, [sp, #64]
	add	x10, x10, x17
	; v83 = imul v82, v49
	mul	x10, x10, x9
	; v84 = iadd v83, v16
	ldr	x17, [sp, #56]
	add	x10, x10, x17
	; v85 = imul v84, v49
	mul	x10, x10, x9
	; v86 = iadd v85, v14
	ldr	x17, [sp, #48]
	add	x10, x10, x17
	; v87 = imul v86, v49
	mul	x10, x10, x9
	; v88 = iadd v87, v12
	ldr	x17, [sp, #40]
	add	x10, x10, x17
	; v89 = imul v88, v49
	mul	x10, x10, x9
	; v90 = iadd v89, v10
	ldr	x17, [sp, #32]
	add	x10, x10, x17
	; v91 = imul v90, v49
	mul	x10, x10, x9
	; v92 = iadd v91, v8
	ldr	x17, [sp, #24]
	add	x10, x10, x17
	; v93 = imul v92, v49
	mul	x10, x10, x9
	; v94 = iadd v93, v6
	ldr	x17, [sp, #16]
	add	x10, x10, x17
	; v95 = imul v94, v49
	mul	x10, x10, x9
	; v96 = iadd v95, v4
	ldr	x17, [sp, #8]
	add	x10, x10, x17
	; v97 = imul v96, v49
	mul	x9, x10, x9
	; v98 = iadd v97, v2
	ldr	x17, [sp, #0]
	add	x9, x9, x17
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
