	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_add64
	.p2align	2
_k_add64:
Lk_add64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_add64_block0:
	; v2 = asm "add %0, %1, %2", "=r,r,r"(v0, v1) -> i64
	mov	x1, x9
	mov	x2, x10
	add x0, x1, x2
	mov	x9, x0
	; return v2
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_add64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_rev32
	.p2align	2
_k_rev32:
Lk_rev32_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_rev32_block0:
	; v1 = asm "rev %0, %1", "=r,r"(v0) -> i32
	mov	x1, x9
	rev w0, w1
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_rev32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_addw
	.p2align	2
_k_addw:
Lk_addw_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_addw_block0:
	; v2 = asm "add %w0, %w1, %w2", "=r,r,r"(v0, v1) -> i64
	mov	x1, x9
	mov	x2, x10
	add w0, w1, w2
	mov	x9, x0
	; return v2
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_addw_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_fmadd
	.p2align	2
_k_fmadd:
Lk_fmadd_begin:
	; regalloc: v0=d19 v1=d20 v2=d21 v3=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
	fmov	d21, d2
Lk_fmadd_block0:
	; v3 = asm "fmadd %0, %1, %2, %3", "=w,w,w,w"(v0, v1, v2) -> f64
	fmov	d1, d19
	fmov	d2, d20
	fmov	d3, d21
	fmadd d0, d1, d2, d3
	fmov	d19, d0
	; return v3
	fmov	d0, d19
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_fmadd_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_fadd32
	.p2align	2
_k_fadd32:
Lk_fadd32_begin:
	; regalloc: v0=d19 v1=d20 v2=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
	fmov	d20, d1
Lk_fadd32_block0:
	; v2 = asm "fadd %0, %1, %2", "=w,w,w"(v0, v1) -> f32
	fmov	d1, d19
	fmov	d2, d20
	fadd s0, s1, s2
	fmov	d19, d0
	; return v2
	fmov	d0, d19
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_fadd32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_fcvt
	.p2align	2
_k_fcvt:
Lk_fcvt_begin:
	; regalloc: v0=d19 v1=d19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	fmov	d19, d0
Lk_fcvt_block0:
	; v1 = asm "fcvt %s0, %d1", "=w,w"(v0) -> f32
	fmov	d1, d19
	fcvt s0, d1
	fmov	d19, d0
	; return v1
	fmov	d0, d19
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_fcvt_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_load
	.p2align	2
_k_load:
Lk_load_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_load_block0:
	; v1 = asm "ldr %0, %1", "=r,m"(v0) -> i64
	mov	x1, x9
	ldr x0, [x1]
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_load_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_add42
	.p2align	2
_k_add42:
Lk_add42_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_add42_block0:
	; v1 = iconst.i64 42
	movz	x10, #0x2a
	; v2 = asm "add %0, %1, #%2", "=r,r,i"(v0, v1) -> i64
	mov	x1, x9
	add x0, x1, #42
	mov	x9, x0
	; return v2
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_add42_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_tied
	.p2align	2
_k_tied:
Lk_tied_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_tied_block0:
	; v2 = asm "add %0, %0, %2", "=r,0,r"(v1, v0) -> i64
	mov	x0, x10
	mov	x1, x9
	add x0, x0, x1
	mov	x9, x0
	; return v2
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_tied_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_clobber_caller
	.p2align	2
_k_clobber_caller:
Lk_clobber_caller_begin:
	; regalloc: v0=x9 v2=x19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x9, x0
Lk_clobber_caller_block0:
	; v1 = iconst.i64 1
	; v2 = iadd v0, v1
	add	x19, x9, #1
	; asm "mov x9, xzr\n mov x10, xzr\n mov x11, xzr\n mov x12, xzr\n mov x13, xzr\n mov x14, xzr\n mov x15, xzr", "~{x9},~{x10},~{x11},~{x12},~{x13},~{x14},~{x15}"()
	mov x9, xzr
	mov x10, xzr
	mov x11, xzr
	mov x12, xzr
	mov x13, xzr
	mov x14, xzr
	mov x15, xzr
	; return v2
	mov	x0, x19
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_clobber_caller_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_getpid
	.p2align	2
_k_getpid:
Lk_getpid_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lk_getpid_block0:
	; v0 = iconst.i64 20
	movz	x9, #0x14
	; v1 = asm "svc #0x80", "={x0},{x16},~{memory}"(v0) -> i64
	mov	x16, x9
	svc #0x80
	mov	x9, x0
	; return v1
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_getpid_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_add_carry
	.p2align	2
