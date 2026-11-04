	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x19 v1=x20 v2=x21 v3=x22 v4=x23 v5=x24 v6=x9 v7=x9 v8=x9 v9=x10 v10=x9 v11=x10 v12=x9 v13=x10 v14=x19 v15=x10 v16=x11 v17=x25 v18=x9 v19=x9 v20=x9 v21=x10 v22=x19 v23=x9 v24=x9 v25=x9 v26=x9 v27=x10 v28=x9 v29=x10 v30=x9 v31=x10 v32=x9 v33=x10 v34=x11 v35=x10 v36=x12 v37=x9 v38=x10 v39=x12 v40=x12 v41=x12 v42=x13 v43=x9 v44=x10 v45=x10 v46=x10 v47=x10 v48=x11 v49=x9
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
	str	x25, [sp, #-16]!
	.cfi_offset x25, -80
	sub	sp, sp, #32
Lmain_block0:
	; v0 = iconst.i32 0
	movz	x19, #0x0
	; v1 = stack_addr ss0
	add	x20, sp, #0
	; v2 = stack_addr ss1
	add	x21, sp, #16
	; v3 = iconst.i64 8
	movz	x22, #0x8
	; v4 = ptradd v1, v3
	add	x23, x20, #8
	; v5 = iconst.i64 16
	movz	x24, #0x10
	; v6 = iconst.i8 7
	movz	x9, #0x7
	; intrinsic memset(v1, v6, v5)
	mov	x0, x20
	mov	x1, x9
	mov	x2, x24
	bl	_memset
	; v7 = iconst.i8 1
	movz	x9, #0x1
	; intrinsic memset(v1, v7, v3)
	mov	x0, x20
	mov	x1, x9
	mov	x2, x22
	bl	_memset
	; v8 = load.i64 v1
	ldr	x9, [sp, #0]
	; v9 = iconst.i64 72340172838076673
	movz	x10, #0x101
	movk	x10, #0x101, lsl #16
	movk	x10, #0x101, lsl #32
	movk	x10, #0x101, lsl #48
	; v10 = icmp.ne v8, v9
	cmp	x9, x10
	cset	w9, ne
	; v11 = iconst.i32 1
	movz	x10, #0x1
	; v12 = select v10, v11, v0
	tst	w9, #255
	csel	w9, w10, w19, ne
	; v13 = load.i64 v4
	ldr	x10, [x23]
	; v14 = iconst.i64 506381209866536711
	movz	x19, #0x707
	movk	x19, #0x707, lsl #16
	movk	x19, #0x707, lsl #32
	movk	x19, #0x707, lsl #48
	; v15 = icmp.ne v13, v14
	cmp	x10, x19
	cset	w10, ne
	; v16 = iconst.i32 2
	movz	x11, #0x2
	; v17 = select v15, v16, v12
	tst	w10, #255
	csel	w25, w11, w9, ne
	; intrinsic memcpy(v2, v1, v5)
	mov	x0, x21
	mov	x1, x20
	mov	x2, x24
	bl	_memcpy
	; v18 = ptradd v2, v3
	add	x9, x21, #8
	; v19 = load.i64 v18
	ldr	x9, [x9]
	; v20 = icmp.ne v19, v14
	cmp	x9, x19
	cset	w9, ne
	; v21 = iconst.i32 3
	movz	x10, #0x3
	; v22 = select v20, v21, v17
	tst	w9, #255
	csel	w19, w10, w25, ne
	; v23 = iconst.i64 506097522914230528
	movz	x9, #0x100
	movk	x9, #0x302, lsl #16
	movk	x9, #0x504, lsl #32
	movk	x9, #0x706, lsl #48
	; store v23, v1
	str	x9, [sp, #0]
	; v24 = iconst.i64 1084818905618843912
	movz	x9, #0x908
	movk	x9, #0xb0a, lsl #16
	movk	x9, #0xd0c, lsl #32
	movk	x9, #0xf0e, lsl #48
	; store v24, v4
	str	x9, [x23]
	; v25 = iconst.i64 1
	; v26 = ptradd v1, v25
	add	x9, x20, #1
	; v27 = iconst.i64 15
	movz	x10, #0xf
	; intrinsic memmove(v26, v1, v27)
	mov	x0, x9
	mov	x1, x20
	mov	x2, x10
	bl	_memmove
	; v28 = load.i64 v1
	ldr	x9, [sp, #0]
	; v29 = iconst.i64 433757350076153856
	movz	x10, #0x201, lsl #16
	movk	x10, #0x403, lsl #32
	movk	x10, #0x605, lsl #48
	; v30 = icmp.ne v28, v29
	cmp	x9, x10
	cset	w9, ne
	; v31 = iconst.i32 4
	movz	x10, #0x4
	; v32 = select v30, v31, v22
	tst	w9, #255
	csel	w9, w10, w19, ne
	; v33 = load.i64 v4
	ldr	x10, [x23]
	; v34 = iconst.i64 1012478732780767239
	movz	x11, #0x807
	movk	x11, #0xa09, lsl #16
	movk	x11, #0xc0b, lsl #32
	movk	x11, #0xe0d, lsl #48
	; v35 = icmp.ne v33, v34
	cmp	x10, x11
	cset	w10, ne
	; v36 = iconst.i32 5
	movz	x12, #0x5
	; v37 = select v35, v36, v32
	tst	w10, #255
	csel	w9, w12, w9, ne
	; v38 = ptrtoint.i64 v1
	mov	x10, x20
	; v39 = ptrtoint.i64 v4
	mov	x12, x23
	; v40 = isub v39, v38
	sub	x12, x12, x10
	; v41 = icmp.ne v40, v3
	cmp	x12, #8
	cset	w12, ne
	; v42 = iconst.i32 6
	movz	x13, #0x6
	; v43 = select v41, v42, v37
	tst	w12, #255
	csel	w9, w13, w9, ne
	; v44 = iadd v38, v3
	add	x10, x10, #8
	; v45 = inttoptr.ptr v44
	; v46 = load.i64 v45
	ldr	x10, [x10]
	; v47 = icmp.ne v46, v34
	cmp	x10, x11
	cset	w10, ne
	; v48 = iconst.i32 7
	movz	x11, #0x7
	; v49 = select v47, v48, v43
	tst	w10, #255
	csel	w9, w11, w9, ne
	; return v49
	mov	x0, x9
	sub	sp, x29, #64
	ldr	x25, [sp], #16
	ldp	x23, x24, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
