	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_load_relaxed
	.p2align	2
_k_load_relaxed:
Lk_load_relaxed_begin:
	; regalloc: v0=x9 v1=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_load_relaxed_block0:
	; v1 = atomic_load.i128 relaxed v0
	ldp	x4, x5, [x9]
	mov	x9, x4
	mov	x10, x5
	; return v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_load_relaxed_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_load_acquire
	.p2align	2
_k_load_acquire:
Lk_load_acquire_begin:
	; regalloc: v0=x9 v1=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_load_acquire_block0:
	; v1 = atomic_load.i128 acquire v0
	ldp	x4, x5, [x9]
	dmb	ishld
	mov	x9, x4
	mov	x10, x5
	; return v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_load_acquire_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_load_seq_cst
	.p2align	2
_k_load_seq_cst:
Lk_load_seq_cst_begin:
	; regalloc: v0=x9 v1=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_load_seq_cst_block0:
	; v1 = atomic_load.i128 seq_cst v0
	ldar	x6, [x9]
	ldp	x4, x5, [x9]
	dmb	ishld
	mov	x9, x4
	mov	x10, x5
	; return v1
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_load_seq_cst_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_store_relaxed
	.p2align	2
_k_store_relaxed:
Lk_store_relaxed_begin:
	; regalloc: v0=x9 v1=x10:x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_store_relaxed_block0:
	; atomic_store relaxed v1, v0
	mov	x0, x10
	mov	x1, x11
	mov	x2, x9
	stp	x0, x1, [x2]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_store_relaxed_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_store_release
	.p2align	2
_k_store_release:
Lk_store_release_begin:
	; regalloc: v0=x9 v1=x10:x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_store_release_block0:
	; atomic_store release v1, v0
	mov	x0, x10
	mov	x1, x11
	mov	x2, x9
	dmb	ish
	stp	x0, x1, [x2]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_store_release_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_store_seq_cst
	.p2align	2
_k_store_seq_cst:
Lk_store_seq_cst_begin:
	; regalloc: v0=x9 v1=x10:x11
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_store_seq_cst_block0:
	; atomic_store seq_cst v1, v0
	mov	x0, x10
	mov	x1, x11
	mov	x2, x9
	dmb	ish
	stp	x0, x1, [x2]
	dmb	ish
	; return
	ldp	x29, x30, [sp], #16
	ret
Lk_store_seq_cst_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_cas_strong
	.p2align	2
_k_cas_strong:
Lk_cas_strong_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x12:x13 v3=x14 v4=x9:x10 v5=x11
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
Lk_cas_strong_block0:
	; v4, v5 = atomic_cas acq_rel acquire v0, v1, v2
	mov	x0, x10
	mov	x1, x11
	mov	x2, x12
	mov	x3, x13
	mov	x4, x9
	mov	x5, x10
	mov	x6, x11
	caspal	x0, x1, x2, x3, [x4]
	cmp	x0, x5
	ccmp	x1, x6, #0, eq
	cset	w7, eq
	mov	x9, x0
	mov	x10, x1
	mov	x11, x7
	; store v4, v3
	mov	x0, x9
	mov	x1, x10
	mov	x2, x14
	stp	x0, x1, [x2]
	; return v5
	mov	x0, x11
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_cas_strong_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_cas_weak
	.p2align	2
_k_cas_weak:
Lk_cas_weak_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x12:x13 v3=x14 v4=x9:x10 v5=x11
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
Lk_cas_weak_block0:
	; v4, v5 = atomic_cas weak relaxed seq_cst v0, v1, v2
	mov	x0, x10
	mov	x1, x11
	mov	x2, x12
	mov	x3, x13
	mov	x4, x9
	mov	x5, x10
	mov	x6, x11
	caspal	x0, x1, x2, x3, [x4]
	cmp	x0, x5
	ccmp	x1, x6, #0, eq
	cset	w7, eq
	mov	x9, x0
	mov	x10, x1
	mov	x11, x7
	; store v4, v3
	mov	x0, x9
	mov	x1, x10
	mov	x2, x14
	stp	x0, x1, [x2]
	; return v5
	mov	x0, x11
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_cas_weak_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_cas_release
	.p2align	2
