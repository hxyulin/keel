	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_private_acc
	.p2align	2
_private_acc:
Lprivate_acc_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x13 v6=x14 v7=x14 v8=x13 v9=x11 v10=x11 v11=x11 v13=x12 v14=x12 v15=x12 v16=x12 v17=x13 v19=x9 v20=x10 v21=x11 v22=x10 v23=x11 v24=x11 v25=x11 v26=x10 v27=x11 v28=x9 v29=x9 v30=x9 v31=x9 v32=x13 v33=x14 v34=x13 v35=x14 v36=x12 v37=x13 v38=x14 v39=x12 v40=x12 v41=x13 v42=x12 v43=x13 v44=x14 v45=x13 v46=x12 v47=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #32
	mov	x9, x0
	mov	x10, x1
Lprivate_acc_block0:
	; v2 = stack_addr ss0
	add	x11, sp, #0
	; v3 = iconst.i32 0
	; v4 = iconst.i64 24
	; v5 = iconst.i64 0
	; store v5, v2
	str	xzr, [sp, #0]
	; v6 = iconst.i64 8
	; v7 = ptradd v2, v6
	add	x14, x11, #8
	; store v5, v7
	str	xzr, [x14]
	; v8 = iconst.i64 16
	; v9 = ptradd v2, v8
	add	x11, x11, #16
	; store v3, v9
	str	wzr, [x11]
	; v10 = iconst.i64 0
	movz	x11, #0x0
	; jump block1(v10)
Lprivate_acc_block1:
	; v12 = icmp.ugt v1, v11
	; brif v12, block2, block3
	cmp	x10, x11
	b.ls	Lprivate_acc_block3
Lprivate_acc_block2:
	; v13 = iconst.i64 3
	; v14 = shl v11, v13
	lsl	x12, x11, #3
	; v15 = ptradd v0, v14
	add	x12, x9, x12
	; v16 = load.i64 v15
	ldr	x12, [x12]
	; v17 = iconst.i64 0
	; v18 = icmp.slt v16, v17
	; brif v18, block5, block4
	cmp	x12, #0
	b.lt	Lprivate_acc_block5
Lprivate_acc_block4:
	; v32 = iconst.i64 8
	; v33 = stack_addr ss0
	add	x14, sp, #0
	; v34 = ptradd v33, v32
	add	x13, x14, #8
	; v35 = load.i64 v34
	ldr	x14, [x13]
	; v36 = iadd v16, v35
	add	x12, x12, x14
	; store v36, v34
	str	x12, [x13]
	; jump block6
	b	Lprivate_acc_block6
Lprivate_acc_block5:
	; v37 = stack_addr ss0
	; v38 = load.i64 v37
	ldr	x14, [sp, #0]
	; v39 = iadd v16, v38
	add	x12, x12, x14
	; store v39, v37
	str	x12, [sp, #0]
	; jump block6
Lprivate_acc_block6:
	; v40 = iconst.i64 16
	; v41 = stack_addr ss0
	add	x13, sp, #0
	; v42 = ptradd v41, v40
	add	x12, x13, #16
	; v43 = load.i32 v42
	ldr	w13, [x12]
	; v44 = iconst.i32 1
	; v45 = iadd v43, v44
	add	w13, w13, #1
	; store v45, v42
	str	w13, [x12]
	; v46 = iconst.i64 1
	; v47 = iadd v11, v46
	add	x11, x11, #1
	; jump block1(v47)
	b	Lprivate_acc_block1
Lprivate_acc_block3:
	; v19 = stack_addr ss0
	add	x9, sp, #0
	; v20 = load.i64 align 8 v19
	ldr	x10, [sp, #0]
	; v21 = iconst.i64 3
	movz	x11, #0x3
	; v22 = imul v20, v21
	mul	x10, x10, x11
	; v23 = iconst.i64 8
	; v24 = ptradd v19, v23
	add	x11, x9, #8
	; v25 = load.i64 align 8 v24
	ldr	x11, [x11]
	; v26 = iadd v22, v25
	add	x10, x10, x11
	; v27 = iconst.i64 16
	; v28 = ptradd v19, v27
	add	x9, x9, #16
	; v29 = load.i32 align 8 v28
	ldr	w9, [x9]
	; v30 = uext.i64 v29
	mov	w9, w9
	; v31 = iadd v26, v30
	add	x9, x10, x9
	; return v31
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lprivate_acc_end:
	.cfi_endproc
	.subsections_via_symbols
