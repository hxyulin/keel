	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_scalar_read8
	.p2align	2
_scalar_read8:
Lscalar_read8_begin:
	; regalloc: v0=x9 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lscalar_read8_block0:
	; v1 = iconst.i64 8
	; v2 = ptradd v0, v1
	; v3 = load.i8 v2
	ldrb	w9, [x9, #8]
	; v4 = sext.i64 v3
	sxtb	x9, w9
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lscalar_read8_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_scalar_write16
	.p2align	2
_scalar_write16:
Lscalar_write16_begin:
	; regalloc: v0=x9 v1=x10 v6=x9 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lscalar_write16_block0:
	; v2 = iconst.i64 14
	; v3 = ptradd v0, v2
	; store v1, v3
	strh	w10, [x9, #14]
	; v12 = iconst.i64 14
	; v13 = ptradd v0, v12
	; v6 = load.i16 v13
	ldrh	w9, [x9, #14]
	; v7 = uext.i64 v6
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lscalar_write16_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_scalar_call
	.p2align	2
_scalar_call:
Lscalar_call_begin:
	; regalloc: v0=x19 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x19, x0
Lscalar_call_block0:
	; v1 = iconst.i64 8
	; v2 = ptradd v0, v1
	; call @scalar_touch(v0)
	mov	x0, x19
	bl	_scalar_touch
	; v3 = load.i8 v2
	ldrb	w9, [x19, #8]
	; v4 = sext.i64 v3
	sxtb	x9, w9
	; return v4
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lscalar_call_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_scalar_pressure
	.p2align	2
_scalar_pressure:
Lscalar_pressure_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x14 v13=x9 v14=x10 v15=x11 v16=x12 v17=x13 v18=x14 v19=x9 v20=x10 v21=x11 v22=x12 v23=x13 v24=x14 v25=x9 v26=x10 v27=x11 v28=x9 v29=x9
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
Lscalar_pressure_block0:
	; v6 = iconst.i64 8
	; v7 = ptradd v0, v6
	; v8 = ptradd v1, v6
	; v9 = ptradd v2, v6
	; v10 = ptradd v3, v6
	; v11 = ptradd v4, v6
	; v12 = ptradd v5, v6
	; v13 = load.i8 v7
	ldrb	w9, [x9, #8]
	; v14 = load.i8 v8
	ldrb	w10, [x10, #8]
	; v15 = load.i8 v9
	ldrb	w11, [x11, #8]
	; v16 = load.i8 v10
	ldrb	w12, [x12, #8]
	; v17 = load.i8 v11
	ldrb	w13, [x13, #8]
	; v18 = load.i8 v12
	ldrb	w14, [x14, #8]
	; v19 = uext.i64 v13
	; v20 = uext.i64 v14
	; v21 = uext.i64 v15
	; v22 = uext.i64 v16
	; v23 = uext.i64 v17
	; v24 = uext.i64 v18
	; v25 = iadd v19, v20
	add	x9, x9, x10
	; v26 = iadd v21, v22
	add	x10, x11, x12
	; v27 = iadd v23, v24
	add	x11, x13, x14
	; v28 = iadd v25, v26
	add	x9, x9, x10
	; v29 = iadd v27, v28
	add	x9, x11, x9
	; return v29
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lscalar_pressure_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_scalar_bool
	.p2align	2
_scalar_bool:
Lscalar_bool_begin:
	; regalloc: v0=x9 v1=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lscalar_bool_block0:
	; v5 = icmp.sge v0, v1
	; brif v5, block1, block2
	cmp	x9, x10
	b.ge	Lscalar_bool_block1
Lscalar_bool_block2:
	; return v1
	mov	x0, x10
	ldp	x29, x30, [sp], #16
	ret
Lscalar_bool_block1:
	; return v0
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lscalar_bool_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_scalar_bool_value
	.p2align	2
_scalar_bool_value:
Lscalar_bool_value_begin:
	; regalloc: v0=x9 v6=x9 v7=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lscalar_bool_value_block0:
	; v1 = iconst.i32 15
	; v2 = and v0, v1
	; v3 = iconst.i32 0
	; v6 = icmp.ne v2, v3
	tst	w9, #15
	cset	w9, ne
	; v7 = uext.i32 v6
	; return v7
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lscalar_bool_value_end:
	.cfi_endproc
	.subsections_via_symbols