_k_cas_release:
Lk_cas_release_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x12:x13 v3=x14 v4=x9:x10 v5=x11
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
Lk_cas_release_block0:
	; v4, v5 = atomic_cas release relaxed v0, v1, v2
	mov	x0, x10
	mov	x1, x11
	mov	x2, x12
	mov	x3, x13
	mov	x4, x9
	mov	x5, x10
	mov	x6, x11
	caspl	x0, x1, x2, x3, [x4]
	cmp	x0, x5
	ccmp	x1, x6, #0, eq
	cset	w7, eq
	mov	x9, x0
	mov	x10, x1
	mov	x11, x7
	; store v4, v3
	mov	x0, x9
	mov	x1, x10
	mov	x2, x14
	stp	x0, x1, [x2]
	; return v5
	mov	x0, x11
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_cas_release_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_cas_acquire
	.p2align	2
_k_cas_acquire:
Lk_cas_acquire_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x12:x13 v3=x14 v4=x9:x10 v5=x11
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
Lk_cas_acquire_block0:
	; v4, v5 = atomic_cas acquire acquire v0, v1, v2
	mov	x0, x10
	mov	x1, x11
	mov	x2, x12
	mov	x3, x13
	mov	x4, x9
	mov	x5, x10
	mov	x6, x11
	caspa	x0, x1, x2, x3, [x4]
	cmp	x0, x5
	ccmp	x1, x6, #0, eq
	cset	w7, eq
	mov	x9, x0
	mov	x10, x1
	mov	x11, x7
	; store v4, v3
	mov	x0, x9
	mov	x1, x10
	mov	x2, x14
	stp	x0, x1, [x2]
	; return v5
	mov	x0, x11
	uxtb	w0, w0
	ldp	x29, x30, [sp], #16
	ret
Lk_cas_acquire_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_add
	.p2align	2
_k_add:
Lk_add_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_add_block0:
	; v2 = atomic_rmw.add relaxed v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
Lk_add_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	adds	x2, x0, x6
	adc	x3, x1, x7
	casp	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_add_inst0_retry
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_add_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_sub
	.p2align	2
_k_sub:
Lk_sub_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_sub_block0:
	; v2 = atomic_rmw.sub acquire v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
Lk_sub_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	subs	x2, x0, x6
	sbc	x3, x1, x7
	caspa	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_sub_inst0_retry
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_sub_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_and
	.p2align	2
_k_and:
Lk_and_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_and_block0:
	; v2 = atomic_rmw.and release v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
Lk_and_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	and	x2, x0, x6
	and	x3, x1, x7
	caspl	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_and_inst0_retry
	mov	x9, x0
	mov	x10, x1
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
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_or_block0:
	; v2 = atomic_rmw.or acq_rel v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
Lk_or_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	orr	x2, x0, x6
	orr	x3, x1, x7
	caspal	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_or_inst0_retry
	mov	x9, x0
	mov	x10, x1
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
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_xor_block0:
	; v2 = atomic_rmw.xor seq_cst v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
Lk_xor_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	eor	x2, x0, x6
	eor	x3, x1, x7
	caspal	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_xor_inst0_retry
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_xor_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_xchg
	.p2align	2
_k_xchg:
Lk_xchg_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_xchg_block0:
	; v2 = atomic_rmw.xchg relaxed v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
	mov	x2, x6
	mov	x3, x7
Lk_xchg_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	casp	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_xchg_inst0_retry
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_xchg_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umin
	.p2align	2
_k_umin:
Lk_umin_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_umin_block0:
	; v2 = atomic_rmw.umin acquire v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
Lk_umin_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	cmp	x0, x6
	sbcs	xzr, x1, x7
	csel	x2, x0, x6, lo
	csel	x3, x1, x7, lo
	caspa	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_umin_inst0_retry
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_umin_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_umax
	.p2align	2
_k_umax:
Lk_umax_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_umax_block0:
	; v2 = atomic_rmw.umax release v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
Lk_umax_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	cmp	x0, x6
	sbcs	xzr, x1, x7
	csel	x2, x0, x6, hs
	csel	x3, x1, x7, hs
	caspl	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_umax_inst0_retry
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_umax_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smin
	.p2align	2
