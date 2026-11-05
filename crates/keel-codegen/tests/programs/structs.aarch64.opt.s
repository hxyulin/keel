	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_pass_s8
	.p2align	2
_k_pass_s8:
Lk_pass_s8_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_pass_s8_block0:
	; v1 = call @c_s8(v0)
	ldr	x0, [x9, #0]
	bl	_c_s8
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_s8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_s16
	.p2align	2
_k_pass_s16:
Lk_pass_s16_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_pass_s16_block0:
	; v1 = call @c_s16(v0)
	ldr	x0, [x9, #0]
	ldr	x1, [x9, #8]
	bl	_c_s16
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_s16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_s24
	.p2align	2
_k_pass_s24:
Lk_pass_s24_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #32
	mov	x9, x0
Lk_pass_s24_block0:
	; v1 = call @c_s24(v0)
	ldr	x17, [x9, #0]
	str	x17, [sp, #0]
	ldr	x17, [x9, #8]
	str	x17, [sp, #8]
	ldr	x17, [x9, #16]
	str	x17, [sp, #16]
	add	x0, sp, #0
	bl	_c_s24
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_s24_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_d2
	.p2align	2
_k_pass_d2:
Lk_pass_d2_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_pass_d2_block0:
	; v1 = call @c_d2(v0)
	ldr	d0, [x9, #0]
	ldr	d1, [x9, #8]
	bl	_c_d2
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_d2_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_f3
	.p2align	2
_k_pass_f3:
Lk_pass_f3_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_pass_f3_block0:
	; v1 = call @c_f3(v0)
	ldr	s0, [x9, #0]
	ldr	s1, [x9, #4]
	ldr	s2, [x9, #8]
	bl	_c_f3
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_f3_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_f4
	.p2align	2
_k_pass_f4:
Lk_pass_f4_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_pass_f4_block0:
	; v1 = call @c_f4(v0)
	ldr	s0, [x9, #0]
	ldr	s1, [x9, #4]
	ldr	s2, [x9, #8]
	ldr	s3, [x9, #12]
	bl	_c_f4
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_f4_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_dl
	.p2align	2
_k_pass_dl:
Lk_pass_dl_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_pass_dl_block0:
	; v1 = call @c_dl(v0)
	ldr	x0, [x9, #0]
	ldr	x1, [x9, #8]
	bl	_c_dl
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_dl_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_c3
	.p2align	2
_k_pass_c3:
Lk_pass_c3_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_pass_c3_block0:
	; v1 = call @c_c3(v0)
	ldrh	w0, [x9, #0]
	ldrb	w17, [x9, #2]
	orr	x0, x0, x17, lsl #16
	bl	_c_c3
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_c3_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_n8
	.p2align	2
_k_pass_n8:
Lk_pass_n8_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_pass_n8_block0:
	; v1 = call @c_n8(v0)
	ldr	x0, [x9, #0]
	bl	_c_n8
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_n8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_big
	.p2align	2
_k_pass_big:
Lk_pass_big_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #48
	mov	x9, x0
Lk_pass_big_block0:
	; v1 = call @c_big(v0)
	ldr	x17, [x9, #0]
	str	x17, [sp, #0]
	ldr	x17, [x9, #8]
	str	x17, [sp, #8]
	ldr	x17, [x9, #16]
	str	x17, [sp, #16]
	ldr	x17, [x9, #24]
	str	x17, [sp, #24]
	ldr	x17, [x9, #32]
	str	x17, [sp, #32]
	add	x0, sp, #0
	bl	_c_big
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_big_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_s8
	.p2align	2
_k_s8:
Lk_s8_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	str	x0, [sp, #0]
	add	x9, sp, #0
Lk_s8_block0:
	; v1 = call @h_s8(v0)
	mov	x0, x9
	bl	_h_s8
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_s8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_s16
	.p2align	2
_k_s16:
Lk_s16_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	str	x0, [sp, #0]
	str	x1, [sp, #8]
	add	x9, sp, #0
Lk_s16_block0:
	; v1 = call @h_s16(v0)
	mov	x0, x9
	bl	_h_s16
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_s16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_s24
	.p2align	2
_k_s24:
Lk_s24_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_s24_block0:
	; v1 = call @h_s24(v0)
	mov	x0, x9
	bl	_h_s24
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_s24_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_d2
	.p2align	2
_k_d2:
Lk_d2_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	str	d0, [sp, #0]
	str	d1, [sp, #8]
	add	x9, sp, #0
Lk_d2_block0:
	; v1 = call @h_d2(v0)
	mov	x0, x9
	bl	_h_d2
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_d2_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_f3
	.p2align	2
_k_f3:
Lk_f3_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	str	s0, [sp, #0]
	str	s1, [sp, #4]
	str	s2, [sp, #8]
	add	x9, sp, #0
Lk_f3_block0:
	; v1 = call @h_f3(v0)
	mov	x0, x9
	bl	_h_f3
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_f3_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_f4
	.p2align	2
_k_f4:
Lk_f4_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	str	s0, [sp, #0]
	str	s1, [sp, #4]
	str	s2, [sp, #8]
	str	s3, [sp, #12]
	add	x9, sp, #0
Lk_f4_block0:
	; v1 = call @h_f4(v0)
	mov	x0, x9
	bl	_h_f4
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_f4_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_dl
	.p2align	2
_k_dl:
Lk_dl_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	str	x0, [sp, #0]
	str	x1, [sp, #8]
	add	x9, sp, #0
Lk_dl_block0:
	; v1 = call @h_dl(v0)
	mov	x0, x9
	bl	_h_dl
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_dl_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_c3
	.p2align	2
_k_c3:
Lk_c3_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	str	x0, [sp, #0]
	add	x9, sp, #0
Lk_c3_block0:
	; v1 = call @h_c3(v0)
	mov	x0, x9
	bl	_h_c3
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_c3_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_n8
	.p2align	2
_k_n8:
Lk_n8_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	str	x0, [sp, #0]
	add	x9, sp, #0
Lk_n8_block0:
	; v1 = call @h_n8(v0)
	mov	x0, x9
	bl	_h_n8
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_n8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_big
	.p2align	2
_k_big:
Lk_big_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_big_block0:
	; v1 = call @h_big(v0)
	mov	x0, x9
	bl	_h_big
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_big_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_after7
	.p2align	2
_k_after7:
Lk_after7_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19 v8=x20 v9=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	sub	sp, sp, #16
	ldr	x20, [x29, #32]
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
	mov	x14, x5
	mov	x15, x6
	add	x19, x29, #16
Lk_after7_block0:
	; v9 = call @h_after7(v0, v1, v2, v3, v4, v5, v6, v7, v8)
	str	x20, [sp, #0]
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	mov	x4, x13
	mov	x5, x14
	mov	x6, x15
	mov	x7, x19
	bl	_h_after7
	mov	x9, x0
	; return v9
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_after7_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_fafter
	.p2align	2
_k_fafter:
Lk_fafter_begin:
	; regalloc: v0=d19 v1=d20 v2=d21 v3=d22 v4=d23 v5=d24 v6=d25 v7=x9 v8=d26 v9=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	ldr	s26, [x29, #32]
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
	fmov	d22, d3
	fmov	d23, d4
	fmov	d24, d5
	fmov	d25, d6
	add	x9, x29, #16
Lk_fafter_block0:
	; v9 = call @h_fafter(v0, v1, v2, v3, v4, v5, v6, v7, v8)
	fmov	d0, d19
	fmov	d1, d20
	fmov	d2, d21
	fmov	d3, d22
	fmov	d4, d23
	fmov	d5, d24
	fmov	d6, d25
	mov	x0, x9
	fmov	d7, d26
	bl	_h_fafter
	mov	x9, x0
	; return v9
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_fafter_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_after8
	.p2align	2
_k_after8:
Lk_after8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19 v8=x20 v9=x21 v10=x22 v11=x23 v12=x9
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
	sub	sp, sp, #48
	str	s0, [sp, #32]
	str	s1, [sp, #36]
	str	s2, [sp, #40]
	ldr	x22, [x29, #24]
	ldr	w23, [x29, #32]
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
	mov	x12, x3
	mov	x13, x4
	mov	x14, x5
	mov	x15, x6
	mov	x19, x7
	add	x20, x29, #16
	add	x21, sp, #32
Lk_after8_block0:
	; v12 = call @h_after8(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11)
	str	x20, [sp, #0]
	str	x21, [sp, #8]
	str	x22, [sp, #16]
	str	w23, [sp, #24]
	mov	x0, x9
	mov	x1, x10
	mov	x2, x11
	mov	x3, x12
	mov	x4, x13
	mov	x5, x14
	mov	x6, x15
	mov	x7, x19
	bl	_h_after8
	mov	x9, x0
	; return v12
	mov	x0, x9
	sub	sp, x29, #48
	ldr	x23, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_after8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_after7
	.p2align	2
_k_pass_after7:
Lk_pass_after7_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19 v8=x20 v9=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	sub	sp, sp, #32
	mov	x9, x0
Lk_pass_after7_block0:
	; v1 = iconst.i64 1
	movz	x10, #0x1
	; v2 = iconst.i64 2
	movz	x11, #0x2
	; v3 = iconst.i64 3
	movz	x12, #0x3
	; v4 = iconst.i64 4
	movz	x13, #0x4
	; v5 = iconst.i64 5
	movz	x14, #0x5
	; v6 = iconst.i64 6
	movz	x15, #0x6
	; v7 = iconst.i64 7
	movz	x19, #0x7
	; v8 = iconst.i64 8
	movz	x20, #0x8
	; v9 = call @c_after7(v1, v2, v3, v4, v5, v6, v7, v0, v8)
	ldr	x17, [x9, #0]
	str	x17, [sp, #0]
	ldr	x17, [x9, #8]
	str	x17, [sp, #8]
	str	x20, [sp, #16]
	mov	x0, x10
	mov	x1, x11
	mov	x2, x12
	mov	x3, x13
	mov	x4, x14
	mov	x5, x15
	mov	x6, x19
	bl	_c_after7
	mov	x9, x0
	; return v9
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_after7_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_fafter
	.p2align	2
_k_pass_fafter:
Lk_pass_fafter_begin:
	; regalloc: v0=x9 v1=d19 v2=d20 v3=d21 v4=d22 v5=d23 v6=d24 v7=d25 v8=d26 v9=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #32
	mov	x9, x0
Lk_pass_fafter_block0:
	; v1 = fconst.f64 1.0
	fmov	d19, #1.0
	; v2 = fconst.f64 2.0
	fmov	d20, #2.0
	; v3 = fconst.f64 3.0
	fmov	d21, #3.0
	; v4 = fconst.f64 4.0
	fmov	d22, #4.0
	; v5 = fconst.f64 5.0
	fmov	d23, #5.0
	; v6 = fconst.f64 6.0
	fmov	d24, #6.0
	; v7 = fconst.f64 7.0
	fmov	d25, #7.0
	; v8 = fconst.f32 0.5
	fmov	s26, #0.5
	; v9 = call @c_fafter(v1, v2, v3, v4, v5, v6, v7, v0, v8)
	ldr	x17, [x9, #0]
	str	x17, [sp, #0]
	ldr	x17, [x9, #8]
	str	x17, [sp, #8]
	str	s26, [sp, #16]
	fmov	d0, d19
	fmov	d1, d20
	fmov	d2, d21
	fmov	d3, d22
	fmov	d4, d23
	fmov	d5, d24
	fmov	d6, d25
	bl	_c_fafter
	mov	x9, x0
	; return v9
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_fafter_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_after8
	.p2align	2
_k_pass_after8:
Lk_pass_after8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v6=x15 v7=x19 v8=x20 v9=x21 v10=x22 v11=x23 v12=x9
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
	sub	sp, sp, #80
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_pass_after8_block0:
	; v3 = iconst.i64 1
	movz	x12, #0x1
	; v4 = iconst.i64 2
	movz	x13, #0x2
	; v5 = iconst.i64 3
	movz	x14, #0x3
	; v6 = iconst.i64 4
	movz	x15, #0x4
	; v7 = iconst.i64 5
	movz	x19, #0x5
	; v8 = iconst.i64 6
	movz	x20, #0x6
	; v9 = iconst.i64 7
	movz	x21, #0x7
	; v10 = iconst.i64 8
	movz	x22, #0x8
	; v11 = iconst.i32 -9
	movn	w23, #0x8
	; v12 = call @c_after8(v3, v4, v5, v6, v7, v8, v9, v10, v0, v1, v2, v11)
	ldrh	w17, [x9, #0]
	strh	w17, [sp, #0]
	ldrb	w17, [x9, #2]
	strb	w17, [sp, #2]
	ldr	x17, [x11, #0]
	str	x17, [sp, #32]
	ldr	x17, [x11, #8]
	str	x17, [sp, #40]
	ldr	x17, [x11, #16]
	str	x17, [sp, #48]
	ldr	x17, [x11, #24]
	str	x17, [sp, #56]
	ldr	x17, [x11, #32]
	str	x17, [sp, #64]
	add	x16, sp, #32
	str	x16, [sp, #8]
	str	w23, [sp, #16]
	mov	x0, x12
	mov	x1, x13
	mov	x2, x14
	mov	x3, x15
	mov	x4, x19
	mov	x5, x20
	mov	x6, x21
	mov	x7, x22
	ldr	s0, [x10, #0]
	ldr	s1, [x10, #4]
	ldr	s2, [x10, #8]
	bl	_c_after8
	mov	x9, x0
	; return v12
	mov	x0, x9
	sub	sp, x29, #48
	ldr	x23, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_after8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_fstack
	.p2align	2
_k_fstack:
Lk_fstack_begin:
	; regalloc: v0=d19 v1=d20 v2=d21 v3=d22 v4=d23 v5=d24 v6=d25 v7=d26 v8=x9 v9=d27 v10=x10 v11=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	ldr	s27, [x29, #28]
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
	fmov	d22, d3
	fmov	d23, d4
	fmov	d24, d5
	fmov	d25, d6
	fmov	d26, d7
	add	x9, x29, #16
	add	x10, x29, #32
Lk_fstack_block0:
	; v11 = call @h_fstack(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10)
	str	s27, [sp, #0]
	fmov	d0, d19
	fmov	d1, d20
	fmov	d2, d21
	fmov	d3, d22
	fmov	d4, d23
	fmov	d5, d24
	fmov	d6, d25
	fmov	d7, d26
	mov	x0, x9
	mov	x1, x10
	bl	_h_fstack
	mov	x9, x0
	; return v11
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_fstack_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_pass_fstack
	.p2align	2
_k_pass_fstack:
Lk_pass_fstack_begin:
	; regalloc: v0=x9 v1=x10 v2=d19 v3=d20 v4=d21 v5=d22 v6=d23 v7=d24 v8=d25 v9=d26 v10=d27 v11=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #32
	mov	x9, x0
	mov	x10, x1
Lk_pass_fstack_block0:
	; v2 = fconst.f64 1.0
	fmov	d19, #1.0
	; v3 = fconst.f64 2.0
	fmov	d20, #2.0
	; v4 = fconst.f64 3.0
	fmov	d21, #3.0
	; v5 = fconst.f64 4.0
	fmov	d22, #4.0
	; v6 = fconst.f64 5.0
	fmov	d23, #5.0
	; v7 = fconst.f64 6.0
	fmov	d24, #6.0
	; v8 = fconst.f64 7.0
	fmov	d25, #7.0
	; v9 = fconst.f64 8.0
	fmov	d26, #8.0
	; v10 = fconst.f32 0.75
	fmov	s27, #0.75
	; v11 = call @c_fstack(v2, v3, v4, v5, v6, v7, v8, v9, v0, v10, v1)
	ldr	x17, [x9, #0]
	str	x17, [sp, #0]
	ldr	w17, [x9, #8]
	str	w17, [sp, #8]
	str	s27, [sp, #12]
	ldr	x17, [x10, #0]
	str	x17, [sp, #16]
	ldr	w17, [x10, #8]
	str	w17, [sp, #24]
	fmov	d0, d19
	fmov	d1, d20
	fmov	d2, d21
	fmov	d3, d22
	fmov	d4, d23
	fmov	d5, d24
	fmov	d6, d25
	fmov	d7, d26
	bl	_c_fstack
	mov	x9, x0
	; return v11
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_pass_fstack_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_scribble_big
	.p2align	2
_k_scribble_big:
Lk_scribble_big_begin:
	; regalloc: v0=x19 v1=x9 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x19, x0
Lk_scribble_big_block0:
	; v1 = iconst.i64 40
	movz	x9, #0x28
	; call @scribble(v0, v1)
	mov	x0, x19
	mov	x1, x9
	bl	_scribble
	; v2 = call @h_big(v0)
	mov	x0, x19
	bl	_h_big
	mov	x9, x0
	; return v2
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_scribble_big_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_scribble_s16
	.p2align	2
_k_scribble_s16:
Lk_scribble_s16_begin:
	; regalloc: v0=x19 v1=x9 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	sub	sp, sp, #16
	str	x0, [sp, #0]
	str	x1, [sp, #8]
	add	x19, sp, #0
Lk_scribble_s16_block0:
	; v1 = iconst.i64 16
	movz	x9, #0x10
	; call @scribble(v0, v1)
	mov	x0, x19
	mov	x1, x9
	bl	_scribble
	; v2 = call @h_s16(v0)
	mov	x0, x19
	bl	_h_s16
	mov	x9, x0
	; return v2
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_scribble_s16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_k_s16
	.p2align	2
_k_k_s16:
Lk_k_s16_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_k_s16_block0:
	; v1 = call @k_s16(v0)
	ldr	x0, [x9, #0]
	ldr	x1, [x9, #8]
	bl	_k_s16
	mov	x9, x0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_k_s16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_k_big
	.p2align	2
_k_k_big:
Lk_k_big_begin:
	; regalloc: v0=x19 v1=x9 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	sub	sp, sp, #48
	mov	x19, x0
Lk_k_big_block0:
	; v1 = call @k_scribble_big(v0)
	ldr	x17, [x19, #0]
	str	x17, [sp, #0]
	ldr	x17, [x19, #8]
	str	x17, [sp, #8]
	ldr	x17, [x19, #16]
	str	x17, [sp, #16]
	ldr	x17, [x19, #24]
	str	x17, [sp, #24]
	ldr	x17, [x19, #32]
	str	x17, [sp, #32]
	add	x0, sp, #0
	bl	_k_scribble_big
	mov	x9, x0
	; v2 = call @h_big(v0)
	mov	x0, x19
	bl	_h_big
	mov	x9, x0
	; return v2
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_k_big_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_make_s16
	.p2align	2
_k_make_s16:
Lk_make_s16_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x10, x0
	add	x9, sp, #0
Lk_make_s16_block0:
	; call @fill_s16(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_fill_s16
	; return
	ldr	x0, [sp, #0]
	ldr	x1, [sp, #8]
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_make_s16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_make_s24
	.p2align	2
_k_make_s24:
Lk_make_s24_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x8
	mov	x10, x0
Lk_make_s24_block0:
	; call @fill_s24(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_fill_s24
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_make_s24_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_make_d2
	.p2align	2
_k_make_d2:
Lk_make_d2_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x10, x0
	add	x9, sp, #0
Lk_make_d2_block0:
	; call @fill_d2(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_fill_d2
	; return
	ldr	d0, [sp, #0]
	ldr	d1, [sp, #8]
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_make_d2_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_make_f3
	.p2align	2
_k_make_f3:
Lk_make_f3_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x10, x0
	add	x9, sp, #0
Lk_make_f3_block0:
	; call @fill_f3(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_fill_f3
	; return
	ldr	s0, [sp, #0]
	ldr	s1, [sp, #4]
	ldr	s2, [sp, #8]
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_make_f3_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_make_c3
	.p2align	2
_k_make_c3:
Lk_make_c3_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x10, x0
	add	x9, sp, #0
Lk_make_c3_block0:
	; call @fill_c3(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_fill_c3
	; return
	ldr	x0, [sp, #0]
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_make_c3_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_make_big
	.p2align	2
_k_make_big:
Lk_make_big_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x8
	mov	x10, x0
Lk_make_big_block0:
	; call @fill_big(v0, v1)
	mov	x0, x9
	mov	x1, x10
	bl	_fill_big
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_make_big_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_get_s16
	.p2align	2
_k_get_s16:
Lk_get_s16_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x9, x0
	mov	x10, x1
Lk_get_s16_block0:
	; call @c_make_s16(v0, v1)
	str	x9, [sp, #0]
	mov	x0, x10
	bl	_c_make_s16
	ldr	x16, [sp, #0]
	str	x0, [x16, #0]
	str	x1, [x16, #8]
	; return
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_get_s16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_get_s24
	.p2align	2
_k_get_s24:
Lk_get_s24_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_get_s24_block0:
	; call @c_make_s24(v0, v1)
	mov	x0, x10
	mov	x8, x9
	bl	_c_make_s24
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_get_s24_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_get_d2
	.p2align	2
_k_get_d2:
Lk_get_d2_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x9, x0
	mov	x10, x1
Lk_get_d2_block0:
	; call @c_make_d2(v0, v1)
	str	x9, [sp, #0]
	mov	x0, x10
	bl	_c_make_d2
	ldr	x16, [sp, #0]
	str	d0, [x16, #0]
	str	d1, [x16, #8]
	; return
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_get_d2_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_get_f3
	.p2align	2
_k_get_f3:
Lk_get_f3_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x9, x0
	mov	x10, x1
Lk_get_f3_block0:
	; call @c_make_f3(v0, v1)
	str	x9, [sp, #0]
	mov	x0, x10
	bl	_c_make_f3
	ldr	x16, [sp, #0]
	str	s0, [x16, #0]
	str	s1, [x16, #4]
	str	s2, [x16, #8]
	; return
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_get_f3_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_get_c3
	.p2align	2
_k_get_c3:
Lk_get_c3_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x9, x0
	mov	x10, x1
Lk_get_c3_block0:
	; call @c_make_c3(v0, v1)
	str	x9, [sp, #0]
	mov	x0, x10
	bl	_c_make_c3
	ldr	x16, [sp, #0]
	strh	w0, [x16, #0]
	lsr	x17, x0, #16
	strb	w17, [x16, #2]
	; return
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_get_c3_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_roundtrip
	.p2align	2
_k_roundtrip:
Lk_roundtrip_begin:
	; regalloc: v0=x19 v1=x20 v2=x21 v3=x19 v4=x9 v5=x9
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
	sub	sp, sp, #112
	mov	x19, x0
Lk_roundtrip_block0:
	; v1 = stack_addr ss0
	add	x20, sp, #48
	; call @k_make_s16(v1, v0)
	mov	x0, x19
	bl	_k_make_s16
	add	x16, sp, #48
	str	x0, [x16, #0]
	str	x1, [x16, #8]
	; v2 = stack_addr ss1
	add	x21, sp, #64
	; call @k_make_big(v2, v0)
	mov	x0, x19
	mov	x8, x21
	bl	_k_make_big
	; v3 = call @k_s16(v1)
	ldr	x0, [x20, #0]
	ldr	x1, [x20, #8]
	bl	_k_s16
	mov	x19, x0
	; v4 = call @k_big(v2)
	ldr	x17, [x21, #0]
	str	x17, [sp, #0]
	ldr	x17, [x21, #8]
	str	x17, [sp, #8]
	ldr	x17, [x21, #16]
	str	x17, [sp, #16]
	ldr	x17, [x21, #24]
	str	x17, [sp, #24]
	ldr	x17, [x21, #32]
	str	x17, [sp, #32]
	add	x0, sp, #0
	bl	_k_big
	mov	x9, x0
	; v5 = iadd v3, v4
	add	x9, x19, x9
	; return v5
	mov	x0, x9
	sub	sp, x29, #32
	ldr	x21, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_roundtrip_end:
	.cfi_endproc
	.subsections_via_symbols
