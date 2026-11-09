	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v5=x12 v6=x12 v7=x12 v8=x13 v9=x13 v10=x13 v11=x12 v12=x11 v13=x10 v14=x11 v15=x12 v17=x12 v18=x12 v19=x12 v20=x12 v21=x12 v22=x11 v23=x12 v24=x10 v25=x9 v26=x10 v28=x10 v29=x9 v30=x10 v31=x11 v32=x12 v33=x12 v34=x13 v37=x13 v38=x9 v39=x11 v40=x12 v43=x12 v44=x9 v45=x11 v46=x11 v47=x12 v48=x11 v49=x12 v50=x9 v51=x11 v52=x11 v53=x11 v54=x12 v55=x11 v56=x12 v57=x9 v58=x11 v59=x12 v60=x13 v61=x10 v62=x14 v63=x10 v64=x10 v65=x10 v66=x11 v67=x9 v68=x10 v69=x11 v70=x10 v71=x11 v72=x10 v74=x12 v75=x9 v76=x10 v77=x11 v78=x10 v79=x11 v80=x10 v81=x11 v83=x12 v84=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #1, lsl #12
	sub	sp, sp, #976
Lmain_block0:
	; v0 = stack_addr ss1
	add	x9, sp, #904
	add	x9, x9, #1, lsl #12
	; v1 = iconst.i64 0
	movz	x10, #0x0
	; jump block1(v1)
	mov	x11, x10
Lmain_block1:
	; v3 = iconst.i64 10
	; v4 = icmp.ult v2, v3
	; brif v4, block2, block3(v1, v1)
	cmp	x11, #10
	b.lo	Lmain_block2
	mov	x11, x10
Lmain_block3:
	; v15 = iconst.i64 10
	; v16 = icmp.ult v13, v15
	; brif v16, block4, block5
	cmp	x10, #10
	b.hs	Lmain_block5
Lmain_block4:
	; v17 = iconst.i64 4
	movz	x12, #0x4
	; v18 = imul v13, v17
	mul	x12, x10, x12
	; v19 = ptradd v0, v18
	add	x12, x9, x12
	; v20 = load.i32 v19
	ldr	w12, [x12]
	; v21 = sext.i64 v20
	sxtw	x12, w12
	; v22 = iadd v14, v21
	add	x11, x11, x12
	; v23 = iconst.i64 1
	; v24 = iadd v13, v23
	add	x10, x10, #1
	; jump block3(v24, v22)
	b	Lmain_block3
