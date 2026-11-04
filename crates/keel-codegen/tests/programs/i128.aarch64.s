	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_iadd
	.p2align	2
_k_iadd:
Lk_iadd_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_iadd_block0:
	; v2 = iadd v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_iadd_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_isub
	.p2align	2
_k_isub:
Lk_isub_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_isub_block0:
	; v2 = isub v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	subs	x4, x0, x2
	sbc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_isub_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_imul
	.p2align	2
_k_imul:
Lk_imul_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_imul_block0:
	; v2 = imul v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_imul_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_udiv
	.p2align	2
_k_udiv:
Lk_udiv_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_udiv_block0:
	; v2 = udiv v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	orr	x16, x2, x3
	cbz	x16, Lk_udiv_trap
	bl	___udivti3
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_udiv_trap:
	brk	#1
Lk_udiv_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sdiv
	.p2align	2
_k_sdiv:
Lk_sdiv_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_sdiv_block0:
	; v2 = sdiv v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	orr	x16, x2, x3
	cbz	x16, Lk_sdiv_trap
	and	x16, x2, x3
	cmn	x16, #1
	b.ne	Lk_sdiv_inst0_divok
	cbnz	x0, Lk_sdiv_inst0_divok
	movz	x16, #0x8000, lsl #48
	cmp	x1, x16
	b.eq	Lk_sdiv_trap
Lk_sdiv_inst0_divok:
	bl	___divti3
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_sdiv_trap:
	brk	#1
Lk_sdiv_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_urem
	.p2align	2
_k_urem:
Lk_urem_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_urem_block0:
	; v2 = urem v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	orr	x16, x2, x3
	cbz	x16, Lk_urem_trap
	bl	___umodti3
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_urem_trap:
	brk	#1
Lk_urem_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_srem
	.p2align	2
_k_srem:
Lk_srem_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_srem_block0:
	; v2 = srem v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	orr	x16, x2, x3
	cbz	x16, Lk_srem_trap
	bl	___modti3
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_srem_trap:
	brk	#1
Lk_srem_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_and
	.p2align	2
_k_and:
Lk_and_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_and_block0:
	; v2 = and v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	and	x4, x0, x2
	and	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_and_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_or
	.p2align	2
_k_or:
Lk_or_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_or_block0:
	; v2 = or v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	orr	x4, x0, x2
	orr	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_or_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xor
	.p2align	2
_k_xor:
Lk_xor_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_xor_block0:
	; v2 = xor v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	eor	x4, x0, x2
	eor	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_xor_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_shl
	.p2align	2
_k_shl:
Lk_shl_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_shl_block0:
	; v2 = shl v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	and	x2, x2, #127
	lsl	x3, x1, x2
	mvn	w6, w2
	lsr	x7, x0, #1
	lsr	x7, x7, x6
	orr	x3, x3, x7
	lsl	x7, x0, x2
	tst	x2, #64
	csel	x5, x7, x3, ne
	csel	x4, xzr, x7, ne
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_shl_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ushr
	.p2align	2
_k_ushr:
Lk_ushr_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_ushr_block0:
	; v2 = ushr v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	and	x2, x2, #127
	lsr	x3, x0, x2
	mvn	w6, w2
	lsl	x7, x1, #1
	lsl	x7, x7, x6
	orr	x3, x3, x7
	lsr	x7, x1, x2
	mov	x6, #0
	tst	x2, #64
	csel	x4, x7, x3, ne
	csel	x5, x6, x7, ne
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_ushr_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sshr
	.p2align	2
_k_sshr:
Lk_sshr_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_sshr_block0:
	; v2 = sshr v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	and	x2, x2, #127
	lsr	x3, x0, x2
	mvn	w6, w2
	lsl	x7, x1, #1
	lsl	x7, x7, x6
	orr	x3, x3, x7
	asr	x7, x1, x2
	asr	x6, x1, #63
	tst	x2, #64
	csel	x4, x7, x3, ne
	csel	x5, x6, x7, ne
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_sshr_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_rotl
	.p2align	2
