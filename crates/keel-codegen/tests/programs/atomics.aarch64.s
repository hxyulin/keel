	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_add_i8
	.p2align	2
_k_add_i8:
Lk_add_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_add_i8_block0:
	; v2 = atomic_rmw.add relaxed v0, v1
	ldaddb	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_add_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sub_i8
	.p2align	2
_k_sub_i8:
Lk_sub_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_sub_i8_block0:
	; v2 = atomic_rmw.sub acquire v0, v1
	neg	w17, w10
	ldaddab	w17, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_sub_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_and_i8
	.p2align	2
_k_and_i8:
Lk_and_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_and_i8_block0:
	; v2 = atomic_rmw.and release v0, v1
	mvn	w17, w10
	ldclrlb	w17, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_and_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_or_i8
	.p2align	2
_k_or_i8:
Lk_or_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_or_i8_block0:
	; v2 = atomic_rmw.or acq_rel v0, v1
	ldsetalb	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_or_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xor_i8
	.p2align	2
_k_xor_i8:
Lk_xor_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_xor_i8_block0:
	; v2 = atomic_rmw.xor seq_cst v0, v1
	ldeoralb	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_xor_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xchg_i8
	.p2align	2
_k_xchg_i8:
Lk_xchg_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_xchg_i8_block0:
	; v2 = atomic_rmw.xchg relaxed v0, v1
	swpb	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_xchg_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umin_i8
	.p2align	2
_k_umin_i8:
Lk_umin_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_umin_i8_block0:
	; v2 = atomic_rmw.umin acquire v0, v1
	lduminab	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_umin_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umax_i8
	.p2align	2
_k_umax_i8:
Lk_umax_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_umax_i8_block0:
	; v2 = atomic_rmw.umax release v0, v1
	ldumaxlb	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_umax_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smin_i8
	.p2align	2
_k_smin_i8:
Lk_smin_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_smin_i8_block0:
	; v2 = atomic_rmw.smin acq_rel v0, v1
	ldsminalb	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	sxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_smin_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smax_i8
	.p2align	2
_k_smax_i8:
Lk_smax_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_smax_i8_block0:
	; v2 = atomic_rmw.smax seq_cst v0, v1
	ldsmaxalb	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	sxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_smax_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_cas_i8
	.p2align	2
_k_cas_i8:
Lk_cas_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_cas_i8_block0:
	; v3 = atomic_cas relaxed v0, v1, v2
	mov	x8, x10
	casb	w8, w11, [x9]
	mov	x9, x8
	; return v3
	mov	x0, x9
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_cas_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_swap_i8
	.p2align	2
_k_swap_i8:
Lk_swap_i8_begin:
	; regalloc: v0=x9 v1=x10 v2=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_swap_i8_block0:
	; v2 = atomic_load.i8 seq_cst v0
	ldarb	w11, [x9]
	; fence acquire
	dmb	ishld
	; atomic_store seq_cst v1, v0
	stlrb	w10, [x9]
	; fence seq_cst
	dmb	ish
	; return v2
	mov	x0, x11
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_swap_i8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_add_i16
	.p2align	2
_k_add_i16:
Lk_add_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_add_i16_block0:
	; v2 = atomic_rmw.add release v0, v1
	ldaddlh	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_add_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sub_i16
	.p2align	2
_k_sub_i16:
Lk_sub_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_sub_i16_block0:
	; v2 = atomic_rmw.sub acq_rel v0, v1
	neg	w17, w10
	ldaddalh	w17, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_sub_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_and_i16
	.p2align	2
_k_and_i16:
Lk_and_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_and_i16_block0:
	; v2 = atomic_rmw.and seq_cst v0, v1
	mvn	w17, w10
	ldclralh	w17, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_and_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_or_i16
	.p2align	2
_k_or_i16:
Lk_or_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_or_i16_block0:
	; v2 = atomic_rmw.or relaxed v0, v1
	ldseth	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_or_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xor_i16
	.p2align	2
_k_xor_i16:
Lk_xor_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_xor_i16_block0:
	; v2 = atomic_rmw.xor acquire v0, v1
	ldeorah	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_xor_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xchg_i16
	.p2align	2
_k_xchg_i16:
Lk_xchg_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_xchg_i16_block0:
	; v2 = atomic_rmw.xchg release v0, v1
	swplh	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_xchg_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umin_i16
	.p2align	2
_k_umin_i16:
Lk_umin_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_umin_i16_block0:
	; v2 = atomic_rmw.umin acq_rel v0, v1
	lduminalh	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_umin_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umax_i16
	.p2align	2
_k_umax_i16:
Lk_umax_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_umax_i16_block0:
	; v2 = atomic_rmw.umax seq_cst v0, v1
	ldumaxalh	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_umax_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smin_i16
	.p2align	2
