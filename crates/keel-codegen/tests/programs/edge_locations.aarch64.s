	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_edge_locations
	.p2align	2
_edge_locations:
Ledge_locations_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Ledge_locations_block0:
	; brif v0, block1(v1), block1(v2)
	cbz	x9, Ledge_locations_inst0_else
	mov	x9, x10
	b	Ledge_locations_block1
Ledge_locations_inst0_else:
	mov	x9, x11
Ledge_locations_block1:
	; v4 = iadd v3, v1
	add	x9, x9, x10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ledge_locations_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_empty_branch
	.p2align	2
_empty_branch:
Lempty_branch_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lempty_branch_block0:
	; brif v0, block1, block2
	cbnz	x9, Lempty_branch_block1
Lempty_branch_block2:
	; v4 = iadd v1, v2
	add	x9, x10, x11
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lempty_branch_block1:
	; v3 = isub v1, v2
	sub	x9, x10, x11
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lempty_branch_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_empty_switch
	.p2align	2
_empty_switch:
Lempty_switch_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lempty_switch_block0:
	; switch v0, block1, [1: block2, 2: block2, 3: block2, 4: block2]
	sub	x16, x9, #1
	cmp	x16, #3
	b.hi	Lempty_switch_block1
	adr	x17, Lempty_switch_inst0_table
	ldrsw	x16, [x17, x16, lsl #2]
	add	x17, x17, x16
	br	x17
Lempty_switch_inst0_table:
	.long	Lempty_switch_block2-Lempty_switch_inst0_table
	.long	Lempty_switch_block2-Lempty_switch_inst0_table
	.long	Lempty_switch_block2-Lempty_switch_inst0_table
	.long	Lempty_switch_block2-Lempty_switch_inst0_table
Lempty_switch_block2:
	; v4 = iadd v1, v2
	add	x9, x10, x11
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lempty_switch_block1:
	; v3 = isub v1, v2
	sub	x9, x10, x11
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lempty_switch_end:
	.cfi_endproc
	.subsections_via_symbols