_k_rotl:
Lk_rotl_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_rotl_block0:
	; v2 = rotl v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	and	x2, x2, #127
	neg	x3, x2
	and	x3, x3, #127
	lsl	x6, x1, x2
	mvn	w7, w2
	lsr	x8, x0, #1
	lsr	x8, x8, x7
	orr	x6, x6, x8
	lsl	x8, x0, x2
	tst	x2, #64
	csel	x5, x8, x6, ne
	csel	x4, xzr, x8, ne
	lsr	x8, x0, x3
	mvn	w16, w3
	lsl	x17, x1, #1
	lsl	x17, x17, x16
	orr	x8, x8, x17
	lsr	x17, x1, x3
	mov	x16, #0
	tst	x3, #64
	csel	x6, x17, x8, ne
	csel	x7, x16, x17, ne
	orr	x4, x4, x6
	orr	x5, x5, x7
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_rotl_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_rotr
	.p2align	2
_k_rotr:
Lk_rotr_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9:x10
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
Lk_rotr_block0:
	; v2 = rotr v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	and	x2, x2, #127
	neg	x3, x2
	and	x3, x3, #127
	lsr	x6, x0, x2
	mvn	w7, w2
	lsl	x8, x1, #1
	lsl	x8, x8, x7
	orr	x6, x6, x8
	lsr	x8, x1, x2
	mov	x7, #0
	tst	x2, #64
	csel	x4, x8, x6, ne
	csel	x5, x7, x8, ne
	lsl	x8, x1, x3
	mvn	w16, w3
	lsr	x17, x0, #1
	lsr	x17, x17, x16
	orr	x8, x8, x17
	lsl	x17, x0, x3
	tst	x3, #64
	csel	x7, x17, x8, ne
	csel	x6, xzr, x17, ne
	orr	x4, x4, x6
	orr	x5, x5, x7
	mov	x9, x4
	mov	x10, x5
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_rotr_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ineg
	.p2align	2
_k_ineg:
Lk_ineg_begin:
	; regalloc: v0=x9:x10 v1=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_ineg_block0:
	; v1 = ineg v0
	mov	x0, x9
	mov	x1, x10
	negs	x4, x0
	ngc	x5, x1
	mov	x9, x4
	mov	x10, x5
	; return v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_ineg_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_bnot
	.p2align	2
_k_bnot:
Lk_bnot_begin:
	; regalloc: v0=x9:x10 v1=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_bnot_block0:
	; v1 = bnot v0
	mov	x0, x9
	mov	x1, x10
	mvn	x4, x0
	mvn	x5, x1
	mov	x9, x4
	mov	x10, x5
	; return v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_bnot_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_clz
	.p2align	2
_k_clz:
Lk_clz_begin:
	; regalloc: v0=x9:x10 v1=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_clz_block0:
	; v1 = clz v0
	mov	x0, x9
	mov	x1, x10
	cmp	x1, #0
	clz	x4, x1
	clz	x5, x0
	add	x5, x5, #64
	csel	x4, x4, x5, ne
	mov	x5, #0
	mov	x9, x4
	mov	x10, x5
	; return v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_clz_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ctz
	.p2align	2
_k_ctz:
Lk_ctz_begin:
	; regalloc: v0=x9:x10 v1=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_ctz_block0:
	; v1 = ctz v0
	mov	x0, x9
	mov	x1, x10
	cmp	x0, #0
	rbit	x4, x0
	clz	x4, x4
	rbit	x5, x1
	clz	x5, x5
	add	x5, x5, #64
	csel	x4, x4, x5, ne
	mov	x5, #0
	mov	x9, x4
	mov	x10, x5
	; return v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_ctz_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_popcnt
	.p2align	2
_k_popcnt:
Lk_popcnt_begin:
	; regalloc: v0=x9:x10 v1=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_popcnt_block0:
	; v1 = popcnt v0
	mov	x0, x9
	mov	x1, x10
	fmov	d0, x0
	fmov	d1, x1
	cnt	v0.8b, v0.8b
	cnt	v1.8b, v1.8b
	addv	b0, v0.8b
	addv	b1, v1.8b
	fmov	w4, s0
	fmov	w5, s1
	add	x4, x4, x5
	mov	x5, #0
	mov	x9, x4
	mov	x10, x5
	; return v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_popcnt_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_bswap
	.p2align	2
_k_bswap:
Lk_bswap_begin:
	; regalloc: v0=x9:x10 v1=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_bswap_block0:
	; v1 = bswap v0
	mov	x0, x9
	mov	x1, x10
	rev	x4, x1
	rev	x5, x0
	mov	x9, x4
	mov	x10, x5
	; return v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_bswap_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_eq
	.p2align	2
