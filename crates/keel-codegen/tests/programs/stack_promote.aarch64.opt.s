	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_private_acc
	.p2align	2
_private_acc:
Lprivate_acc_begin:
	; regalloc: v0=x9 v1=x10 v3=x11 v5=x12 v16=x15 v21=x10 v26=x9 v30=x10 v31=x9 v36=x13 v39=x11 v45=x14 v47=x12 v62=x11 v68=x13 v75=x12 v76=x11 v77=x13 v78=x14 v80=x12 v81=x9 v82=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lprivate_acc_block0:
	; v3 = iconst.i32 0
	movz	w11, #0x0
	; v5 = iconst.i64 0
	movz	x12, #0x0
	; v12 = icmp.ugt v1, v5
	; brif v12, block2(v5, v5, v5, v3), block3(v5, v5, v3)
	cmp	x10, #0
	b.ls	Lprivate_acc_inst14_else
	mov	x13, x12
	mov	x14, x11
	mov	x11, x12
	b	Lprivate_acc_block2
Lprivate_acc_inst14_else:
	mov	x9, x12
	b	Lprivate_acc_block3
Lprivate_acc_block2:
	; v13 = iconst.i64 3
	; v14 = shl v75, v13
	; v15 = ptradd v0, v14
	; v16 = load.i64 v15
	ldr	x15, [x9, x12, lsl #3]
	; v54 = iconst.i64 0
	; v18 = icmp.slt v16, v54
	; brif v18, block5, block4
	cmp	x15, #0
	b.lt	Lprivate_acc_block5
Lprivate_acc_block4:
	; v36 = iadd v16, v77
	add	x13, x15, x13
	; jump block6(v76, v36)
	b	Lprivate_acc_block6
Lprivate_acc_block5:
	; v39 = iadd v16, v76
	add	x11, x15, x11
	; jump block6(v39, v77)
Lprivate_acc_block6:
	; v44 = iconst.i32 1
	; v45 = iadd v78, v44
	add	w14, w14, #1
	; v46 = iconst.i64 1
	; v47 = iadd v75, v46
	add	x12, x12, #1
	; v83 = icmp.ugt v1, v47
	; brif v83, block2(v47, v62, v68, v45), block3(v62, v68, v45)
	cmp	x10, x12
	b.hi	Lprivate_acc_block2
	mov	x12, x11
	mov	x9, x13
	mov	x11, x14
Lprivate_acc_block3:
	; v21 = iconst.i64 3
	movz	x10, #0x3
	; v22 = imul v80, v21
	; v26 = iadd v22, v81
	madd	x9, x12, x10, x9
	; v30 = uext.i64 v82
	mov	w10, w11
	; v31 = iadd v26, v30
	add	x9, x9, x10
	; return v31
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lprivate_acc_end:
	.cfi_endproc
	.subsections_via_symbols