Lmain_block5:
	; v25 = iconst.i32 0
	movz	w9, #0x0
	; v26 = iconst.i64 135
	; v27 = icmp.ne v14, v26
	; v28 = iconst.i32 1
	movz	w10, #0x1
	; v29 = select v27, v28, v25
	cmp	x11, #135
	csel	w9, w10, w9, ne
	; v30 = stack_addr ss3
	add	x10, sp, #952
	add	x10, x10, #1, lsl #12
	; v31 = stack_addr ss4
	add	x11, sp, #960
	add	x11, x11, #1, lsl #12
	; v32 = iconst.i64 0
	; v33 = ptrtoint.i64 v30
	mov	x12, x10
	; v34 = iconst.i64 7
	; v35 = and v33, v34
	; v36 = icmp.ne v35, v32
	; v37 = iconst.i32 2
	movz	w13, #0x2
	; v38 = select v36, v37, v29
	tst	x12, #7
	csel	w9, w13, w9, ne
	; v39 = ptrtoint.i64 v31
	; v40 = iconst.i64 15
	; v41 = and v39, v40
	; v42 = icmp.ne v41, v32
	; v43 = iconst.i32 3
	movz	w12, #0x3
	; v44 = select v42, v43, v38
	tst	x11, #15
	csel	w9, w12, w9, ne
	; v45 = iconst.i32 287454020
	movz	w11, #0x3344
	movk	w11, #0x1122, lsl #16
	; store v45, v30
	str	w11, [sp, #5048]
	; v46 = load.i8 v30
	ldrb	w11, [x10]
	; v47 = iconst.i8 68
	; v48 = icmp.ne v46, v47
	cmp	w11, #68
	cset	w11, ne
	; v49 = iconst.i32 4
	movz	w12, #0x4
	; v50 = select v48, v49, v44
	tst	w11, #255
	csel	w9, w12, w9, ne
	; v51 = iconst.i64 2
	; v52 = ptradd v30, v51
	add	x11, x10, #2
	; v53 = load.i16 v52
	ldrh	w11, [x11]
	; v54 = iconst.i16 4386
	movz	w12, #0x1122
	; v55 = icmp.ne v53, v54
	uxth	w17, w12
	cmp	w11, w17
	cset	w11, ne
	; v56 = iconst.i32 5
	movz	w12, #0x5
	; v57 = select v55, v56, v50
	tst	w11, #255
	csel	w9, w12, w9, ne
	; v58 = iconst.i8 127
	movz	w11, #0x7f
	; v59 = stack_addr ss2
	add	x12, sp, #944
	add	x12, x12, #1, lsl #12
	; store v58, v59
	strb	w11, [x12]
	; v60 = iconst.i64 -1
	movn	x13, #0x0
	; store v60, v30
	str	x13, [sp, #5048]
	; v61 = stack_addr ss4
	add	x10, sp, #960
	add	x10, x10, #1, lsl #12
	; store v60, v61
	str	x13, [sp, #5056]
	; v62 = iconst.i64 8
	; v63 = ptradd v61, v62
	add	x10, x10, #8
	; store v60, v63
	str	x13, [x10]
	; v64 = load.i8 v59
	ldrb	w10, [x12]
	; v65 = icmp.ne v64, v58
	cmp	w10, #127
	cset	w10, ne
	; v66 = iconst.i32 6
	movz	w11, #0x6
	; v67 = select v65, v66, v57
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v68 = stack_addr ss0
	add	x10, sp, #0
	; v69 = iconst.i64 4992
	movz	x11, #0x1380
	; v70 = ptradd v68, v69
	add	x10, x10, x11
	; v71 = iconst.i64 123456789
	movz	x11, #0xcd15
	movk	x11, #0x75b, lsl #16
	; store v71, v70
	str	x11, [x10]
	; v72 = load.i64 v70
	ldr	x10, [x10]
	; v73 = icmp.ne v72, v71
	; v74 = iconst.i32 7
	movz	w12, #0x7
	; v75 = select v73, v74, v67
	cmp	x10, x11
	csel	w9, w12, w9, ne
	; v76 = stack_addr ss0
	add	x10, sp, #0
	; v77 = stack_addr ss1
	add	x11, sp, #904
	add	x11, x11, #1, lsl #12
	; v78 = ptrtoint.i64 v76
	; v79 = ptrtoint.i64 v77
	; v80 = isub v79, v78
	sub	x10, x11, x10
	; v81 = iconst.i64 5000
	movz	x11, #0x1388
	; v82 = icmp.ult v80, v81
	; v83 = iconst.i32 8
	movz	w12, #0x8
	; v84 = select v82, v83, v75
	cmp	x10, x11
	csel	w9, w12, w9, lo
	; return v84
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lmain_block2:
	; v5 = iconst.i64 4
	movz	x12, #0x4
	; v6 = imul v2, v5
	mul	x12, x11, x12
	; v7 = ptradd v0, v6
	add	x12, x9, x12
	; v8 = iconst.i64 3
	movz	x13, #0x3
	; v9 = imul v2, v8
	mul	x13, x11, x13
	; v10 = trunc.i32 v9
	; store v10, v7
	str	w13, [x12]
	; v11 = iconst.i64 1
	; v12 = iadd v2, v11
	add	x11, x11, #1
	; jump block1(v12)
	b	Lmain_block1
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
