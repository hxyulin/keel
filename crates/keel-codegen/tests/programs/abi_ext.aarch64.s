	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_pass
	.p2align	2
_k_pass:
Lk_pass_begin:
	; regalloc: v0=x9 v1=x10 v2=x9 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_pass_block0:
	; v1 = trunc.i8 v0
	mov	x10, x9
	; v2 = trunc.i16 v0
	; v3 = call @c_raw(v1, v2, v1, v2)
	mov	x0, x10
	mov	x1, x9
	mov	x2, x10
	mov	x3, x9
	sxtb	w0, w0
	sxth	w1, w1
	uxtb	w2, w2
	uxth	w3, w3
	bl	_c_raw
	mov	x9, x0
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_stack
	.p2align	2
_k_pass_stack:
Lk_pass_stack_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x9, x0
Lk_pass_stack_block0:
	; v1 = trunc.i8 v0
	mov	x10, x9
	; v2 = trunc.i16 v0
	mov	x11, x9
	; v3 = trunc.i32 v0
	mov	x12, x9
	; v4 = call @c_stack(v0, v0, v0, v0, v0, v0, v0, v0, v1, v2, v1, v3)
	strb	w10, [sp, #0]
	strh	w11, [sp, #2]
	strb	w10, [sp, #4]
	str	w12, [sp, #8]
	mov	x0, x9
	mov	x1, x9
	mov	x2, x9
	mov	x3, x9
	mov	x4, x9
	mov	x5, x9
	mov	x6, x9
	mov	x7, x9
	bl	_c_stack
	mov	x9, x0
	; return v4
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_stack_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ret_s8
	.p2align	2
_k_ret_s8:
Lk_ret_s8_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_ret_s8_block0:
	; v1 = trunc.i8 v0
	; return v1
	mov	x0, x9
	sxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_ret_s8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ret_u8
	.p2align	2
_k_ret_u8:
Lk_ret_u8_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_ret_u8_block0:
	; v1 = trunc.i8 v0
	; return v1
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_ret_u8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ret_s16
	.p2align	2
_k_ret_s16:
Lk_ret_s16_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_ret_s16_block0:
	; v1 = trunc.i16 v0
	; return v1
	mov	x0, x9
	sxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_ret_s16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_ret_u16
	.p2align	2
_k_ret_u16:
Lk_ret_u16_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_ret_u16_block0:
	; v1 = trunc.i16 v0
	; return v1
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_ret_u16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_use_ret
	.p2align	2
_k_use_ret:
Lk_use_ret_begin:
	; regalloc: v0=x9 v1=x9 v2=x10 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lk_use_ret_block0:
	; v0 = call @c_ret_s8()
	bl	_c_ret_s8
	mov	x9, x0
	; v1 = sext.i32 v0
	sxtb	w9, w9
	; v2 = iconst.i32 1
	; v3 = iadd v1, v2
	add	w9, w9, #1
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_use_ret_end:
	.cfi_endproc
	.subsections_via_symbols
