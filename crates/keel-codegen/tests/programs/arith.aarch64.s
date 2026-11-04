	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_udiv32
	.p2align	2
_udiv32:
Ludiv32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ludiv32_block0:
	; v2 = udiv v0, v1
	cbz	w10, Ludiv32_trap
	udiv	w9, w9, w10
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ludiv32_trap:
	brk	#1
Ludiv32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_sdiv64
	.p2align	2
_sdiv64:
Lsdiv64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lsdiv64_block0:
	; v2 = sdiv v0, v1
	cbz	x10, Lsdiv64_trap
	movz	x8, #0x8000, lsl #48
	cmn	x10, #1
	ccmp	x9, x8, #0, eq
	b.eq	Lsdiv64_trap
	sdiv	x9, x9, x10
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsdiv64_trap:
	brk	#1
Lsdiv64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_srem32
	.p2align	2
_srem32:
Lsrem32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lsrem32_block0:
	; v2 = srem v0, v1
	cbz	w10, Lsrem32_trap
	sdiv	w8, w9, w10
	msub	w9, w8, w10, w9
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsrem32_trap:
	brk	#1
Lsrem32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_urem64
	.p2align	2
_urem64:
Lurem64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lurem64_block0:
	; v2 = urem v0, v1
	cbz	x10, Lurem64_trap
	udiv	x8, x9, x10
	msub	x9, x8, x10, x9
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lurem64_trap:
	brk	#1
Lurem64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_sdiv8
	.p2align	2
_sdiv8:
Lsdiv8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lsdiv8_block0:
	; v2 = trunc.i8 v0
	; v3 = trunc.i8 v1
	; v4 = sdiv v2, v3
	sxtb	w16, w9
	sxtb	w17, w10
	cbz	w17, Lsdiv8_trap
	movn	w8, #0x7f
	cmn	w17, #1
	ccmp	w16, w8, #0, eq
	b.eq	Lsdiv8_trap
	sdiv	w9, w16, w17
	; v5 = sext.i32 v4
	sxtb	w9, w9
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsdiv8_trap:
	brk	#1
Lsdiv8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_add8
	.p2align	2
_add8:
Ladd8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ladd8_block0:
	; v2 = trunc.i8 v0
	; v3 = trunc.i8 v1
	; v4 = iadd v2, v3
	add	w9, w9, w10
	; v5 = sext.i32 v4
	sxtb	w9, w9
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ladd8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_mul32
	.p2align	2
_mul32:
Lmul32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lmul32_block0:
	; v2 = imul v0, v1
	mul	w9, w9, w10
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmul32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_shl8
	.p2align	2
_shl8:
Lshl8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lshl8_block0:
	; v2 = trunc.i8 v0
	; v3 = trunc.i8 v1
	; v4 = shl v2, v3
	and	w17, w10, #7
	lsl	w9, w9, w17
	; v5 = uext.i32 v4
	uxtb	w9, w9
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lshl8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_ushr8
	.p2align	2
_ushr8:
Lushr8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lushr8_block0:
	; v2 = trunc.i8 v0
	; v3 = trunc.i8 v1
	; v4 = ushr v2, v3
	and	w17, w10, #7
	uxtb	w16, w9
	lsr	w9, w16, w17
	; v5 = uext.i32 v4
	uxtb	w9, w9
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lushr8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_sshr16
	.p2align	2
_sshr16:
Lsshr16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lsshr16_block0:
	; v2 = trunc.i16 v0
	; v3 = trunc.i16 v1
	; v4 = sshr v2, v3
	and	w17, w10, #15
	sxth	w16, w9
	asr	w9, w16, w17
	; v5 = sext.i32 v4
	sxth	w9, w9
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsshr16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_shl64
	.p2align	2
_shl64:
Lshl64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lshl64_block0:
	; v2 = shl v0, v1
	lsl	x9, x9, x10
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lshl64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_bitops
	.p2align	2
_bitops:
Lbitops_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lbitops_block0:
	; v2 = and v0, v1
	and	x11, x9, x10
	; v3 = or v0, v1
	orr	x9, x9, x10
	; v4 = xor v2, v3
	eor	x9, x11, x9
	; v5 = isub v4, v1
	sub	x9, x9, x10
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lbitops_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_slt8
	.p2align	2