_k_eq:
Lk_eq_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_eq_block0:
	; v2 = icmp.eq v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x0, x2
	ccmp	x1, x3, #0, eq
	cset	w9, eq
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_eq_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ne
	.p2align	2
_k_ne:
Lk_ne_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_ne_block0:
	; v2 = icmp.ne v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x0, x2
	ccmp	x1, x3, #0, eq
	cset	w9, ne
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_ne_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_slt
	.p2align	2
_k_slt:
Lk_slt_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_slt_block0:
	; v2 = icmp.slt v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x0, x2
	sbcs	xzr, x1, x3
	cset	w9, lt
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_slt_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sle
	.p2align	2
_k_sle:
Lk_sle_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_sle_block0:
	; v2 = icmp.sle v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x2, x0
	sbcs	xzr, x3, x1
	cset	w9, ge
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_sle_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sgt
	.p2align	2
_k_sgt:
Lk_sgt_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_sgt_block0:
	; v2 = icmp.sgt v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x2, x0
	sbcs	xzr, x3, x1
	cset	w9, lt
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_sgt_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sge
	.p2align	2
_k_sge:
Lk_sge_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_sge_block0:
	; v2 = icmp.sge v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x0, x2
	sbcs	xzr, x1, x3
	cset	w9, ge
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_sge_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ult
	.p2align	2
_k_ult:
Lk_ult_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_ult_block0:
	; v2 = icmp.ult v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x0, x2
	sbcs	xzr, x1, x3
	cset	w9, lo
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_ult_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ule
	.p2align	2
_k_ule:
Lk_ule_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_ule_block0:
	; v2 = icmp.ule v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x2, x0
	sbcs	xzr, x3, x1
	cset	w9, hs
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_ule_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ugt
	.p2align	2
_k_ugt:
Lk_ugt_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_ugt_block0:
	; v2 = icmp.ugt v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x2, x0
	sbcs	xzr, x3, x1
	cset	w9, lo
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_ugt_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_uge
	.p2align	2
_k_uge:
Lk_uge_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x9 v3=x9
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
Lk_uge_block0:
	; v2 = icmp.uge v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	cmp	x0, x2
	sbcs	xzr, x1, x3
	cset	w9, hs
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_uge_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_select
	.p2align	2
_k_select:
Lk_select_begin:
	; regalloc: v0=x9:x10 v1=x11:x12 v2=x13:x14 v3=x9:x10
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
	mov	x13, x4
	mov	x14, x5
Lk_select_block0:
	; v3 = select v0, v1, v2
	mov	x0, x11
	mov	x1, x12
	mov	x2, x13
	mov	x3, x14
	mov	x4, x9
	mov	x5, x10
	orr	x6, x4, x5
	cmp	x6, #0
	csel	x6, x0, x2, ne
	csel	x7, x1, x3, ne
	mov	x9, x6
	mov	x10, x7
	; return v3
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_select_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_select32
	.p2align	2
_k_select32:
Lk_select32_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x12:x13 v3=x9:x10
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
	mov	x13, x4
Lk_select32_block0:
	; v3 = select v0, v1, v2
	mov	x0, x10
	mov	x1, x11
	mov	x2, x12
	mov	x3, x13
	mov	x4, x9
	cmp	w4, #0
	csel	x6, x0, x2, ne
	csel	x7, x1, x3, ne
	mov	x9, x6
	mov	x10, x7
	; return v3
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_select32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_truth
	.p2align	2
_k_truth:
Lk_truth_begin:
	; regalloc: v0=x9:x10 v1=x11 v2=x12 v3=x11 v4=x9 v5=x9 v6=x9 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_truth_block0:
	; v1 = iconst.i64 1
	movz	x11, #0x1
	; v2 = iconst.i64 0
	movz	x12, #0x0
	; v3 = select v0, v1, v2
	orr	x16, x9, x10
	cmp	x16, #0
	csel	x11, x11, x12, ne
	; brif v0, block1, block2
	orr	x16, x9, x10
	cmp	x16, #0
	b.ne	Lk_truth_block1
Lk_truth_block2:
	; v6 = iconst.i64 4
	; v7 = iadd v3, v6
	add	x9, x11, #4
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_truth_block1:
	; v4 = iconst.i64 2
	; v5 = iadd v3, v4
	add	x9, x11, #2
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_truth_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ext
	.p2align	2
