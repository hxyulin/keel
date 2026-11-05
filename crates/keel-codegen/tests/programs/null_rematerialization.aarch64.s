	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_null_probe
	.p2align	2
_null_probe:
Lnull_probe_begin:
	; regalloc: v0=x19 v1=x20 v2=x21 v3=x9 v4=x22 v5=x19 v6=x20 v7=x9 v8=x10 v9=x11 v10=x10 v11=x11 v12=x10 v13=x9
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
	sub	sp, sp, #16
	mov	x19, x0
	mov	x20, x1
	mov	x21, x2
Lnull_probe_block0:
	; v3 = iconst.i64 0
	movz	x9, #0x0
	; v4 = inttoptr.ptr v3
	mov	x22, x9
	; call @clobber()
	bl	_clobber
	; v5 = icmp.slt v0, v1
	cmp	x19, x20
	cset	w19, lt
	; v6 = select v5, v4, v2
	tst	w19, #255
	csel	x20, x22, x21, ne
	; v7 = call @null_observe(v4, v6, v4, v4, v4, v4, v4, v4, v4, v4)
	str	x22, [sp, #0]
	str	x22, [sp, #8]
	mov	x0, x22
	mov	x1, x20
	mov	x2, x22
	mov	x3, x22
	mov	x4, x22
	mov	x5, x22
	mov	x6, x22
	mov	x7, x22
	bl	_null_observe
	mov	x9, x0
	; brif v5, block1(v4, v6), block1(v6, v4)
	tst	w19, #255
	b.eq	Lnull_probe_inst6_else
	mov	x10, x22
	mov	x11, x20
	b	Lnull_probe_block1
Lnull_probe_inst6_else:
	mov	x10, x20
	mov	x11, x22
Lnull_probe_block1:
	; v10 = ptrtoint.i64 v8
	; v11 = ptrtoint.i64 v9
	; v12 = iadd v10, v11
	add	x10, x10, x11
	; v13 = iadd v12, v7
	add	x9, x10, x9
	; return v13
	mov	x0, x9
	sub	sp, x29, #32
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lnull_probe_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_null_return
	.p2align	2
_null_return:
Lnull_return_begin:
	; regalloc: v0=x9 v1=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lnull_return_block0:
	; v0 = iconst.i64 0
	movz	x9, #0x0
	; v1 = inttoptr.ptr v0
	; return v1
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lnull_return_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_null_cycle
	.p2align	2
_null_cycle:
Lnull_cycle_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x12 v5=x9 v6=x10 v7=x11 v8=x13 v9=x14 v10=x11 v11=x9 v12=x10 v13=x11 v14=x11 v15=x9 v16=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lnull_cycle_block0:
	; v3 = iconst.i64 0
	movz	x12, #0x0
	; v4 = inttoptr.ptr v3
	; jump block1(v0, v1, v2, v4)
	mov	x13, x12
Lnull_cycle_block1:
	; v9 = iconst.i64 1
	; v10 = isub v7, v9
	sub	x11, x11, #1
	; brif v10, block1(v6, v5, v10, v4), block2(v5, v6, v8)
	cbz	x11, Lnull_cycle_inst5_else
	mov	x13, x12
	mov	x16, x9
	mov	x9, x10
	mov	x10, x16
	b	Lnull_cycle_block1
Lnull_cycle_inst5_else:
	mov	x11, x13
Lnull_cycle_block2:
	; v14 = ptrtoint.i64 v13
	; v15 = isub v11, v12
	sub	x9, x9, x10
	; v16 = iadd v15, v14
	add	x9, x9, x11
	; return v16
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lnull_cycle_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_null_branch
	.p2align	2
_null_branch:
Lnull_branch_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v4=x9 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lnull_branch_block0:
	; v1 = iconst.i64 0
	movz	x10, #0x0
	; v2 = inttoptr.ptr v1
	; v3 = icmp.eq v0, v2
	; brif v3, block1, block2
	cmp	x9, x10
	b.eq	Lnull_branch_block1
Lnull_branch_block2:
	; v5 = iconst.i64 91
	movz	x9, #0x5b
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lnull_branch_block1:
	; v4 = iconst.i64 37
	movz	x9, #0x25
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lnull_branch_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_null_setjmp
	.p2align	2
_null_setjmp:
Lnull_setjmp_begin:
	; regalloc: v0=x19 v1=x9 v2=x20 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	mov	x19, x0
Lnull_setjmp_block0:
	; v1 = iconst.i64 0
	movz	x9, #0x0
	; v2 = inttoptr.ptr v1
	mov	x20, x9
	; v3 = call @setjmp(v0)
	mov	x0, x19
	bl	_setjmp
	mov	x9, x0
	; brif v3, block2, block1
	cbnz	w9, Lnull_setjmp_block2
Lnull_setjmp_block1:
	; v4 = iconst.i32 1
	movz	x9, #0x1
	; call @longjmp(v0, v4)
	mov	x0, x19
	mov	x1, x9
	bl	_longjmp
	; trap
	brk	#1
Lnull_setjmp_block2:
	; return v2
	mov	x0, x20
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lnull_setjmp_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_null_stackrestore
	.p2align	2
_null_stackrestore:
Lnull_stackrestore_begin:
	; regalloc: v0=x9 v1=x10 v2=x20 v3=x21 v4=x9
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
	mov	x19, sp
	mov	x9, x0
Lnull_stackrestore_block0:
	; v1 = iconst.i64 0
	movz	x10, #0x0
	; v2 = inttoptr.ptr v1
	mov	x20, x10
	; v3 = intrinsic stack_save()
	mov	x21, sp
	; v4 = intrinsic stack_alloc(v0)
	add	x16, x9, #15
	and	x16, x16, #-16
	sub	sp, sp, x16
	add	x9, sp, #0
	; store v2, v4
	str	x20, [x9]
	; call @clobber()
	bl	_clobber
	; intrinsic stack_restore(v3)
	mov	sp, x21
	; return v2
	mov	x0, x20
	sub	sp, x29, #32
	ldr	x21, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lnull_stackrestore_end:
	.cfi_endproc
	.subsections_via_symbols