_slt8:
Lslt8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lslt8_block0:
	; v2 = trunc.i8 v0
	; v3 = trunc.i8 v1
	; v4 = icmp.slt v2, v3
	sxtb	w16, w9
	sxtb	w17, w10
	cmp	w16, w17
	cset	w9, lt
	; v5 = uext.i32 v4
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lslt8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_ult16
	.p2align	2
_ult16:
Lult16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x10 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lult16_block0:
	; v2 = trunc.i16 v0
	; v3 = trunc.i16 v1
	; v4 = icmp.ult v2, v3
	uxth	w16, w9
	uxth	w17, w10
	cmp	w16, w17
	cset	w9, lo
	; v5 = uext.i32 v4
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lult16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_ugt64
	.p2align	2
_ugt64:
Lugt64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lugt64_block0:
	; v2 = icmp.ugt v0, v1
	cmp	x9, x10
	cset	w9, hi
	; v3 = uext.i32 v2
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lugt64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_sext_low_byte
	.p2align	2
_sext_low_byte:
Lsext_low_byte_begin:
	; regalloc: v0=x9 v1=x9 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lsext_low_byte_block0:
	; v1 = trunc.i8 v0
	; v2 = sext.i64 v1
	sxtb	x9, w9
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsext_low_byte_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_uext32
	.p2align	2
_uext32:
Luext32_begin:
	; regalloc: v0=x9 v1=x9 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Luext32_block0:
	; v1 = trunc.i32 v0
	; v2 = uext.i64 v1
	mov	w9, w9
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Luext32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_big_const
	.p2align	2
_big_const:
Lbig_const_begin:
	; regalloc: v0=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lbig_const_block0:
	; v0 = iconst.i64 1311768467463790320
	movz	x9, #0xdef0
	movk	x9, #0x9abc, lsl #16
	movk	x9, #0x5678, lsl #32
	movk	x9, #0x1234, lsl #48
	; return v0
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lbig_const_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_neg_const
	.p2align	2
_neg_const:
Lneg_const_begin:
	; regalloc: v0=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lneg_const_block0:
	; v0 = iconst.i64 -2
	movn	x9, #0x1
	; return v0
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lneg_const_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_store16
	.p2align	2
_store16:
Lstore16_begin:
	; regalloc: v0=x9 v1=x10 v2=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lstore16_block0:
	; v2 = trunc.i16 v1
	; store v2, v0
	strh	w10, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lstore16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_load_u8
	.p2align	2
_load_u8:
Lload_u8_begin:
	; regalloc: v0=x9 v1=x9 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lload_u8_block0:
	; v1 = load.i8 v0
	ldrb	w9, [x9]
	; v2 = uext.i32 v1
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lload_u8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_swap_loop
	.p2align	2
_swap_loop:
Lswap_loop_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10 v5=x11 v6=x12 v8=x12 v9=x11 v10=x11 v11=x9 v12=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lswap_loop_block0:
	; jump block1(v0, v1, v2)
Lswap_loop_block1:
	; v6 = iconst.i64 0
	; v7 = icmp.eq v5, v6
	; brif v7, block3, block2
	cmp	x11, #0
	b.eq	Lswap_loop_block3
Lswap_loop_block2:
	; v8 = iconst.i64 1
	; v9 = isub v5, v8
	sub	x11, x11, #1
	; jump block1(v4, v3, v9)
	mov	x16, x9
	mov	x9, x10
	mov	x10, x16
	b	Lswap_loop_block1
Lswap_loop_block3:
	; v10 = iconst.i64 10
	movz	x11, #0xa
	; v11 = imul v3, v10
	mul	x9, x9, x11
	; v12 = iadd v11, v4
	add	x9, x9, x10
	; return v12
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lswap_loop_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_max64
	.p2align	2
_max64:
Lmax64_begin:
	; regalloc: v0=x9 v1=x10 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lmax64_block0:
	; v2 = icmp.sgt v0, v1
	; brif v2, block1(v0), block1(v1)
	cmp	x9, x10
	b.gt	Lmax64_block1
	mov	x9, x10
Lmax64_block1:
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmax64_end:
	.cfi_endproc
	.subsections_via_symbols
