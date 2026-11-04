	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_mask_uext32
	.p2align	2
_mask_uext32:
Lmask_uext32_begin:
	; regalloc: v0=x9 v1=x9 v2=x9
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
	; return v2
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
	; regalloc: v0=x9 v1=x9 v3=x9 v4=x9
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
	; v3 = and v1, v2
	and	w9, w9, #0x7fffffff
	; v4 = uext.i64 v3
	mov	w9, w9
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
	; regalloc: v0=x9 v1=x9 v3=x9 v4=x9
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
	; regalloc: v0=x9 v1=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lmask_wide8_block0:
	; v1 = trunc.i8 v0
	; v5 = uext.i64 v1
	uxtb	w9, w9
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
	; regalloc: v5=x9:x10 v6=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lmask_wide_sign_block0:
	; v5 = iconst.i128 0
	movz	x4, #0x0
	movz	x5, #0x0
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
