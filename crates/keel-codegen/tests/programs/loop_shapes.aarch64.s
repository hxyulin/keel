	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_isort
	.p2align	2
_k_isort:
Lk_isort_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x11 v5=x12 v6=x12 v7=x12 v8=x12 v9=x13 v10=x14 v12=x19 v13=x13 v14=x13 v15=x15 v16=x14 v17=x13 v18=x13 v19=x12 v20=x11 v21=x14 v22=x14 v23=x15 v24=x15 v25=x15 v26=x19
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
Lk_isort_block0:
	; v2 = iconst.i64 1
	movz	x11, #0x1
	; jump block1(v2)
Lk_isort_block1:
	; v4 = icmp.ugt v1, v3
	; brif v4, block2, block3
	cmp	x10, x11
	b.ls	Lk_isort_block3
Lk_isort_block2:
	; v5 = iconst.i64 2
	; v6 = shl v3, v5
	lsl	x12, x11, #2
	; v7 = ptradd v0, v6
	add	x12, x9, x12
	; v8 = load.i32 v7
	ldr	w12, [x12]
	; jump block4(v3)
	mov	x13, x11
Lk_isort_block4:
	; v10 = iconst.i64 0
	; v11 = icmp.ne v9, v10
	; brif v11, block7, block6
	cmp	x13, #0
	b.eq	Lk_isort_block6
Lk_isort_block7:
	; v21 = iconst.i64 1
	; v22 = isub v9, v21
	sub	x14, x13, #1
	; v23 = iconst.i64 2
	; v24 = shl v22, v23
	lsl	x15, x14, #2
	; v25 = ptradd v0, v24
	add	x15, x9, x15
	; v26 = load.i32 v25
	ldr	w19, [x15]
	; v27 = icmp.slt v8, v26
	; brif v27, block5, block6
	cmp	w12, w19
	b.ge	Lk_isort_block6
Lk_isort_block5:
	; v12 = iconst.i64 2
	; v13 = shl v9, v12
	lsl	x13, x13, #2
	; v14 = ptradd v0, v13
	add	x13, x9, x13
	; v15 = load.i32 v25
	ldr	w15, [x15]
	; store v15, v14
	str	w15, [x13]
	; jump block4(v22)
	mov	x13, x14
	b	Lk_isort_block4
Lk_isort_block6:
	; v16 = iconst.i64 2
	; v17 = shl v9, v16
	lsl	x13, x13, #2
	; v18 = ptradd v0, v17
	add	x13, x9, x13
	; store v8, v18
	str	w12, [x13]
	; v19 = iconst.i64 1
	; v20 = iadd v3, v19
	add	x11, x11, #1
	; jump block1(v20)
	b	Lk_isort_block1
Lk_isort_block3:
	; return
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_isort_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_matmul
	.p2align	2
_k_matmul:
Lk_matmul_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x13 v7=x14 v8=x15 v9=x14 v10=x13 v11=x15 v13=x19 v14=x19 v15=x20 v17=x21 v19=x22 v21=x21 v22=x21 v23=x22 v24=x22 v27=x22 v28=x22 v29=x21 v30=x20 v31=x21 v32=x19 v33=x19 v35=x21 v37=x19 v38=x19 v39=x15
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
	b.le	Lk_matmul_block4
Lk_matmul_block2:
	; v7 = imul v3, v5
	mul	w14, w12, w13
	; v8 = iconst.i32 0
	movz	w15, #0x0
	; jump block5(v8)
Lk_matmul_block5:
	; v12 = icmp.sgt v3, v11
	; brif v12, block6, block3
	cmp	w12, w15
	b.le	Lk_matmul_block3
Lk_matmul_block6:
	; v13 = iconst.i32 0
	movz	w19, #0x0
	; jump block7(v13, v13)
	mov	x20, x19
Lk_matmul_block7:
	; v16 = icmp.sgt v3, v14
	; brif v16, block8, block9
	cmp	w12, w19
	b.le	Lk_matmul_block9
Lk_matmul_block8:
	; v17 = iadd v14, v7
	add	w21, w19, w14
	; v18 = sext.i64 v17
	; v19 = iconst.i64 2
	; v20 = shl v18, v19
	; v21 = ptradd v0, v20
	add	x21, x9, w21, sxtw #2
	; v22 = load.i32 v21
	ldr	w21, [x21]
	; v23 = imul v3, v14
	mul	w22, w12, w19
	; v24 = iadd v11, v23
	add	w22, w15, w22
	; v25 = sext.i64 v24
	; v26 = shl v25, v19
	; v27 = ptradd v1, v26
	add	x22, x10, w22, sxtw #2
	; v28 = load.i32 v27
	ldr	w22, [x22]
	; v29 = imul v22, v28
	mul	w21, w21, w22
	; v30 = iadd v15, v29
	add	w20, w20, w21
	; v31 = iconst.i32 1
	; v32 = iadd v14, v31
	add	w19, w19, #1
	; jump block7(v32, v30)
	b	Lk_matmul_block7
