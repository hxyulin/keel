	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_dense
	.p2align	2
_dense:
Ldense_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x9 v4=x9 v6=x9 v7=x9 v8=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Ldense_block0:
	; v1 = iconst.i32 100
	movz	w10, #0x64
	; switch v0, block5, [0: block1, 1: block2, 2: block1, 4: block3(v0), 5: block4, 7: block2, 8: block3(v1)]
	cmp	w9, #8
	b.hi	Ldense_block5
	adr	x17, Ldense_inst1_table
	ldrsw	x16, [x17, w9, uxtw #2]
	add	x17, x17, x16
	br	x17
Ldense_inst1_table:
	.long	Ldense_block1-Ldense_inst1_table
	.long	Ldense_block2-Ldense_inst1_table
	.long	Ldense_block1-Ldense_inst1_table
	.long	Ldense_block5-Ldense_inst1_table
	.long	Ldense_block3-Ldense_inst1_table
	.long	Ldense_block4-Ldense_inst1_table
	.long	Ldense_block5-Ldense_inst1_table
	.long	Ldense_block2-Ldense_inst1_table
	.long	Ldense_inst1_edge7-Ldense_inst1_table
Ldense_inst1_edge7:
	mov	x9, x10
	b	Ldense_block3
Ldense_block4:
	; v7 = iconst.i32 50
	movz	w9, #0x32
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ldense_block3:
	; v5 = iconst.i32 1000
	; v6 = iadd v4, v5
	add	w9, w9, #1000
	; return v6
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ldense_block2:
	; v3 = iconst.i32 20
	movz	w9, #0x14
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ldense_block1:
	; v2 = iconst.i32 10
	movz	w9, #0xa
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ldense_block5:
	; v8 = iconst.i32 -1
	movn	w9, #0x0
	; return v8
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ldense_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_dense_off
	.p2align	2
_dense_off:
Ldense_off_begin:
	; regalloc: v0=x9 v1=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Ldense_off_block0:
	; switch v0, block2, [1000: block1, 1001: block2, 1002: block1, 1003: block1, 1005: block2]
	sub	x16, x9, #1000
	cmp	x16, #5
	b.hi	Ldense_off_block2
	adr	x17, Ldense_off_inst0_table
	ldrsw	x16, [x17, x16, lsl #2]
	add	x17, x17, x16
	br	x17
Ldense_off_inst0_table:
	.long	Ldense_off_block1-Ldense_off_inst0_table
	.long	Ldense_off_block2-Ldense_off_inst0_table
	.long	Ldense_off_block1-Ldense_off_inst0_table
	.long	Ldense_off_block1-Ldense_off_inst0_table
	.long	Ldense_off_block2-Ldense_off_inst0_table
	.long	Ldense_off_block2-Ldense_off_inst0_table
Ldense_off_block1:
	; v1 = iconst.i64 7
	movz	x9, #0x7
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ldense_off_block2:
	; v5 = iconst.i64 1
	; v4 = shl v0, v5
	lsl	x9, x9, #1
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ldense_off_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_dense_big
	.p2align	2
_dense_big:
Ldense_big_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Ldense_big_block0:
	; v1 = iconst.i32 1
	movz	w10, #0x1
	; v2 = iconst.i32 2
	movz	w11, #0x2
	; v3 = iconst.i32 3
	movz	w12, #0x3
	; switch v0, block2(v0), [70000: block2(v1), 70001: block2(v2), 70002: block2(v3), 70003: block1, 70004: block2(v1)]
	movz	w17, #0x1170
	movk	w17, #0x1, lsl #16
	sub	w16, w9, w17
	cmp	w16, #4
	b.hi	Ldense_big_block2
	adr	x17, Ldense_big_inst3_table
	ldrsw	x16, [x17, w16, uxtw #2]
	add	x17, x17, x16
	br	x17
Ldense_big_inst3_table:
	.long	Ldense_big_inst3_edge1-Ldense_big_inst3_table
	.long	Ldense_big_inst3_edge2-Ldense_big_inst3_table
	.long	Ldense_big_inst3_edge3-Ldense_big_inst3_table
	.long	Ldense_big_block1-Ldense_big_inst3_table
	.long	Ldense_big_inst3_edge5-Ldense_big_inst3_table
Ldense_big_inst3_edge1:
	mov	x9, x10
	b	Ldense_big_block2
Ldense_big_inst3_edge2:
	mov	x9, x11
	b	Ldense_big_block2
Ldense_big_inst3_edge3:
	mov	x9, x12
	b	Ldense_big_block2
Ldense_big_inst3_edge5:
	mov	x9, x10
	b	Ldense_big_block2
Ldense_big_block1:
	; v4 = iconst.i32 99
	movz	w9, #0x63
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ldense_big_block2:
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ldense_big_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_sparse
	.p2align	2
_sparse:
Lsparse_begin:
	; regalloc: v0=x9 v1=x9 v2=x9 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lsparse_block0:
	; switch v0, block4, [-1: block1, 4886718345: block2, 5000: block3, 3: block1]
	movn	x17, #0x0
	cmp	x9, x17
	b.eq	Lsparse_block1
	movz	x17, #0x6789
	movk	x17, #0x2345, lsl #16
	movk	x17, #0x1, lsl #32
	cmp	x9, x17
	b.eq	Lsparse_block2
	movz	x17, #0x1388
	cmp	x9, x17
	b.eq	Lsparse_block3
	cmp	x9, #3
	b.eq	Lsparse_block1
	b	Lsparse_block4
Lsparse_block3:
	; v3 = iconst.i64 33
	movz	x9, #0x21
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsparse_block2:
	; v2 = iconst.i64 22
	movz	x9, #0x16
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsparse_block1:
	; v1 = iconst.i64 11
	movz	x9, #0xb
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsparse_block4:
	; v4 = iconst.i64 44
	movz	x9, #0x2c
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsparse_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_narrow
	.p2align	2
_narrow:
Lnarrow_begin:
	; regalloc: v0=x9 v1=x9 v2=x9 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lnarrow_block0:
	; switch v0, block3, [-128: block1, 255: block2, 0: block1, 127: block2]
	uxtb	w16, w9
	cmp	w16, #128
	b.eq	Lnarrow_block1
	cmp	w16, #255
	b.eq	Lnarrow_block2
	cmp	w16, #0
	b.eq	Lnarrow_block1
	cmp	w16, #127
	b.eq	Lnarrow_block2
	b	Lnarrow_block3
Lnarrow_block2:
	; v2 = iconst.i32 2
	movz	w9, #0x2
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lnarrow_block1:
	; v1 = iconst.i32 1
	movz	w9, #0x1
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lnarrow_block3:
	; v3 = iconst.i32 3
	movz	w9, #0x3
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lnarrow_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_machine
	.p2align	2
_machine:
Lmachine_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v3=x11 v4=x12 v7=x10 v10=x12 v13=x11 v14=x12 v15=x11 v18=x12 v22=x9 v23=x12 v28=x11 v29=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lmachine_block0:
	; v1 = iconst.i64 0
	movz	x10, #0x0
	; jump block1(v1, v1, v1)
	mov	x11, x10
	mov	x12, x10
Lmachine_block1:
	; v5 = icmp.ugt v0, v2
	; brif v5, block2, block7(v4)
	cmp	x9, x10
	b.ls	Lmachine_block7
Lmachine_block2:
	; v6 = iconst.i64 1
	; v7 = iadd v2, v6
	add	x10, x10, #1
	; switch v3, block6, [0: block3, 1: block4, 2: block5]
	cmp	x11, #0
	b.eq	Lmachine_block3
	cmp	x11, #1
	b.eq	Lmachine_block4
	cmp	x11, #2
	b.eq	Lmachine_block5
	b	Lmachine_block6
Lmachine_block5:
	; v18 = isub v4, v7
	sub	x12, x12, x10
	; v29 = iconst.i64 0
	movz	x11, #0x0
	; jump block1(v7, v29, v18)
	b	Lmachine_block1
Lmachine_block4:
	; v13 = iconst.i64 3
	movz	x11, #0x3
	; v14 = imul v4, v13
	mul	x12, x12, x11
	; v15 = iconst.i64 2
	movz	x11, #0x2
	; jump block1(v7, v15, v14)
	b	Lmachine_block1
Lmachine_block3:
	; v28 = iconst.i64 1
	movz	x11, #0x1
	; v10 = iadd v4, v28
	add	x12, x12, #1
	; jump block1(v7, v28, v10)
	b	Lmachine_block1
Lmachine_block6:
	; v22 = iconst.i64 -1
	movn	x9, #0x0
	; jump block7(v22)
	mov	x12, x9
Lmachine_block7:
	; return v23
	mov	x0, x12
	ldp	x29, x30, [sp], #16
	ret
Lmachine_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_constant
	.p2align	2
_constant:
Lconstant_begin:
	; regalloc: v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lconstant_block0:
	; v2 = iconst.i32 1
	movz	w9, #0x1
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lconstant_end:
	.cfi_endproc
	.subsections_via_symbols
