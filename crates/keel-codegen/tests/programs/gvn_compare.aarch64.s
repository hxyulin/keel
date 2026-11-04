	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_compare
	.p2align	2
_compare:
Lcompare_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x9 v6=x10 v7=x11 v8=x12 v9=x9 v10=x10 v11=x9 v12=x9 v13=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lcompare_block0:
	; v2 = icmp.slt v0, v1
	cmp	x9, x10
	cset	w11, lt
	; v3 = icmp.sgt v1, v0
	cmp	x10, x9
	cset	w12, gt
	; v4 = icmp.ult v0, v1
	cmp	x9, x10
	cset	w13, lo
	; v5 = icmp.ugt v1, v0
	cmp	x10, x9
	cset	w9, hi
	; v6 = uext.i64 v2
	mov	w10, w11
	; v7 = uext.i64 v3
	mov	w11, w12
	; v8 = uext.i64 v4
	mov	w12, w13
	; v9 = uext.i64 v5
	; v10 = iadd v6, v7
	add	x10, x10, x11
	; v11 = iadd v8, v9
	add	x9, x12, x9
	; v12 = iadd v11, v11
	add	x9, x9, x9
	; v13 = iadd v10, v12
	add	x9, x10, x9
	; return v13
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lcompare_end:
	.cfi_endproc
	.subsections_via_symbols