_k_smin_i16:
Lk_smin_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_smin_i16_block0:
	; v2 = atomic_rmw.smin relaxed v0, v1
	ldsminh	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	sxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_smin_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smax_i16
	.p2align	2
_k_smax_i16:
Lk_smax_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_smax_i16_block0:
	; v2 = atomic_rmw.smax acquire v0, v1
	ldsmaxah	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	sxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_smax_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_cas_i16
	.p2align	2
_k_cas_i16:
Lk_cas_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_cas_i16_block0:
	; v3 = atomic_cas release v0, v1, v2
	mov	x8, x10
	caslh	w8, w11, [x9]
	mov	x9, x8
	; return v3
	mov	x0, x9
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_cas_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_swap_i16
	.p2align	2
_k_swap_i16:
Lk_swap_i16_begin:
	; regalloc: v0=x9 v1=x10 v2=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_swap_i16_block0:
	; v2 = atomic_load.i16 seq_cst v0
	ldarh	w11, [x9]
	; fence acquire
	dmb	ishld
	; atomic_store seq_cst v1, v0
	stlrh	w10, [x9]
	; fence seq_cst
	dmb	ish
	; return v2
	mov	x0, x11
	uxth	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_swap_i16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_add_i32
	.p2align	2
_k_add_i32:
Lk_add_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_add_i32_block0:
	; v2 = atomic_rmw.add seq_cst v0, v1
	ldaddal	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_add_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sub_i32
	.p2align	2
_k_sub_i32:
Lk_sub_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_sub_i32_block0:
	; v2 = atomic_rmw.sub relaxed v0, v1
	neg	w17, w10
	ldadd	w17, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_sub_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_and_i32
	.p2align	2
_k_and_i32:
Lk_and_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_and_i32_block0:
	; v2 = atomic_rmw.and acquire v0, v1
	mvn	w17, w10
	ldclra	w17, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_and_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_or_i32
	.p2align	2
_k_or_i32:
Lk_or_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_or_i32_block0:
	; v2 = atomic_rmw.or release v0, v1
	ldsetl	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_or_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xor_i32
	.p2align	2
_k_xor_i32:
Lk_xor_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_xor_i32_block0:
	; v2 = atomic_rmw.xor acq_rel v0, v1
	ldeoral	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_xor_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xchg_i32
	.p2align	2
_k_xchg_i32:
Lk_xchg_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_xchg_i32_block0:
	; v2 = atomic_rmw.xchg seq_cst v0, v1
	swpal	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_xchg_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umin_i32
	.p2align	2
_k_umin_i32:
Lk_umin_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_umin_i32_block0:
	; v2 = atomic_rmw.umin relaxed v0, v1
	ldumin	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_umin_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umax_i32
	.p2align	2
_k_umax_i32:
Lk_umax_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_umax_i32_block0:
	; v2 = atomic_rmw.umax acquire v0, v1
	ldumaxa	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_umax_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smin_i32
	.p2align	2
_k_smin_i32:
Lk_smin_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_smin_i32_block0:
	; v2 = atomic_rmw.smin release v0, v1
	ldsminl	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_smin_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smax_i32
	.p2align	2
_k_smax_i32:
Lk_smax_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_smax_i32_block0:
	; v2 = atomic_rmw.smax acq_rel v0, v1
	ldsmaxal	w10, w8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_smax_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_cas_i32
	.p2align	2
_k_cas_i32:
Lk_cas_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_cas_i32_block0:
	; v3 = atomic_cas seq_cst v0, v1, v2
	mov	x8, x10
	casal	w8, w11, [x9]
	mov	x9, x8
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_cas_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_swap_i32
	.p2align	2
_k_swap_i32:
Lk_swap_i32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_swap_i32_block0:
	; v2 = atomic_load.i32 seq_cst v0
	ldar	w11, [x9]
	; fence acquire
	dmb	ishld
	; atomic_store seq_cst v1, v0
	stlr	w10, [x9]
	; fence seq_cst
	dmb	ish
	; return v2
	mov	x0, x11
	ldp	x29, x30, [sp], #16
	ret
Lk_swap_i32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_add_i64
	.p2align	2
_k_add_i64:
Lk_add_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_add_i64_block0:
	; v2 = atomic_rmw.add acquire v0, v1
	ldadda	x10, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_add_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sub_i64
	.p2align	2
_k_sub_i64:
Lk_sub_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_sub_i64_block0:
	; v2 = atomic_rmw.sub release v0, v1
	neg	x17, x10
	ldaddl	x17, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_sub_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_and_i64
	.p2align	2