_k_ext:
Lk_ext_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=[sp, #0]:[sp, #8] v4=x9:x14 v5=x15:x19 v6=x10:x20 v7=x21:[sp, #16] v8=[sp, #24]:[sp, #32] v9=x24:x25 v10=x26:x27 v11=x28:x13 v12=x12:x23 v13=x11:x22 v14=x9:x14 v15=x15:x19 v16=x10:x13 v17=x12:x20 v18=x11:x21 v19=x9:x14 v20=x9:x14 v21=x9:x10 v22=x9:x10 v23=x9:x10
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
	sub	sp, sp, #48
	mov	x9, x0
Lk_ext_block0:
	; v1 = trunc.i32 v0
	mov	x10, x9
	; v2 = trunc.i8 v0
	mov	x11, x9
	; v3 = sext.i128 v0
	mov	x0, x9
	mov	x4, x0
	asr	x5, x4, #63
	str	x4, [sp, #0]
	str	x5, [sp, #8]
	; v4 = uext.i128 v0
	mov	x0, x9
	mov	x4, x0
	mov	x5, #0
	mov	x9, x4
	mov	x14, x5
	; v5 = sext.i128 v1
	mov	x0, x10
	sxtw	x4, w0
	asr	x5, x4, #63
	mov	x15, x4
	mov	x19, x5
	; v6 = uext.i128 v1
	mov	x0, x10
	mov	w4, w0
	mov	x5, #0
	mov	x10, x4
	mov	x20, x5
	; v7 = sext.i128 v2
	mov	x0, x11
	sxtb	x4, w0
	asr	x5, x4, #63
	mov	x21, x4
	str	x5, [sp, #16]
	; v8 = uext.i128 v2
	mov	x0, x11
	uxtb	w4, w0
	mov	x5, #0
	str	x4, [sp, #24]
	str	x5, [sp, #32]
	; v9 = iconst.i128 3
	movz	x4, #0x3
	movz	x5, #0x0
	mov	x24, x4
	mov	x25, x5
	; v10 = iconst.i128 5
	movz	x4, #0x5
	movz	x5, #0x0
	mov	x26, x4
	mov	x27, x5
	; v11 = iconst.i128 7
	movz	x4, #0x7
	movz	x5, #0x0
	mov	x28, x4
	mov	x13, x5
	; v12 = iconst.i128 11
	movz	x4, #0xb
	movz	x5, #0x0
	mov	x12, x4
	mov	x23, x5
	; v13 = iconst.i128 13
	movz	x4, #0xd
	movz	x5, #0x0
	mov	x11, x4
	mov	x22, x5
	; v14 = imul v4, v9
	mov	x0, x9
	mov	x1, x14
	mov	x2, x24
	mov	x3, x25
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x9, x4
	mov	x14, x5
	; v15 = imul v5, v10
	mov	x0, x15
	mov	x1, x19
	mov	x2, x26
	mov	x3, x27
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x15, x4
	mov	x19, x5
	; v16 = imul v6, v11
	mov	x0, x10
	mov	x1, x20
	mov	x2, x28
	mov	x3, x13
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x10, x4
	mov	x13, x5
	; v17 = imul v7, v12
	mov	x0, x21
	ldr	x1, [sp, #16]
	mov	x2, x12
	mov	x3, x23
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x12, x4
	mov	x20, x5
	; v18 = imul v8, v13
	ldr	x0, [sp, #24]
	ldr	x1, [sp, #32]
	mov	x2, x11
	mov	x3, x22
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x11, x4
	mov	x21, x5
	; v19 = iadd v3, v14
	ldr	x0, [sp, #0]
	ldr	x1, [sp, #8]
	mov	x2, x9
	mov	x3, x14
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x14, x5
	; v20 = iadd v19, v15
	mov	x0, x9
	mov	x1, x14
	mov	x2, x15
	mov	x3, x19
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x14, x5
	; v21 = iadd v20, v16
	mov	x0, x9
	mov	x1, x14
	mov	x2, x10
	mov	x3, x13
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v22 = iadd v21, v17
	mov	x0, x9
	mov	x1, x10
	mov	x2, x12
	mov	x3, x20
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v23 = iadd v22, v18
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x21
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v23
	mov	x0, x9
	mov	x1, x10
	sub	sp, x29, #80
	ldp	x27, x28, [sp], #16
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_ext_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_lo
	.p2align	2
_k_lo:
Lk_lo_begin:
	; regalloc: v0=x9:x10 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_lo_block0:
	; v1 = trunc.i64 v0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_lo_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_hi
	.p2align	2
_k_hi:
Lk_hi_begin:
	; regalloc: v0=x9:x10 v1=x11 v2=x9:x10 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_hi_block0:
	; v1 = iconst.i32 64
	movz	x11, #0x40
	; v2 = ushr v0, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	and	x2, x2, #127
	lsr	x3, x0, x2
	mvn	w6, w2
	lsl	x7, x1, #1
	lsl	x7, x7, x6
	orr	x3, x3, x7
	lsr	x7, x1, x2
	mov	x6, #0
	tst	x2, #64
	csel	x4, x7, x3, ne
	csel	x5, x6, x7, ne
	mov	x9, x4
	mov	x10, x5
	; v3 = trunc.i64 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_hi_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_mem
	.p2align	2
_k_mem:
Lk_mem_begin:
	; regalloc: v0=x9 v1=x10 v2=x11:x12 v3=x10:x13 v4=x14:x15 v5=x10:x13
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_mem_block0:
	; v2 = load.i128 v0
	ldp	x4, x5, [x9]
	mov	x11, x4
	mov	x12, x5
	; v3 = load.i128 v1
	ldp	x4, x5, [x10]
	mov	x10, x4
	mov	x13, x5
	; v4 = iconst.i128 1
	movz	x4, #0x1
	movz	x5, #0x0
	mov	x14, x4
	mov	x15, x5
	; v5 = iadd v3, v4
	mov	x0, x10
	mov	x1, x13
	mov	x2, x14
	mov	x3, x15
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x10, x4
	mov	x13, x5
	; store v5, v0
	mov	x0, x10
	mov	x1, x13
	mov	x2, x9
	stp	x0, x1, [x2]
	; return v2
	mov	x0, x11
	mov	x1, x12
	ldp	x29, x30, [sp], #16
	ret
Lk_mem_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_minus_one
	.p2align	2
_k_minus_one:
Lk_minus_one_begin:
	; regalloc: v0=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lk_minus_one_block0:
	; v0 = iconst.i128 -1
	movn	x4, #0x0
	movn	x5, #0x0
	mov	x9, x4
	mov	x10, x5
	; return v0
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_minus_one_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_args
	.p2align	2
_k_args:
Lk_args_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x12 v3=x9:x13 v4=x14:x15 v5=x10:x11 v6=x9:x10 v7=x11:x12 v8=x9:x10
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
Lk_args_block0:
	; v3 = sext.i128 v0
	mov	x0, x9
	mov	x4, x0
	asr	x5, x4, #63
	mov	x9, x4
	mov	x13, x5
	; v4 = iconst.i128 3
	movz	x4, #0x3
	movz	x5, #0x0
	mov	x14, x4
	mov	x15, x5
	; v5 = imul v1, v4
	mov	x0, x10
	mov	x1, x11
	mov	x2, x14
	mov	x3, x15
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x10, x4
	mov	x11, x5
	; v6 = iadd v3, v5
	mov	x0, x9
	mov	x1, x13
	mov	x2, x10
	mov	x3, x11
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v7 = sext.i128 v2
	mov	x0, x12
	mov	x4, x0
	asr	x5, x4, #63
	mov	x11, x4
	mov	x12, x5
	; v8 = isub v6, v7
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	subs	x4, x0, x2
	sbc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v8
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_args_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_tail
	.p2align	2
_k_tail:
Lk_tail_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19:x20 v8=x21 v9=x9 v10=x9 v11=x9 v12=x9 v13=x9 v14=x9 v15=x9:x10 v16=x11:x12 v17=x11:x12 v18=x9:x10 v19=x11:x12 v20=x13:x14 v21=x11:x12 v22=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	str	x21, [sp, #-16]!
	.cfi_offset x21, -48
	ldr	x19, [x29, #16]
	ldr	x20, [x29, #24]
	ldr	x21, [x29, #32]
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
	mov	x14, x5
	mov	x15, x6
Lk_tail_block0:
	; v9 = iadd v0, v1
	add	x9, x9, x10
	; v10 = iadd v9, v2
	add	x9, x9, x11
	; v11 = iadd v10, v3
	add	x9, x9, x12
	; v12 = iadd v11, v4
	add	x9, x9, x13
	; v13 = iadd v12, v5
	add	x9, x9, x14
	; v14 = iadd v13, v6
	add	x9, x9, x15
	; v15 = sext.i128 v14
	mov	x0, x9
	mov	x4, x0
	asr	x5, x4, #63
	mov	x9, x4
	mov	x10, x5
	; v16 = iconst.i128 1000
	movz	x4, #0x3e8
	movz	x5, #0x0
	mov	x11, x4
	mov	x12, x5
	; v17 = imul v7, v16
	mov	x0, x19
	mov	x1, x20
	mov	x2, x11
	mov	x3, x12
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x11, x4
	mov	x12, x5
	; v18 = iadd v15, v17
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v19 = sext.i128 v8
	mov	x0, x21
	mov	x4, x0
	asr	x5, x4, #63
	mov	x11, x4
	mov	x12, x5
	; v20 = iconst.i128 7
	movz	x4, #0x7
	movz	x5, #0x0
	mov	x13, x4
	mov	x14, x5
	; v21 = imul v19, v20
	mov	x0, x11
	mov	x1, x12
	mov	x2, x13
	mov	x3, x14
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x11, x4
	mov	x12, x5
	; v22 = iadd v18, v21
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v22
	mov	x0, x9
	mov	x1, x10
	sub	sp, x29, #32
	ldr	x21, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_tail_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_call_c
	.p2align	2
_k_call_c:
Lk_call_c_begin:
	; regalloc: v0=x19:x20 v1=x21 v2=x22 v3=x23:x24 v4=x9 v5=x10 v6=x11 v7=x12 v8=x13 v9=x14 v10=x9:x10 v11=x9:x10 v12=x9:x10
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
	sub	sp, sp, #32
	mov	x19, x0
	mov	x20, x1
Lk_call_c_block0:
	; v1 = iconst.i64 1
	movz	x21, #0x1
	; v2 = iconst.i64 2
	movz	x22, #0x2
	; v3 = call @c_args(v1, v0, v2)
	mov	x0, x21
	mov	x1, x19
	mov	x2, x20
	mov	x3, x22
	bl	_c_args
	mov	x23, x0
	mov	x24, x1
	; v4 = iconst.i64 3
	movz	x9, #0x3
	; v5 = iconst.i64 4
	movz	x10, #0x4
	; v6 = iconst.i64 5
	movz	x11, #0x5
	; v7 = iconst.i64 6
	movz	x12, #0x6
	; v8 = iconst.i64 7
	movz	x13, #0x7
	; v9 = iconst.i64 -5
	movn	x14, #0x4
	; v10 = call @c_tail(v1, v2, v4, v5, v6, v7, v8, v0, v9)
	str	x19, [sp, #0]
	str	x20, [sp, #8]
	str	x14, [sp, #16]
	mov	x0, x21
	mov	x1, x22
	mov	x2, x9
	mov	x3, x10
	mov	x4, x11
	mov	x5, x12
	mov	x6, x13
	bl	_c_tail
	mov	x9, x0
	mov	x10, x1
	; v11 = iadd v3, v10
	mov	x0, x23
	mov	x1, x24
	mov	x2, x9
	mov	x3, x10
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v12 = iadd v11, v0
	mov	x0, x9
	mov	x1, x10
	mov	x2, x19
	mov	x3, x20
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v12
	mov	x0, x9
	mov	x1, x10
	sub	sp, x29, #48
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_call_c_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pow
	.p2align	2
_k_pow:
Lk_pow_begin:
	; regalloc: v0=x9:x10 v1=x11 v2=x12:x13 v3=x9:x10 v4=x11 v5=x12:x13 v6=x14 v7=x15 v8=x19:x20 v9=x12:x13 v10=x9:x10 v11=x11 v12=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_pow_block0:
	; v2 = iconst.i128 1
	movz	x4, #0x1
	movz	x5, #0x0
	mov	x12, x4
	mov	x13, x5
	; jump block1(v0, v1, v2)
Lk_pow_block1:
	; brif v4, block2, block3(v5)
	cbnz	x11, Lk_pow_block2
	mov	x9, x12
	mov	x10, x13
Lk_pow_block3:
	; return v12
	mov	x0, x9
	mov	x1, x10
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_pow_block2:
	; v6 = iconst.i64 1
	; v7 = and v4, v6
	and	x15, x11, #0x1
	; v8 = imul v5, v3
	mov	x0, x12
	mov	x1, x13
	mov	x2, x9
	mov	x3, x10
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x19, x4
	mov	x20, x5
	; v9 = select v7, v8, v5
	mov	x0, x19
	mov	x1, x20
	mov	x2, x12
	mov	x3, x13
	mov	x4, x15
	cmp	x4, #0
	csel	x6, x0, x2, ne
	csel	x7, x1, x3, ne
	mov	x12, x6
	mov	x13, x7
	; v10 = imul v3, v3
	mov	x0, x9
	mov	x1, x10
	mov	x2, x9
	mov	x3, x10
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x9, x4
	mov	x10, x5
	; v11 = ushr v4, v6
	lsr	x11, x11, #1
	; jump block1(v10, v11, v9)
	b	Lk_pow_block1
Lk_pow_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_tail16
	.p2align	2
_k_tail16:
Lk_tail16_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19 v8=x20 v9=x21:x22 v10=x9 v11=x9 v12=x9 v13=x9 v14=x9 v15=x9 v16=x9 v17=x10 v18=x10 v19=x9 v20=x9:x10 v21=x11:x12 v22=x11:x12 v23=x9:x10
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
	ldr	x20, [x29, #16]
	ldr	x21, [x29, #32]
	ldr	x22, [x29, #40]
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
	mov	x14, x5
	mov	x15, x6
	mov	x19, x7
Lk_tail16_block0:
	; v10 = iadd v0, v1
	add	x9, x9, x10
	; v11 = iadd v10, v2
	add	x9, x9, x11
	; v12 = iadd v11, v3
	add	x9, x9, x12
	; v13 = iadd v12, v4
	add	x9, x9, x13
	; v14 = iadd v13, v5
	add	x9, x9, x14
	; v15 = iadd v14, v6
	add	x9, x9, x15
	; v16 = iadd v15, v7
	add	x9, x9, x19
	; v17 = iconst.i64 3
	movz	x10, #0x3
	; v18 = imul v8, v17
	mul	x10, x20, x10
	; v19 = iadd v16, v18
	add	x9, x9, x10
	; v20 = sext.i128 v19
	mov	x0, x9
	mov	x4, x0
	asr	x5, x4, #63
	mov	x9, x4
	mov	x10, x5
	; v21 = iconst.i128 1000
	movz	x4, #0x3e8
	movz	x5, #0x0
	mov	x11, x4
	mov	x12, x5
	; v22 = imul v9, v21
	mov	x0, x21
	mov	x1, x22
	mov	x2, x11
	mov	x3, x12
	mul	x4, x0, x2
	umulh	x5, x0, x2
	madd	x5, x0, x3, x5
	madd	x5, x1, x2, x5
	mov	x11, x4
	mov	x12, x5
	; v23 = iadd v20, v22
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v23
	mov	x0, x9
	mov	x1, x10
	sub	sp, x29, #32
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_tail16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_rem_live
	.p2align	2
_k_rem_live:
Lk_rem_live_begin:
	; regalloc: v0=x19:x20 v1=x21:x22 v2=x23 v3=x9:x10 v4=x9:x10 v5=x9:x10 v6=x11:x12 v7=x9:x10
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
	str	x23, [sp, #-16]!
	.cfi_offset x23, -64
	mov	x19, x0
	mov	x20, x1
	mov	x21, x2
	mov	x22, x3
	mov	x23, x4
Lk_rem_live_block0:
	; v3 = urem v0, v1
	mov	x0, x19
	mov	x1, x20
	mov	x2, x21
	mov	x3, x22
	orr	x16, x2, x3
	cbz	x16, Lk_rem_live_trap
	bl	___umodti3
	mov	x9, x0
	mov	x10, x1
	; v4 = iadd v3, v0
	mov	x0, x9
	mov	x1, x10
	mov	x2, x19
	mov	x3, x20
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v5 = iadd v4, v1
	mov	x0, x9
	mov	x1, x10
	mov	x2, x21
	mov	x3, x22
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v6 = sext.i128 v2
	mov	x0, x23
	mov	x4, x0
	asr	x5, x4, #63
	mov	x11, x4
	mov	x12, x5
	; v7 = iadd v5, v6
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; return v7
	mov	x0, x9
	mov	x1, x10
	sub	sp, x29, #48
	ldr	x23, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_rem_live_trap:
	brk	#1
Lk_rem_live_end:
	.cfi_endproc
	.subsections_via_symbols
