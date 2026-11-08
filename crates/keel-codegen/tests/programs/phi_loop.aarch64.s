	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_phi_loop
	.p2align	2
_phi_loop:
Lphi_loop_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x9 v5=x11 v7=x11 v8=x9
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
	; v3 = iconst.i64 1
	; jump block1(v0, v2)
Lphi_loop_block1:
	; v6 = icmp.ult v5, v1
	; brif v6, block2, block3(v4)
	cmp	x11, x10
	b.hs	Lphi_loop_block3
Lphi_loop_block2:
	; v7 = iadd v5, v3
	add	x11, x11, #1
	; jump block1(v4, v7)
	b	Lphi_loop_block1
Lphi_loop_block3:
	; return v8
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lphi_loop_end:
	.cfi_endproc
	.subsections_via_symbols
