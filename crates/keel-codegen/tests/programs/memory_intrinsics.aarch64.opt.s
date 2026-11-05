	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v1=x9 v2=x10 v4=x11 v12=x13 v19=x10 v21=x14 v22=x10 v23=x12 v24=x13 v26=x13 v28=x12 v29=x13 v31=x14 v32=x10 v33=x12 v34=x13 v36=x14 v37=x10 v38=x9 v39=x11 v40=x11 v42=x12 v43=x10 v44=x9 v45=x9 v46=x9 v48=x11 v49=x9 v52=x12 v57=x13 v58=d19 v62=x14 v65=x15 v68=x19
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	sub	sp, sp, #48
Lmain_block0:
	; v1 = stack_addr ss0
	add	x9, sp, #16
	; v2 = stack_addr ss1
	add	x10, sp, #32
	; v3 = iconst.i64 8
	; v4 = ptradd v1, v3
	add	x11, x9, #8
	; v52 = iconst.i64 506381209866536711
	movz	x12, #0x707
	movk	x12, #0x707, lsl #16
	movk	x12, #0x707, lsl #32
	movk	x12, #0x707, lsl #48
	; store notrap v52, v1
	str	x12, [sp, #16]
	; store notrap v52, v4
	str	x12, [x11]
	; v57 = iconst.i64 72340172838076673
	movz	x13, #0x101
	movk	x13, #0x101, lsl #16
	movk	x13, #0x101, lsl #32
	movk	x13, #0x101, lsl #48
	; store notrap v57, v1
	str	x13, [sp, #16]
	; v12 = iconst.i32 0
	movz	w13, #0x0
	; v58 = load.i8x16 notrap v1
	ldr	q19, [sp, #16]
	; store notrap v58, v2
	str	q19, [sp, #32]
	; v18 = ptradd v2, v3
	; v19 = load.i64 notrap v18
	ldr	x10, [x10, #8]
	; v20 = icmp.ne v19, v52
	; v21 = iconst.i32 3
	movz	w14, #0x3
	; v22 = select v20, v21, v12
	cmp	x10, x12
	csel	w10, w14, w13, ne
	; v23 = iconst.i64 506097522914230528
	movz	x12, #0x100
	movk	x12, #0x302, lsl #16
	movk	x12, #0x504, lsl #32
	movk	x12, #0x706, lsl #48
	; store notrap v23, v1
	str	x12, [sp, #16]
	; v24 = iconst.i64 1084818905618843912
	movz	x13, #0x908
	movk	x13, #0xb0a, lsl #16
	movk	x13, #0xd0c, lsl #32
	movk	x13, #0xf0e, lsl #48
	; store notrap v24, v4
	str	x13, [x11]
	; v25 = iconst.i64 1
	; v26 = ptradd v1, v25
	add	x13, x9, #1
	; v62 = load.i32 notrap v4
	ldr	w14, [x11]
	; v63 = iconst.i64 12
	; v64 = ptradd v1, v63
	; v65 = load.i16 notrap v64
	ldrh	w15, [x9, #12]
	; v66 = iconst.i64 14
	; v67 = ptradd v1, v66
	; v68 = load.i8 notrap v67
	ldrb	w19, [x9, #14]
	; store notrap v23, v26
	str	x12, [x13]
	; v70 = ptradd v26, v3
	; store notrap v62, v70
	str	w14, [x13, #8]
	; v72 = ptradd v26, v63
	; store notrap v65, v72
	strh	w15, [x13, #12]
	; v74 = ptradd v26, v66
	; store notrap v68, v74
	strb	w19, [x13, #14]
	; v28 = load.i64 notrap v1
	ldr	x12, [sp, #16]
	; v29 = iconst.i64 433757350076153856
	movz	x13, #0x201, lsl #16
	movk	x13, #0x403, lsl #32
	movk	x13, #0x605, lsl #48
	; v30 = icmp.ne v28, v29
	; v31 = iconst.i32 4
	movz	w14, #0x4
	; v32 = select v30, v31, v22
	cmp	x12, x13
	csel	w10, w14, w10, ne
	; v33 = load.i64 notrap v4
	ldr	x12, [x11]
	; v34 = iconst.i64 1012478732780767239
	movz	x13, #0x807
	movk	x13, #0xa09, lsl #16
	movk	x13, #0xc0b, lsl #32
	movk	x13, #0xe0d, lsl #48
	; v35 = icmp.ne v33, v34
	; v36 = iconst.i32 5
	movz	w14, #0x5
	; v37 = select v35, v36, v32
	cmp	x12, x13
	csel	w10, w14, w10, ne
	; v38 = ptrtoint.i64 v1
	; v39 = ptrtoint.i64 v4
	; v40 = isub v39, v38
	sub	x11, x11, x9
	; v41 = icmp.ne v40, v3
	; v42 = iconst.i32 6
	movz	w12, #0x6
	; v43 = select v41, v42, v37
	cmp	x11, #8
	csel	w10, w12, w10, ne
	; v44 = iadd v38, v3
	add	x9, x9, #8
	; v45 = inttoptr.ptr v44
	; v46 = load.i64 v45
	ldr	x9, [x9]
	; v47 = icmp.ne v46, v34
	; v48 = iconst.i32 7
	movz	w11, #0x7
	; v49 = select v47, v48, v43
	cmp	x9, x13
	csel	w9, w11, w10, ne
	; return v49
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