Lk_matmul_block9:
	; v33 = iadd v11, v7
	add	w19, w15, w14
	; v34 = sext.i64 v33
	; v35 = iconst.i64 2
	; v36 = shl v34, v35
	; v37 = ptradd v2, v36
	add	x19, x11, w19, sxtw #2
	; store v15, v37
	str	w20, [x19]
	; v38 = iconst.i32 1
	; v39 = iadd v11, v38
	add	w15, w15, #1
	; jump block5(v39)
	b	Lk_matmul_block5
Lk_matmul_block3:
	; v9 = iconst.i32 1
	; v10 = iadd v5, v9
	add	w13, w13, #1
	; jump block1(v10)
	b	Lk_matmul_block1
Lk_matmul_block4:
	; return
	sub	sp, x29, #32
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_matmul_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_signed_dot32
	.p2align	2
_signed_dot32:
Lsigned_dot32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x12 v6=x13 v8=x14 v9=x15 v10=x14 v11=x15 v12=x15 v13=x14 v14=x14 v15=x14 v16=x13 v17=x14 v18=x12
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
Lsigned_dot32_block0:
	; v4 = iconst.i32 0
	movz	w13, #0x0
	; jump block1(v4, v3)
	mov	x16, x12
	mov	x12, x13
	mov	x13, x16
Lsigned_dot32_block1:
	; v7 = icmp.slt v5, v2
	; brif v7, block2, block3
	cmp	w12, w11
	b.ge	Lsigned_dot32_block3
Lsigned_dot32_block2:
	; v8 = sext.i64 v5
	sxtw	x14, w12
	; v9 = iconst.i64 2
	; v10 = shl v8, v9
	lsl	x14, x14, #2
	; v11 = ptradd v0, v10
	add	x15, x9, x14
	; v12 = load.i32 v11
	ldr	w15, [x15]
	; v13 = ptradd v1, v10
	add	x14, x10, x14
	; v14 = load.i32 v13
	ldr	w14, [x14]
	; v15 = imul v12, v14
	mul	w14, w15, w14
	; v16 = iadd v6, v15
	add	w13, w13, w14
	; v17 = iconst.i32 1
	; v18 = iadd v5, v17
	add	w12, w12, #1
	; jump block1(v18, v16)
	b	Lsigned_dot32_block1
Lsigned_dot32_block3:
	; return v6
	mov	x0, x13
	ldp	x29, x30, [sp], #16
	ret
Lsigned_dot32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_signed_dot64
	.p2align	2
_signed_dot64:
Lsigned_dot64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x12 v6=x13 v8=x14 v9=x14 v10=x15 v11=x15 v12=x14 v13=x14 v14=x14 v15=x13 v16=x14 v17=x12
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
Lsigned_dot64_block0:
	; v4 = iconst.i64 0
	movz	x13, #0x0
	; jump block1(v4, v3)
	mov	x16, x12
	mov	x12, x13
	mov	x13, x16
Lsigned_dot64_block1:
	; v7 = icmp.slt v5, v2
	; brif v7, block2, block3
	cmp	x12, x11
	b.ge	Lsigned_dot64_block3
Lsigned_dot64_block2:
	; v8 = iconst.i64 3
	; v9 = shl v5, v8
	lsl	x14, x12, #3
	; v10 = ptradd v0, v9
	add	x15, x9, x14
	; v11 = load.i64 v10
	ldr	x15, [x15]
	; v12 = ptradd v1, v9
	add	x14, x10, x14
	; v13 = load.i64 v12
	ldr	x14, [x14]
	; v14 = imul v11, v13
	mul	x14, x15, x14
	; v15 = iadd v6, v14
	add	x13, x13, x14
	; v16 = iconst.i64 1
	; v17 = iadd v5, v16
	add	x12, x12, #1
	; jump block1(v17, v15)
	b	Lsigned_dot64_block1
Lsigned_dot64_block3:
	; return v6
	mov	x0, x13
	ldp	x29, x30, [sp], #16
	ret
Lsigned_dot64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_div_loop
	.p2align	2
_div_loop:
Ldiv_loop_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x13 v6=x14 v8=x15 v9=x15 v10=x19 v11=x15 v12=x14 v13=x15 v14=x13
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
	; jump block1(v4, v4)
	mov	x14, x13
Ldiv_loop_block1:
	; v7 = icmp.slt v5, v2
	; brif v7, block2, block3
	cmp	x13, x11
	b.ge	Ldiv_loop_block3
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
	; v12 = iadd v6, v11
	add	x14, x14, x15
	; v13 = iconst.i64 1
	; v14 = iadd v5, v13
	add	x13, x13, #1
	; jump block1(v14, v12)
	b	Ldiv_loop_block1
Ldiv_loop_block3:
	; return v6
	mov	x0, x14
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Ldiv_loop_trap:
	brk	#1
Ldiv_loop_end:
	.cfi_endproc
	.subsections_via_symbols