_k_and_i64:
Lk_and_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_and_i64_block0:
	; v2 = atomic_rmw.and acq_rel v0, v1
	mvn	x17, x10
	ldclral	x17, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_and_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_or_i64
	.p2align	2
_k_or_i64:
Lk_or_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_or_i64_block0:
	; v2 = atomic_rmw.or seq_cst v0, v1
	ldsetal	x10, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_or_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xor_i64
	.p2align	2
_k_xor_i64:
Lk_xor_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_xor_i64_block0:
	; v2 = atomic_rmw.xor relaxed v0, v1
	ldeor	x10, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_xor_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xchg_i64
	.p2align	2
_k_xchg_i64:
Lk_xchg_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_xchg_i64_block0:
	; v2 = atomic_rmw.xchg acquire v0, v1
	swpa	x10, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_xchg_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umin_i64
	.p2align	2
_k_umin_i64:
Lk_umin_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_umin_i64_block0:
	; v2 = atomic_rmw.umin release v0, v1
	lduminl	x10, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_umin_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umax_i64
	.p2align	2
_k_umax_i64:
Lk_umax_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_umax_i64_block0:
	; v2 = atomic_rmw.umax acq_rel v0, v1
	ldumaxal	x10, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_umax_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smin_i64
	.p2align	2
_k_smin_i64:
Lk_smin_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_smin_i64_block0:
	; v2 = atomic_rmw.smin seq_cst v0, v1
	ldsminal	x10, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_smin_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smax_i64
	.p2align	2
_k_smax_i64:
Lk_smax_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_smax_i64_block0:
	; v2 = atomic_rmw.smax relaxed v0, v1
	ldsmax	x10, x8, [x9]
	mov	x9, x8
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_smax_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_cas_i64
	.p2align	2
_k_cas_i64:
Lk_cas_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_cas_i64_block0:
	; v3 = atomic_cas acquire v0, v1, v2
	mov	x8, x10
	casa	x8, x11, [x9]
	mov	x9, x8
	; return v3
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_cas_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_swap_i64
	.p2align	2
_k_swap_i64:
Lk_swap_i64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_swap_i64_block0:
	; v2 = atomic_load.i64 seq_cst v0
	ldar	x11, [x9]
	; fence acquire
	dmb	ishld
	; atomic_store seq_cst v1, v0
	stlr	x10, [x9]
	; fence seq_cst
	dmb	ish
	; return v2
	mov	x0, x11
	ldp	x29, x30, [sp], #16
	ret
Lk_swap_i64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xchg_ptr
	.p2align	2
_k_xchg_ptr:
Lk_xchg_ptr_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v3=x11 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_xchg_ptr_block0:
	; v2 = atomic_rmw.xchg seq_cst v0, v1
	swpal	x10, x8, [x9]
	mov	x10, x8
	; v3 = atomic_load.ptr acquire v0
	ldar	x11, [x9]
	; v4 = atomic_cas acq_rel v0, v3, v2
	mov	x8, x11
	casal	x8, x10, [x9]
	mov	x9, x8
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_xchg_ptr_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_count
	.p2align	2
_k_count:
Lk_count_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x11 v5=x12 v6=x13 v7=x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lk_count_block0:
	; v2 = iconst.i64 0
	movz	x11, #0x0
	; jump block1(v2)
Lk_count_block1:
	; v4 = icmp.ult v3, v1
	; brif v4, block2, block3
	cmp	x11, x10
	b.lo	Lk_count_block2
Lk_count_block3:
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_count_block2:
	; v5 = iconst.i64 1
	movz	x12, #0x1
	; v6 = atomic_rmw.add relaxed v0, v5
	ldadd	x12, x8, [x9]
	mov	x13, x8
	; v7 = iadd v3, v5
	add	x11, x11, #1
	; jump block1(v7)
	b	Lk_count_block1
Lk_count_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_lock
	.p2align	2
_k_lock:
Lk_lock_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_lock_block0:
	; v1 = iconst.i32 0
	movz	x10, #0x0
	; v2 = iconst.i32 1
	movz	x11, #0x1
	; jump block1
Lk_lock_block1:
	; v3 = atomic_cas acquire v0, v1, v2
	mov	x8, x10
	casa	w8, w11, [x9]
	mov	x12, x8
	; v4 = icmp.eq v3, v1
	; brif v4, block2, block1
	cmp	w12, #0
	b.ne	Lk_lock_block1
Lk_lock_block2:
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_lock_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_unlock
	.p2align	2
_k_unlock:
Lk_unlock_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_unlock_block0:
	; v1 = iconst.i32 0
	movz	x10, #0x0
	; atomic_store release v1, v0
	stlr	w10, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_unlock_end:
	.cfi_endproc
	.subsections_via_symbols
