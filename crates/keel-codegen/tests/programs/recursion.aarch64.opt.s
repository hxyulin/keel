	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v1=x9 v2=x9 v4=x9 v6=x19 v7=x20 v8=x9 v9=x10 v11=x11 v12=x19 v14=x9 v15=x10 v17=x11 v18=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
Lmain_block0:
	; v1 = iconst.i64 10
	movz	x9, #0xa
	; v2 = call @fib(v1)
	mov	x0, x9
	bl	_fib
	mov	x9, x0
	; v3 = iconst.i64 55
	; v4 = icmp.ne v2, v3
	cmp	x9, #55
	cset	w9, ne
	; v6 = uext.i32 v4
	mov	w19, w9
	; v7 = iconst.i64 20
	movz	x20, #0x14
	; v8 = call @fib(v7)
	mov	x0, x20
	bl	_fib
	mov	x9, x0
	; v9 = iconst.i64 6765
	movz	x10, #0x1a6d
	; v10 = icmp.ne v8, v9
	; v11 = iconst.i32 2
	movz	x11, #0x2
	; v12 = select v10, v11, v6
	cmp	x9, x10
	csel	w19, w11, w19, ne
	; v14 = call @fact(v7)
	mov	x0, x20
	bl	_fact
	mov	x9, x0
	; v15 = iconst.i64 2432902008176640000
	movz	x10, #0x82b4, lsl #16
	movk	x10, #0x677c, lsl #32
	movk	x10, #0x21c3, lsl #48
	; v16 = icmp.ne v14, v15
	; v17 = iconst.i32 3
	movz	x11, #0x3
	; v18 = select v16, v17, v12
	cmp	x9, x10
	csel	w9, w11, w19, ne
	; return v18
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lmain_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_fib
	.p2align	2
_fib:
Lfib_begin:
	; regalloc: v0=x19 v4=x9 v5=x20 v6=x9 v7=x9 v8=x9 v9=x9
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
Lfib_block0:
	; v1 = iconst.i64 2
	; v2 = icmp.ult v0, v1
	; brif v2, block2(v0), block1
	cmp	x19, #2
	b.hs	Lfib_block1
	mov	x9, x19
	b	Lfib_block2
Lfib_block1:
	; v3 = iconst.i64 1
	; v4 = isub v0, v3
	sub	x9, x19, #1
	; v5 = call @fib(v4)
	mov	x0, x9
	bl	_fib
	mov	x20, x0
	; v12 = iconst.i64 2
	; v6 = isub v0, v12
	sub	x9, x19, #2
	; v7 = call @fib(v6)
	mov	x0, x9
	bl	_fib
	mov	x9, x0
	; v8 = iadd v5, v7
	add	x9, x20, x9
	; jump block2(v8)
Lfib_block2:
	; return v9
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lfib_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_fact
	.p2align	2
_fact:
Lfact_begin:
	; regalloc: v0=x19 v3=x9 v4=x9 v5=x9 v6=x9 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x19, x0
Lfact_block0:
	; v1 = iconst.i64 0
	; v2 = icmp.eq v0, v1
	; v3 = iconst.i64 1
	movz	x9, #0x1
	; brif v2, block2(v3), block1
	cmp	x19, #0
	b.eq	Lfact_block2
Lfact_block1:
	; v10 = iconst.i64 1
	; v4 = isub v0, v10
	sub	x9, x19, #1
	; v5 = call @fact(v4)
	mov	x0, x9
	bl	_fact
	mov	x9, x0
	; v6 = imul v0, v5
	mul	x9, x19, x9
	; jump block2(v6)
Lfact_block2:
	; return v7
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lfact_end:
	.cfi_endproc
	.subsections_via_symbols
