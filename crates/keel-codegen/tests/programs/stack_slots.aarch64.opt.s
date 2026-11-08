	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x11 v1=x9 v8=x11 v9=x11 v10=x11 v12=x9 v13=x9 v14=x10 v20=x11 v21=x11 v22=x10 v24=x9 v27=x9 v29=x9 v30=x10 v31=x11 v33=x12 v35=x12 v37=x13 v38=x9 v39=x12 v41=x12 v43=x13 v44=x9 v45=x12 v46=x12 v48=x12 v49=x13 v50=x9 v53=x12 v54=x13 v55=x12 v56=x13 v57=x9 v60=x12 v68=x10 v71=x11 v78=x10 v79=x11 v80=x10 v81=x11 v83=x12 v84=x9 v89=x11 v90=x10 v95=x9 v98=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #1, lsl #12
	sub	sp, sp, #976
Lmain_block0:
	; v1 = iconst.i64 0
	movz	x9, #0x0
	; jump block2(v1)
Lmain_block2:
	; v85 = iconst.i64 2
	; v6 = shl v95, v85
	; v90 = stack_addr ss1
	add	x10, sp, #904
	add	x10, x10, #1, lsl #12
	; v7 = ptradd v90, v6
	; v8 = iconst.i64 3
	movz	x11, #0x3
	; v9 = imul v95, v8
	mul	x11, x9, x11
	; v10 = trunc.i32 v9
	; store v10, v7
	str	w11, [x10, x9, lsl #2]
	; v11 = iconst.i64 1
	; v12 = iadd v95, v11
	add	x9, x9, #1
	; v96 = iconst.i64 10
	; v97 = icmp.ult v12, v96
	; v98 = iconst.i64 0
	movz	x10, #0x0
	; brif v97, block2(v12), block3(v98, v98)
	cmp	x9, #10
	b.lo	Lmain_block2
	mov	x9, x10
Lmain_block3:
	; v93 = iconst.i64 10
	; v16 = icmp.ult v13, v93
	; brif v16, block4, block5
	cmp	x9, #10
	b.hs	Lmain_block5
Lmain_block4:
	; v86 = iconst.i64 2
	; v18 = shl v13, v86
	; v89 = stack_addr ss1
	add	x11, sp, #904
	add	x11, x11, #1, lsl #12
	; v19 = ptradd v89, v18
	; v20 = load.i32 v19
	ldr	w11, [x11, x9, lsl #2]
	; v21 = sext.i64 v20
	sxtw	x11, w11
	; v22 = iadd v14, v21
	add	x10, x10, x11
	; v23 = iconst.i64 1
	; v24 = iadd v13, v23
	add	x9, x9, #1
	; jump block3(v24, v22)
	b	Lmain_block3
Lmain_block5:
	; v26 = iconst.i64 135
	; v27 = icmp.ne v14, v26
	cmp	x10, #135
	cset	w9, ne
	; v29 = uext.i32 v27
	; v30 = stack_addr ss3
	add	x10, sp, #944
	add	x10, x10, #1, lsl #12
	; v31 = stack_addr ss4
	add	x11, sp, #960
	add	x11, x11, #1, lsl #12
	; v33 = ptrtoint.i64 v30
	mov	x12, x10
	; v34 = iconst.i64 7
	; v35 = and v33, v34
	and	x12, x12, #0x7
	; v37 = iconst.i32 2
	movz	w13, #0x2
	; v38 = select v35, v37, v29
	cmp	x12, #0
	csel	w9, w13, w9, ne
	; v39 = ptrtoint.i64 v31
	mov	x12, x11
	; v40 = iconst.i64 15
	; v41 = and v39, v40
	and	x12, x12, #0xf
	; v43 = iconst.i32 3
	movz	w13, #0x3
	; v44 = select v41, v43, v38
	cmp	x12, #0
	csel	w9, w13, w9, ne
	; v45 = iconst.i32 287454020
	movz	w12, #0x3344
	movk	w12, #0x1122, lsl #16
	; store notrap v45, v30
	str	w12, [sp, #5040]
	; v46 = load.i8 notrap v30
	ldrb	w12, [x10]
	; v47 = iconst.i8 68
	; v48 = icmp.ne v46, v47
	cmp	w12, #68
	cset	w12, ne
	; v49 = iconst.i32 4
	movz	w13, #0x4
	; v50 = select v48, v49, v44
	tst	w12, #255
	csel	w9, w13, w9, ne
	; v51 = iconst.i64 2
	; v52 = ptradd v30, v51
	; v53 = load.i16 notrap v52
	ldrh	w12, [x10, #2]
	; v54 = iconst.i16 4386
	movz	w13, #0x1122
	; v55 = icmp.ne v53, v54
	uxth	w17, w13
	cmp	w12, w17
	cset	w12, ne
	; v56 = iconst.i32 5
	movz	w13, #0x5
	; v57 = select v55, v56, v50
	tst	w12, #255
	csel	w9, w13, w9, ne
	; v60 = iconst.i64 -1
	movn	x12, #0x0
	; store notrap v60, v30
	str	x12, [sp, #5040]
	; store notrap v60, v31
	str	x12, [sp, #5056]
	; v62 = iconst.i64 8
	; v63 = ptradd v31, v62
	; store notrap v60, v63
	str	x12, [x11, #8]
	; v68 = stack_addr ss0
	add	x10, sp, #0
	; v69 = iconst.i64 4992
	; v70 = ptradd v68, v69
	; v71 = iconst.i64 123456789
	movz	x11, #0xcd15
	movk	x11, #0x75b, lsl #16
	; store notrap v71, v70
	str	x11, [x10, #4992]
	; v78 = ptrtoint.i64 v68
	; v0 = stack_addr ss1
	add	x11, sp, #904
	add	x11, x11, #1, lsl #12
	; v79 = ptrtoint.i64 v0
	; v80 = isub v79, v78
	sub	x10, x11, x10
	; v81 = iconst.i64 5000
	movz	x11, #0x1388
	; v82 = icmp.ult v80, v81
	; v83 = iconst.i32 8
	movz	w12, #0x8
	; v84 = select v82, v83, v57
	cmp	x10, x11
	csel	w9, w12, w9, lo
	; return v84
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
