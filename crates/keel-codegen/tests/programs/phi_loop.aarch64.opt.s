	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_phi_loop
	.p2align	2
_phi_loop:
Lphi_loop_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v5=x11 v7=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lphi_loop_block0:
	; v2 = iconst.i64 0
	movz	x11, #0x0
	; jump block1(v2)
Lphi_loop_block1:
	; v6 = icmp.ugt v1, v5
	; brif v6, block2, block3
	cmp	x10, x11
	b.ls	Lphi_loop_block3
Lphi_loop_block2:
	; v3 = iconst.i64 1
	; v7 = iadd v5, v3
	add	x11, x11, #1
	; jump block1(v7)
	b	Lphi_loop_block1
Lphi_loop_block3:
	; return v0
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lphi_loop_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_edge_transfers
	.p2align	2
_edge_transfers:
Ledge_transfers_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x13 v6=x11 v7=x12 v10=x14 v13=x12 v14=x11 v15=x12 v16=x11 v18=x13 v19=x9
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
Ledge_transfers_block0:
	; v4 = iconst.i64 0
	movz	x13, #0x0
	; jump block1(v4, v2, v3)
Ledge_transfers_block1:
	; v8 = icmp.ugt v1, v5
	; brif v8, block2, block6
	cmp	x10, x13
	b.ls	Ledge_transfers_block6
Ledge_transfers_block2:
	; v9 = ptradd v0, v5
	; v10 = load.i8 v9
	ldrb	w14, [x9, x13, lsl #0]
	; v11 = iconst.i8 0
	; v12 = icmp.eq v10, v11
	; brif v12, block3, block4
	cmp	w14, #0
	b.eq	Ledge_transfers_block3
Ledge_transfers_block4:
	; v14 = xor v6, v7
	eor	x11, x11, x12
	; jump block5(v7, v14)
	b	Ledge_transfers_block5
Ledge_transfers_block3:
	; v13 = iadd v6, v7
	add	x12, x11, x12
	; jump block5(v13, v6)
Ledge_transfers_block5:
	; v17 = iconst.i64 1
	; v18 = iadd v5, v17
	add	x13, x13, #1
	; jump block1(v18, v15, v16)
	mov	x16, x11
	mov	x11, x12
	mov	x12, x16
	b	Ledge_transfers_block1
Ledge_transfers_block6:
	; v19 = xor v6, v7
	eor	x9, x11, x12
	; return v19
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ledge_transfers_end:
	.cfi_endproc
	.subsections_via_symbols
