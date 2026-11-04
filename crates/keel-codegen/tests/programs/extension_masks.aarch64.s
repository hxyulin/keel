	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_mask_uext32
	.p2align	2
_mask_uext32:
Lmask_uext32_begin:
	; regalloc: v0=x9 v1=x9 v2=x9 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lmask_uext32_block0:
	; v1 = trunc.i32 v0
	; v2 = uext.i64 v1
	mov	w9, w9
	; v3 = iconst.i64 4294967295
	; v4 = and v3, v2
	and	x9, x9, #0xffffffff
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmask_uext32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_mask_sext32
	.p2align	2
_mask_sext32:
Lmask_sext32_begin:
	; regalloc: v0=x9 v1=x9 v2=x10 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lmask_sext32_block0:
	; v1 = trunc.i32 v0
	; v2 = iconst.i32 2147483647
	; v3 = and v2, v1
	and	w9, w9, #0x7fffffff
	; v4 = sext.i64 v3
	sxtw	x9, w9
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmask_sext32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_mask_sign32
	.p2align	2
_mask_sign32:
Lmask_sign32_begin:
	; regalloc: v0=x9 v1=x9 v2=x10 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lmask_sign32_block0:
	; v1 = trunc.i32 v0
	; v2 = iconst.i32 -2147483648
	; v3 = and v1, v2
	and	w9, w9, #0x80000000
	; v4 = sext.i64 v3
	sxtw	x9, w9
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmask_sign32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_mask_wide8
	.p2align	2
_mask_wide8:
Lmask_wide8_begin:
	; regalloc: v0=x9 v1=x9 v2=x9:x10 v3=x11:x12 v4=x9:x10 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lmask_wide8_block0:
	; v1 = trunc.i8 v0
	; v2 = uext.i128 v1
	mov	x0, x9
	uxtb	w4, w0
	mov	x5, #0
	mov	x9, x4
	mov	x10, x5
	; v3 = iconst.i128 255
	movz	x4, #0xff
	movz	x5, #0x0
	mov	x11, x4
	mov	x12, x5
	; v4 = and v2, v3
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	and	x4, x0, x2
	and	x5, x1, x3
	mov	x9, x4
	mov	x10, x5
	; v5 = trunc.i64 v4
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmask_wide8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_mask_wide_sign
	.p2align	2
_mask_wide_sign:
Lmask_wide_sign_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x9:x10 v4=x11 v5=x9:x10 v6=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lmask_wide_sign_block0:
	; v1 = iconst.i64 9223372036854775807
	; v2 = and v0, v1
	and	x9, x9, #0x7fffffffffffffff
	; v3 = sext.i128 v2
	mov	x0, x9
	mov	x4, x0
	asr	x5, x4, #63
	mov	x9, x4
	mov	x10, x5
	; v4 = iconst.i64 64
	movz	x11, #0x40
	; v5 = ushr v3, v4
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
	; v6 = trunc.i64 v5
	; return v6
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmask_wide_sign_end:
	.cfi_endproc
	.subsections_via_symbols
