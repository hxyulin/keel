	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_isort
	.p2align	2
_k_isort:
Lk_isort_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x11 v8=x12 v20=x11 v22=x14 v26=x15 v43=x13 v44=x13
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_isort_block0:
	; v2 = iconst.i64 1
	movz	x11, #0x1
	; jump block1(v2)
Lk_isort_block1:
	; v4 = icmp.ugt v1, v3
	; brif v4, block2, block3
	cmp	x10, x11
	b.hi	Lk_isort_block2
Lk_isort_block3:
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_isort_block2:
	; v5 = iconst.i64 2
	; v6 = shl v3, v5
	; v7 = ptradd v0, v6
	; v8 = load.i32 v7
	ldr	w12, [x9, x11, lsl #2]
	; v10 = iconst.i64 0
	; v11 = icmp.ne v3, v10
	; brif v11, block7(v3), block6(v3)
	cmp	x11, #0
	b.eq	Lk_isort_inst12_else
	mov	x13, x11
	b	Lk_isort_block7
Lk_isort_inst12_else:
	mov	x13, x11
	b	Lk_isort_block6
Lk_isort_block7:
	; v41 = iconst.i64 1
	; v22 = isub v43, v41
	sub	x14, x13, #1
	; v38 = iconst.i64 2
	; v24 = shl v22, v38
	; v25 = ptradd v0, v24
	; v26 = load.i32 v25
	ldr	w15, [x9, x14, lsl #2]
	; v27 = icmp.slt v8, v26
	; brif v27, block5, block6(v43)
	cmp	w12, w15
	b.ge	Lk_isort_block6
Lk_isort_block5:
	; v40 = iconst.i64 2
	; v13 = shl v43, v40
	; v14 = ptradd v0, v13
	; store v26, v14
	str	w15, [x9, x13, lsl #2]
	; v45 = iconst.i64 0
	; v46 = icmp.ne v22, v45
	; brif v46, block7(v22), block6(v22)
	cmp	x14, #0
	b.eq	Lk_isort_inst18_else
	mov	x13, x14
	b	Lk_isort_block7
Lk_isort_inst18_else:
	mov	x13, x14
Lk_isort_block6:
	; v39 = iconst.i64 2
	; v17 = shl v44, v39
	; v18 = ptradd v0, v17
	; store v8, v18
	str	w12, [x9, x13, lsl #2]
	; v42 = iconst.i64 1
	; v20 = iadd v3, v42
	add	x11, x11, #1
	; jump block1(v20)
	b	Lk_isort_block1
Lk_isort_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_matmul
	.p2align	2
_k_matmul:
Lk_matmul_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x13 v7=x14 v10=x13 v11=x15 v17=x22 v18=x22 v22=x22 v25=x23 v28=x23 v30=x20 v32=x19 v33=x20 v34=x20 v39=x15 v44=x15 v45=x19 v50=x21 v52=x19 v53=x20 v54=x21 v55=x22 v56=x23 v59=x23 v60=x23 v64=x23 v65=x24 v68=x24 v70=x20 v72=x19 v73=x22 v74=x23 v75=x23 v79=x23 v80=x24 v83=x24 v85=x21 v87=x19 v88=x22 v89=x20 v90=x20 v91=x19 v92=x20 v93=x21 v95=x19
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
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
Lk_matmul_block0:
	; v4 = iconst.i32 0
	movz	w13, #0x0
	; jump block1(v4)
Lk_matmul_block1:
	; v6 = icmp.sgt v3, v5
	; brif v6, block2, block4
	cmp	w12, w13
	b.gt	Lk_matmul_block2
Lk_matmul_block4:
	; return
	sub	sp, x29, #48
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_matmul_block2:
	; v7 = imul v3, v5
	mul	w14, w12, w13
	; v44 = iconst.i32 0
	movz	w15, #0x0
	; jump block5(v44)
Lk_matmul_block5:
	; v12 = icmp.sgt v3, v11
	; brif v12, block6, block3
	cmp	w12, w15
	b.gt	Lk_matmul_block6
Lk_matmul_block3:
	; v9 = iconst.i32 1
	; v10 = iadd v5, v9
	add	w13, w13, #1
	; jump block1(v10)
	b	Lk_matmul_block1
Lk_matmul_block6:
	; v45 = iconst.i32 0
	movz	w19, #0x0
	; v90 = iconst.i32 0
	movz	w20, #0x0
	; jump block10(v45, v45, v90, v11)
	mov	x21, x20
	mov	x20, x19
	mov	x22, x15
Lk_matmul_block10:
	; v56 = isub v3, v52
	sub	w23, w12, w19
	; v57 = iconst.i32 2
	; v58 = icmp.sge v56, v57
	; brif v58, block11, block12
	cmp	w23, #2
	b.ge	Lk_matmul_block11
Lk_matmul_block12:
	; v89 = iadd v53, v54
	add	w20, w20, w21
	; v16 = icmp.sgt v3, v52
	; brif v16, block8(v52, v89, v55), block9(v89)
	cmp	w12, w19
	b.le	Lk_matmul_inst16_else
	mov	x21, x22
	b	Lk_matmul_block8
Lk_matmul_inst16_else:
	mov	x19, x20
	b	Lk_matmul_block9
Lk_matmul_block8:
	; v17 = iadd v7, v91
	add	w22, w14, w19
	; v18 = sext.i64 v17
	sxtw	x22, w22
	; v19 = iconst.i64 2
	; v20 = shl v18, v19
	; v21 = ptradd v0, v20
	; v22 = load.i32 v21
	ldr	w22, [x9, x22, lsl #2]
	; v25 = sext.i64 v93
	sxtw	x23, w21
	; v26 = shl v25, v19
	; v27 = ptradd v1, v26
	; v28 = load.i32 v27
	ldr	w23, [x10, x23, lsl #2]
	; v29 = imul v22, v28
	; v30 = iadd v92, v29
	madd	w20, w22, w23, w20
	; v31 = iconst.i32 1
	; v32 = iadd v91, v31
	add	w19, w19, #1
	; v50 = iadd v3, v93
	add	w21, w12, w21
	; v97 = icmp.sgt v3, v32
	; brif v97, block8(v32, v30, v50), block9(v30)
	cmp	w12, w19
	b.gt	Lk_matmul_block8
	mov	x19, x20
Lk_matmul_block9:
	; v33 = iadd v7, v11
	add	w20, w14, w15
	; v34 = sext.i64 v33
	sxtw	x20, w20
	; v35 = iconst.i64 2
	; v36 = shl v34, v35
	; v37 = ptradd v2, v36
	; store v95, v37
	str	w19, [x11, x20, lsl #2]
	; v38 = iconst.i32 1
	; v39 = iadd v11, v38
	add	w15, w15, #1
	; jump block5(v39)
	b	Lk_matmul_block5
Lk_matmul_block11:
	; v59 = iadd v7, v52
	add	w23, w14, w19
	; v60 = sext.i64 v59
	sxtw	x23, w23
	; v61 = iconst.i64 2
	; v62 = shl v60, v61
	; v63 = ptradd v0, v62
	; v64 = load.i32 v63
	ldr	w23, [x9, x23, lsl #2]
	; v65 = sext.i64 v55
	sxtw	x24, w22
	; v66 = shl v65, v61
	; v67 = ptradd v1, v66
	; v68 = load.i32 v67
	ldr	w24, [x10, x24, lsl #2]
	; v69 = imul v64, v68
	; v70 = iadd v53, v69
	madd	w20, w23, w24, w20
	; v71 = iconst.i32 1
	; v72 = iadd v52, v71
	add	w19, w19, #1
	; v73 = iadd v3, v55
	add	w22, w12, w22
	; v74 = iadd v7, v72
	add	w23, w14, w19
	; v75 = sext.i64 v74
	sxtw	x23, w23
	; v76 = iconst.i64 2
	; v77 = shl v75, v76
	; v78 = ptradd v0, v77
	; v79 = load.i32 v78
	ldr	w23, [x9, x23, lsl #2]
	; v80 = sext.i64 v73
	sxtw	x24, w22
	; v81 = shl v80, v76
	; v82 = ptradd v1, v81
	; v83 = load.i32 v82
	ldr	w24, [x10, x24, lsl #2]
	; v84 = imul v79, v83
	; v85 = iadd v54, v84
	madd	w21, w23, w24, w21
	; v86 = iconst.i32 1
	; v87 = iadd v72, v86
	add	w19, w19, #1
	; v88 = iadd v3, v73
	add	w22, w12, w22
	; jump block10(v87, v70, v85, v88)
	b	Lk_matmul_block10
Lk_matmul_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_signed_dot32
	.p2align	2
_signed_dot32:
Lsigned_dot32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v8=x14 v10=x14 v12=x15 v14=x14 v16=x13 v18=x12 v19=x12 v20=x13 v21=x14 v22=x15 v25=x15 v27=x15 v29=x19 v31=x15 v33=x13 v35=x12 v36=x15 v38=x15 v40=x19 v42=x15 v44=x14 v46=x12 v47=x13 v48=x14 v49=x12 v50=x13 v52=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
Lsigned_dot32_block0:
	; v4 = iconst.i32 0
	movz	w13, #0x0
	; v48 = iconst.i32 0
	movz	w14, #0x0
	; jump block4(v4, v3, v48)
	mov	x16, x12
	mov	x12, x13
	mov	x13, x16
Lsigned_dot32_block4:
	; v22 = isub v2, v19
	sub	w15, w11, w12
	; v23 = iconst.i32 2
	; v24 = icmp.sge v22, v23
	; brif v24, block5, block6
	cmp	w15, #2
	b.ge	Lsigned_dot32_block5
Lsigned_dot32_block6:
	; v47 = iadd v20, v21
	add	w13, w13, w14
	; v7 = icmp.sgt v2, v19
	; brif v7, block2(v19, v47), block3(v47)
	cmp	w11, w12
	b.gt	Lsigned_dot32_block2
	mov	x9, x13
	b	Lsigned_dot32_block3
Lsigned_dot32_block2:
	; v8 = sext.i64 v49
	sxtw	x14, w12
	; v9 = iconst.i64 2
	; v10 = shl v8, v9
	lsl	x14, x14, #2
	; v11 = ptradd v0, v10
	; v12 = load.i32 v11
	ldr	w15, [x9, x14, lsl #0]
	; v13 = ptradd v1, v10
	; v14 = load.i32 v13
	ldr	w14, [x10, x14, lsl #0]
	; v15 = imul v12, v14
	; v16 = iadd v50, v15
	madd	w13, w15, w14, w13
	; v17 = iconst.i32 1
	; v18 = iadd v49, v17
	add	w12, w12, #1
	; v53 = icmp.sgt v2, v18
	; brif v53, block2(v18, v16), block3(v16)
	cmp	w11, w12
	b.gt	Lsigned_dot32_block2
	mov	x9, x13
Lsigned_dot32_block3:
	; return v52
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lsigned_dot32_block5:
	; v25 = sext.i64 v19
	sxtw	x15, w12
	; v26 = iconst.i64 2
	; v27 = shl v25, v26
	lsl	x15, x15, #2
	; v28 = ptradd v0, v27
	; v29 = load.i32 v28
	ldr	w19, [x9, x15, lsl #0]
	; v30 = ptradd v1, v27
	; v31 = load.i32 v30
	ldr	w15, [x10, x15, lsl #0]
	; v32 = imul v29, v31
	; v33 = iadd v20, v32
	madd	w13, w19, w15, w13
	; v34 = iconst.i32 1
	; v35 = iadd v19, v34
	add	w12, w12, #1
	; v36 = sext.i64 v35
	sxtw	x15, w12
	; v37 = iconst.i64 2
	; v38 = shl v36, v37
	lsl	x15, x15, #2
	; v39 = ptradd v0, v38
	; v40 = load.i32 v39
	ldr	w19, [x9, x15, lsl #0]
	; v41 = ptradd v1, v38
	; v42 = load.i32 v41
	ldr	w15, [x10, x15, lsl #0]
	; v43 = imul v40, v42
	; v44 = iadd v21, v43
	madd	w14, w19, w15, w14
	; v45 = iconst.i32 1
	; v46 = iadd v35, v45
	add	w12, w12, #1
	; jump block4(v46, v33, v44)
	b	Lsigned_dot32_block4
Lsigned_dot32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_signed_dot64
	.p2align	2
_signed_dot64:
Lsigned_dot64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v9=x14 v11=x15 v13=x14 v15=x13 v17=x12 v18=x12 v19=x13 v20=x14 v21=x15 v25=x15 v27=x19 v29=x15 v31=x13 v33=x12 v35=x15 v37=x19 v39=x15 v41=x14 v43=x12 v44=x13 v45=x14 v46=x12 v47=x13 v49=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
Lsigned_dot64_block0:
	; v4 = iconst.i64 0
	movz	x13, #0x0
	; v45 = iconst.i64 0
	movz	x14, #0x0
	; jump block4(v4, v3, v45)
	mov	x16, x12
	mov	x12, x13
	mov	x13, x16
Lsigned_dot64_block4:
	; v21 = isub v2, v18
	sub	x15, x11, x12
	; v22 = iconst.i64 2
	; v23 = icmp.sge v21, v22
	; brif v23, block5, block6
	cmp	x15, #2
	b.ge	Lsigned_dot64_block5
Lsigned_dot64_block6:
	; v44 = iadd v19, v20
	add	x13, x13, x14
	; v7 = icmp.sgt v2, v18
	; brif v7, block2(v18, v44), block3(v44)
	cmp	x11, x12
	b.gt	Lsigned_dot64_block2
	mov	x9, x13
	b	Lsigned_dot64_block3
Lsigned_dot64_block2:
	; v8 = iconst.i64 3
	; v9 = shl v46, v8
	lsl	x14, x12, #3
	; v10 = ptradd v0, v9
	; v11 = load.i64 v10
	ldr	x15, [x9, x14, lsl #0]
	; v12 = ptradd v1, v9
	; v13 = load.i64 v12
	ldr	x14, [x10, x14, lsl #0]
	; v14 = imul v11, v13
	; v15 = iadd v47, v14
	madd	x13, x15, x14, x13
	; v16 = iconst.i64 1
	; v17 = iadd v46, v16
	add	x12, x12, #1
	; v50 = icmp.sgt v2, v17
	; brif v50, block2(v17, v15), block3(v15)
	cmp	x11, x12
	b.gt	Lsigned_dot64_block2
	mov	x9, x13
Lsigned_dot64_block3:
	; return v49
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lsigned_dot64_block5:
	; v24 = iconst.i64 3
	; v25 = shl v18, v24
	lsl	x15, x12, #3
	; v26 = ptradd v0, v25
	; v27 = load.i64 v26
	ldr	x19, [x9, x15, lsl #0]
	; v28 = ptradd v1, v25
	; v29 = load.i64 v28
	ldr	x15, [x10, x15, lsl #0]
	; v30 = imul v27, v29
	; v31 = iadd v19, v30
	madd	x13, x19, x15, x13
	; v32 = iconst.i64 1
	; v33 = iadd v18, v32
	add	x12, x12, #1
	; v34 = iconst.i64 3
	; v35 = shl v33, v34
	lsl	x15, x12, #3
	; v36 = ptradd v0, v35
	; v37 = load.i64 v36
	ldr	x19, [x9, x15, lsl #0]
	; v38 = ptradd v1, v35
	; v39 = load.i64 v38
	ldr	x15, [x10, x15, lsl #0]
	; v40 = imul v37, v39
	; v41 = iadd v20, v40
	madd	x14, x19, x15, x14
	; v42 = iconst.i64 1
	; v43 = iadd v33, v42
	add	x12, x12, #1
	; jump block4(v43, v31, v41)
	b	Lsigned_dot64_block4
Lsigned_dot64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_div_loop
	.p2align	2
_div_loop:
Ldiv_loop_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v8=x15 v9=x15 v10=x19 v11=x15 v12=x14 v14=x13 v15=x13 v16=x14 v18=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
Ldiv_loop_block0:
	; v4 = iconst.i64 0
	movz	x13, #0x0
	; v7 = icmp.sgt v2, v4
	; brif v7, block2(v4, v4), block3(v4)
	cmp	x11, #0
	b.le	Ldiv_loop_inst3_else
	mov	x14, x13
	b	Ldiv_loop_block2
Ldiv_loop_inst3_else:
	mov	x9, x13
	b	Ldiv_loop_block3
Ldiv_loop_block2:
	; v8 = load.i64 v0
	ldr	x15, [x9]
	; v9 = sdiv v8, v3
	cbz	x12, Ldiv_loop_trap
	movz	x8, #0x8000, lsl #48
	cmn	x12, #1
	ccmp	x15, x8, #0, eq
	b.eq	Ldiv_loop_trap
	sdiv	x15, x15, x12
	; v10 = load.i64 v1
	ldr	x19, [x10]
	; v11 = iadd v9, v10
	add	x15, x15, x19
	; v12 = iadd v16, v11
	add	x14, x14, x15
	; v13 = iconst.i64 1
	; v14 = iadd v15, v13
	add	x13, x13, #1
	; v19 = icmp.sgt v2, v14
	; brif v19, block2(v14, v12), block3(v12)
	cmp	x11, x13
	b.gt	Ldiv_loop_block2
	mov	x9, x14
Ldiv_loop_block3:
	; return v18
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Ldiv_loop_trap:
	brk	#1
Ldiv_loop_end:
	.cfi_endproc
	.subsections_via_symbols
