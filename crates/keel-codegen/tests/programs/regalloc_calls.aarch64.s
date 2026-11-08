	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_across
	.p2align	2
_across:
Lacross_begin:
	; regalloc: v0=x19 v1=x9 v2=x20 v3=x9 v4=x21 v5=x22 v6=x9 v7=x10 v8=x23 v9=x9 v10=x10 v11=x10 v12=x9 v13=x9
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
Lacross_block0:
	; v1 = iconst.i64 3
	movz	x9, #0x3
	; v2 = imul v0, v1
	mul	x20, x19, x9
	; v3 = iconst.i64 5
	; v4 = iadd v0, v3
	add	x21, x19, #5
	; v5 = xor v2, v4
	eor	x22, x20, x21
	; v6 = trunc.i32 v0
	mov	x9, x19
	; v7 = iconst.i32 7
	; v8 = iadd v6, v7
	add	w23, w9, #7
	; call @clobber()
	bl	_clobber
	; v9 = sext.i64 v8
	sxtw	x9, w23
	; v10 = iadd v2, v4
	add	x10, x20, x21
	; v11 = iadd v10, v5
	add	x10, x10, x22
	; v12 = iadd v11, v9
	add	x9, x10, x9
	; v13 = iadd v12, v0
	add	x9, x9, x19
	; return v13
	mov	x0, x9
	sub	sp, x29, #48
	ldr	x23, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lacross_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_nested
	.p2align	2
_nested:
Lnested_begin:
	; regalloc: v0=x19 v1=x9 v2=x20 v3=x9 v4=x21 v5=x22 v6=x9 v7=x9 v8=x9 v9=x9 v10=x9
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
	mov	x19, x0
Lnested_block0:
	; v1 = iconst.i64 1
	; v2 = iadd v0, v1
	add	x20, x19, #1
	; v3 = iconst.i64 2
	movz	x9, #0x2
	; v4 = imul v0, v3
	mul	x21, x19, x9
	; v5 = call @across(v0)
	mov	x0, x19
	bl	_across
	mov	x22, x0
	; v6 = call @across(v2)
	mov	x0, x20
	bl	_across
	mov	x9, x0
	; v7 = iadd v5, v6
	add	x9, x22, x9
	; v8 = iadd v7, v2
	add	x9, x9, x20
	; v9 = iadd v8, v4
	add	x9, x9, x21
	; v10 = iadd v9, v0
	add	x9, x9, x19
	; return v10
	mov	x0, x9
	sub	sp, x29, #32
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lnested_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_call_loop
	.p2align	2
_call_loop:
Lcall_loop_begin:
	; regalloc: v0=x19 v1=x9 v2=x10 v3=x11 v4=x20 v5=x21 v6=x22 v8=x9 v9=x9 v10=x9 v11=x10 v12=x10
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
	mov	x19, x0
	mov	x9, x1
Lcall_loop_block0:
	; v2 = iconst.i64 0
	movz	x10, #0x0
	; v3 = iconst.i64 10
	movz	x11, #0xa
	; v4 = imul v1, v3
	mul	x20, x9, x11
	; jump block1(v2, v2)
	mov	x21, x10
	mov	x22, x10
Lcall_loop_block1:
	; v7 = icmp.ult v5, v0
	; brif v7, block2, block3
	cmp	x21, x19
	b.hs	Lcall_loop_block3
Lcall_loop_block2:
	; v8 = call @twice(v5)
	mov	x0, x21
	bl	_twice
	mov	x9, x0
	; v9 = iadd v6, v8
	add	x9, x22, x9
	; v10 = iadd v9, v4
	add	x9, x9, x20
	; v11 = iconst.i64 1
	; v12 = iadd v5, v11
	add	x10, x21, #1
	; jump block1(v12, v10)
	mov	x21, x10
	mov	x22, x9
	b	Lcall_loop_block1
Lcall_loop_block3:
	; return v6
	mov	x0, x22
	sub	sp, x29, #32
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lcall_loop_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_memset_loop
	.p2align	2
_memset_loop:
Lmemset_loop_begin:
	; regalloc: v0=x19 v1=x20 v2=x21 v3=x9 v4=x22 v5=x23 v7=x9 v8=x9 v9=x9 v10=x10 v11=x10
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
	sub	sp, sp, #16
	mov	x19, x0
Lmemset_loop_block0:
	; v1 = stack_addr ss0
	add	x20, sp, #0
	; v2 = iconst.i64 8
	movz	x21, #0x8
	; v3 = iconst.i64 0
	movz	x9, #0x0
	; jump block1(v3, v3)
	mov	x22, x9
	mov	x23, x9
Lmemset_loop_block1:
	; v6 = icmp.ult v4, v0
	; brif v6, block2, block3
	cmp	x22, x19
	b.hs	Lmemset_loop_block3
Lmemset_loop_block2:
	; v7 = trunc.i8 v4
	mov	x9, x22
	; intrinsic memset(v1, v7, v2)
	mov	x0, x20
	mov	x1, x9
	mov	x2, x21
	bl	_memset
	; v8 = load.i64 v1
	ldr	x9, [sp, #0]
	; v9 = iadd v5, v8
	add	x9, x23, x9
	; v10 = iconst.i64 1
	; v11 = iadd v4, v10
	add	x10, x22, #1
	; jump block1(v11, v9)
	mov	x22, x10
	mov	x23, x9
	b	Lmemset_loop_block1
Lmemset_loop_block3:
	; return v5
	mov	x0, x23
	sub	sp, x29, #48
	ldr	x23, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lmemset_loop_end:
	.cfi_endproc
	.subsections_via_symbols