_k_smin:
Lk_smin_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_smin_block0:
	; v2 = atomic_rmw.smin acq_rel v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
Lk_smin_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	cmp	x0, x6
	sbcs	xzr, x1, x7
	csel	x2, x0, x6, lt
	csel	x3, x1, x7, lt
	caspal	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_smin_inst0_retry
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_smin_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_smax
	.p2align	2
_k_smax:
Lk_smax_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x9:x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_smax_block0:
	; v2 = atomic_rmw.smax seq_cst v0, v1
	mov	x4, x9
	mov	x6, x10
	mov	x7, x11
	ldp	x0, x1, [x4]
Lk_smax_inst0_retry:
	mov	x16, x0
	mov	x17, x1
	cmp	x0, x6
	sbcs	xzr, x1, x7
	csel	x2, x0, x6, ge
	csel	x3, x1, x7, ge
	caspal	x0, x1, x2, x3, [x4]
	cmp	x0, x16
	ccmp	x1, x17, #0, eq
	b.ne	Lk_smax_inst0_retry
	mov	x9, x0
	mov	x10, x1
	; return v2
	mov	x0, x9
	mov	x1, x10
	ldp	x29, x30, [sp], #16
	ret
Lk_smax_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_bump
	.p2align	2
_k_bump:
Lk_bump_begin:
	; regalloc: v0=x9 v1=x10:x11 v2=x10:x11 v3=x12:x13 v4=x12:x13 v5=x14:x15 v6=x12:x13 v7=x12:x13 v8=x19:x20 v9=x14:x15 v10=x12:x13 v11=x10:x11
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
Lk_bump_block0:
	; v1 = atomic_load.i128 relaxed v0
	ldp	x4, x5, [x9]
	mov	x10, x4
	mov	x11, x5
	; jump block1(v1)
Lk_bump_block1:
	; v3 = iconst.i128 1
	movz	x4, #0x1
	movz	x5, #0x0
	mov	x12, x4
	mov	x13, x5
	; v4 = iadd v2, v3
	mov	x0, x10
	mov	x1, x11
	mov	x2, x12
	mov	x3, x13
	adds	x4, x0, x2
	adc	x5, x1, x3
	mov	x12, x4
	mov	x13, x5
	; v5 = iconst.i128 64
	movz	x4, #0x40
	movz	x5, #0x0
	mov	x14, x4
	mov	x15, x5
	; v6 = shl v4, v5
	mov	x0, x12
	mov	x1, x13
	mov	x2, x14
	mov	x3, x15
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
	mov	x12, x4
	mov	x13, x5
	; v7 = ushr v6, v5
	mov	x0, x12
	mov	x1, x13
	mov	x2, x14
	mov	x3, x15
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
	mov	x12, x4
	mov	x13, x5
	; v8 = bnot v7
	mov	x0, x12
	mov	x1, x13
	mvn	x4, x0
	mvn	x5, x1
	mov	x19, x4
	mov	x20, x5
	; v9 = shl v8, v5
	mov	x0, x19
	mov	x1, x20
	mov	x2, x14
	mov	x3, x15
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
	mov	x14, x4
	mov	x15, x5
	; v10 = or v7, v9
	mov	x0, x12
	mov	x1, x13
	mov	x2, x14
	mov	x3, x15
	orr	x4, x0, x2
	orr	x5, x1, x3
	mov	x12, x4
	mov	x13, x5
	; v11, v12 = atomic_cas weak acq_rel relaxed v0, v2, v10
	mov	x0, x10
	mov	x1, x11
	mov	x2, x12
	mov	x3, x13
	mov	x4, x9
	mov	x5, x10
	mov	x6, x11
	caspal	x0, x1, x2, x3, [x4]
	cmp	x0, x5
	ccmp	x1, x6, #0, eq
	mov	x10, x0
	mov	x11, x1
	; brif v12, block2, block1(v11)
	b.ne	Lk_bump_block1
Lk_bump_block2:
	; return
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_bump_end:
	.cfi_endproc
	.subsections_via_symbols