_k_add_carry:
Lk_add_carry_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_add_carry_block0:
	; v3, v4 = asm "adds %0, %2, %3\n cset %w1, cs", "=r,=r,r,r,~{cc}"(v0, v1) -> (i64, i32)
	mov	x2, x9
	mov	x3, x10
	adds x0, x2, x3
	cset w1, cs
	mov	x9, x0
	mov	x10, x1
	; store v3, v2
	str	x9, [x11]
	; return v4
	mov	x0, x10
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_add_carry_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_clobber_all
	.p2align	2
_k_clobber_all:
Lk_clobber_all_begin:
	; regalloc: v0=[sp, #0] v1=[sp, #8] v3=[sp, #16] v4=[sp, #24] v5=x9 v6=x10 v7=x9
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
	stp	d8, d9, [sp, #-16]!
	.cfi_offset d8, -112
	.cfi_offset d9, -104
	stp	d10, d11, [sp, #-16]!
	.cfi_offset d10, -128
	.cfi_offset d11, -120
	stp	d12, d13, [sp, #-16]!
	.cfi_offset d12, -144
	.cfi_offset d13, -136
	stp	d14, d15, [sp, #-16]!
	.cfi_offset d14, -160
	.cfi_offset d15, -152
	sub	sp, sp, #32
	str	x0, [sp, #0]
Lk_clobber_all_block0:
	; v1 = iconst.i64 3
	movz	x16, #0x3
	str	x16, [sp, #8]
	; v2 = imul v0, v1
	; v3 = iadd v0, v1
	ldr	x16, [sp, #0]
	add	x16, x16, #3
	str	x16, [sp, #16]
	; v4 = sitofp.f64 v0
	ldr	x16, [sp, #0]
	scvtf	d16, x16
	str	d16, [sp, #24]
	; asm "mov x9, xzr\n mov x10, xzr\n mov x11, xzr\n mov x12, xzr\n mov x13, xzr\n mov x14, xzr\n mov x15, xzr\n mov x19, xzr\n mov x20, xzr\n mov x21, xzr\n mov x22, xzr\n mov x23, xzr\n mov x24, xzr\n mov x25, xzr\n mov x26, xzr\n mov x27, xzr\n mov x28, xzr\n mov x30, xzr\n movi d8, #0\n movi d9, #0\n movi d10, #0\n movi d11, #0\n movi d12, #0\n movi d13, #0\n movi d14, #0\n movi d15, #0\n movi d19, #0\n movi d24, #0\n movi d31, #0", "~{x9},~{x10},~{x11},~{x12},~{x13},~{x14},~{x15},~{x19},~{x20},~{x21},~{x22},~{x23},~{x24},~{x25},~{x26},~{x27},~{x28},~{x30},~{v8},~{v9},~{v10},~{v11},~{v12},~{v13},~{v14},~{v15},~{v19},~{v20},~{v21},~{v22},~{v23},~{v24},~{v25},~{v26},~{v27},~{v28},~{v29},~{v30},~{v31}"()
	mov x9, xzr
	mov x10, xzr
	mov x11, xzr
	mov x12, xzr
	mov x13, xzr
	mov x14, xzr
	mov x15, xzr
	mov x19, xzr
	mov x20, xzr
	mov x21, xzr
	mov x22, xzr
	mov x23, xzr
	mov x24, xzr
	mov x25, xzr
	mov x26, xzr
	mov x27, xzr
	mov x28, xzr
	mov x30, xzr
	movi d8, #0
	movi d9, #0
	movi d10, #0
	movi d11, #0
	movi d12, #0
	movi d13, #0
	movi d14, #0
	movi d15, #0
	movi d19, #0
	movi d24, #0
	movi d31, #0
	; v5 = fptosi.i64 v4
	ldr	d16, [sp, #24]
	fcvtzs	x9, d16
	; v6 = iadd v2, v3
	ldr	x16, [sp, #16]
	ldr	x17, [sp, #0]
	ldr	x8, [sp, #8]
	madd	x10, x17, x8, x16
	; v7 = iadd v5, v6
	add	x9, x9, x10
	; return v7
	mov	x0, x9
	sub	sp, x29, #144
	ldp	d14, d15, [sp], #16
	ldp	d12, d13, [sp], #16
	ldp	d10, d11, [sp], #16
	ldp	d8, d9, [sp], #16
	ldp	x27, x28, [sp], #16
	ldp	x25, x26, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_clobber_all_end:
	.cfi_endproc
	.subsections_via_symbols
