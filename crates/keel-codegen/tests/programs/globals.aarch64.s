	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x19 v1=x20 v2=x9 v3=x9 v4=x9 v5=x9 v6=x9 v7=x9 v8=x9 v9=x10 v10=x20 v11=x10 v12=x9 v13=x10 v15=x10 v16=x19 v17=x21 v18=x22 v19=x9 v20=x9 v21=x23 v22=x24 v23=x9 v24=x25 v26=x10 v27=x19 v28=x9 v29=x9 v30=x10 v32=x10 v33=x9 v34=x10 v35=x11 v37=x12 v38=x19 v39=x9 v41=x10 v42=x19 v43=x9 v44=x10 v45=x9 v47=x10 v48=x19 v49=x23 v50=x24 v51=x9 v53=x10 v54=x19 v55=x9 v56=x10 v57=x9 v59=x10 v60=x9 v61=x10 v62=x11 v63=x10 v64=x11 v65=x12 v66=x10 v67=x10 v69=x11 v70=x19 v71=x9 v72=x10 v74=x10 v75=x9 v76=x10 v77=x11 v78=x11 v79=x11 v80=x12 v82=x12 v83=x9 v84=x11 v85=x10 v86=x10 v87=x10 v88=x11 v90=x11 v91=x19 v92=x25 v93=x9 v94=x10 v95=x9 v97=x10 v98=x19 v99=x9 v100=x9 v102=x10 v103=x19 v104=x9 v105=x9 v106=x10 v108=x10 v109=x19 v110=x9 v111=x10 v113=x10 v114=x9
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
Lmain_block0:
	; v0 = iconst.i32 0
	movz	w19, #0x0
	; v1 = symbol_addr @greeting
	adrp	x20, _greeting@PAGE
	add	x20, x20, _greeting@PAGEOFF
	; v2 = call @puts(v1)
	mov	x0, x20
	bl	_puts
	mov	x9, x0
	; v3 = symbol_addr @tail
	adrp	x9, _tail@PAGE
	add	x9, x9, _tail@PAGEOFF
	; v4 = load.ptr v3
	ldr	x9, [x9]
	; v5 = call @puts(v4)
	mov	x0, x9
	bl	_puts
	mov	x9, x0
	; v6 = symbol_addr @puts
	adrp	x9, _puts@GOTPAGE
	ldr	x9, [x9, _puts@GOTPAGEOFF]
	; v7 = call_indirect v6(v1) : (ptr) -> i32 cc(c)
	mov	x0, x20
	blr	x9
	mov	x9, x0
	; v8 = symbol_addr @counter
	adrp	x9, _counter@PAGE
	add	x9, x9, _counter@PAGEOFF
	; v9 = load.i64 v8
	ldr	x10, [x9]
	; v10 = iconst.i64 2
	movz	x20, #0x2
	; v11 = iadd v9, v10
	add	x10, x10, #2
	; store v11, v8
	str	x10, [x9]
	; v12 = call @c_read_counter()
	bl	_c_read_counter
	mov	x9, x0
	; v13 = iconst.i64 42
	; v14 = icmp.ne v12, v13
	; v15 = iconst.i32 3
	movz	w10, #0x3
	; v16 = select v14, v15, v0
	cmp	x9, #42
	csel	w19, w10, w19, ne
	; v17 = symbol_addr @ops
	adrp	x21, _ops@PAGE
	add	x21, x21, _ops@PAGEOFF
	; v18 = iconst.i64 8
	movz	x22, #0x8
	; v19 = ptradd v17, v18
	add	x9, x21, #8
	; v20 = load.ptr v19
	ldr	x9, [x9]
	; v21 = iconst.i64 10
	movz	x23, #0xa
	; v22 = iconst.i64 3
	movz	x24, #0x3
	; v23 = call_indirect v20(v21, v22) : (i64, i64) -> i64 cc(c)
	mov	x0, x23
	mov	x1, x24
	blr	x9
	mov	x9, x0
	; v24 = iconst.i64 7
	movz	x25, #0x7
	; v25 = icmp.ne v23, v24
	; v26 = iconst.i32 4
	movz	w10, #0x4
	; v27 = select v25, v26, v16
	cmp	x9, #7
	csel	w19, w10, w19, ne
	; v28 = load.ptr v17
	ldr	x9, [x21]
	; v29 = call_indirect v28(v21, v22) : (i64, i64) -> i64 cc(c)
	mov	x0, x23
	mov	x1, x24
	blr	x9
	mov	x9, x0
	; v30 = iconst.i64 13
	; v31 = icmp.ne v29, v30
	; v32 = iconst.i32 5
	movz	w10, #0x5
	; v33 = select v31, v32, v27
	cmp	x9, #13
	csel	w9, w10, w19, ne
	; v34 = symbol_addr @c_counter
	adrp	x10, _c_counter@GOTPAGE
	ldr	x10, [x10, _c_counter@GOTPAGEOFF]
	; v35 = load.i64 v34
	ldr	x11, [x10]
	; v36 = icmp.ne v35, v24
	; v37 = iconst.i32 6
	movz	w12, #0x6
	; v38 = select v36, v37, v33
	cmp	x11, #7
	csel	w19, w12, w9, ne
	; store v18, v34
	str	x22, [x10]
	; v39 = call @c_read_c_counter()
	bl	_c_read_c_counter
	mov	x9, x0
	; v40 = icmp.ne v39, v18
	; v41 = iconst.i32 7
	movz	w10, #0x7
	; v42 = select v40, v41, v38
	cmp	x9, #8
	csel	w19, w10, w19, ne
	; v43 = symbol_addr @c_double
	adrp	x9, _c_double@GOTPAGE
	ldr	x9, [x9, _c_double@GOTPAGEOFF]
	; v44 = iconst.i64 21
	movz	x10, #0x15
	; v45 = call_indirect v43(v44) : (i64) -> i64 cc(c)
	mov	x0, x10
	blr	x9
	mov	x9, x0
	; v46 = icmp.ne v45, v13
	; v47 = iconst.i32 8
	movz	w10, #0x8
	; v48 = select v46, v47, v42
	cmp	x9, #42
	csel	w19, w10, w19, ne
	; v49 = symbol_addr @add
	adrp	x23, _add@PAGE
	add	x23, x23, _add@PAGEOFF
	; v50 = iconst.i64 40
	movz	x24, #0x28
	; v51 = call @c_apply(v49, v50, v10)
	mov	x0, x23
	mov	x1, x24
	mov	x2, x20
	bl	_c_apply
	mov	x9, x0
	; v52 = icmp.ne v51, v13
	; v53 = iconst.i32 9
	movz	w10, #0x9
	; v54 = select v52, v53, v48
	cmp	x9, #42
	csel	w19, w10, w19, ne
	; v55 = call @c_get_mul()
	bl	_c_get_mul
	mov	x9, x0
	; v56 = iconst.i64 6
	movz	x10, #0x6
	; v57 = call_indirect v55(v56, v24) : (i64, i64) -> i64 cc(c)
	mov	x0, x10
	mov	x1, x25
	blr	x9
	mov	x9, x0
	; v58 = icmp.ne v57, v13
	; v59 = iconst.i32 10
	movz	w10, #0xa
	; v60 = select v58, v59, v54
	cmp	x9, #42
	csel	w9, w10, w19, ne
	; v61 = symbol_addr @scratch
	adrp	x10, _scratch@PAGE
	add	x10, x10, _scratch@PAGEOFF
	; v62 = iconst.i64 56
	; v63 = ptradd v61, v62
	add	x10, x10, #56
	; v64 = load.i64 v63
	ldr	x11, [x10]
	; v65 = iconst.i64 99
	movz	x12, #0x63
	; store v65, v63
	str	x12, [x10]
	; v66 = load.i64 v63
	ldr	x10, [x10]
	; v67 = iadd v64, v66
	add	x10, x11, x10
	; v68 = icmp.ne v67, v65
	; v69 = iconst.i32 11
	movz	w11, #0xb
	; v70 = select v68, v69, v60
	cmp	x10, #99
	csel	w19, w11, w9, ne
	; v71 = call @c_check_mixed()
	bl	_c_check_mixed
	mov	x9, x0
	; v72 = iconst.i32 1
	; v73 = icmp.ne v71, v72
	; v74 = iconst.i32 12
	movz	w10, #0xc
	; v75 = select v73, v74, v70
	cmp	w9, #1
	csel	w9, w10, w19, ne
	; v76 = symbol_addr @mixed
	adrp	x10, _mixed@PAGE
	add	x10, x10, _mixed@PAGEOFF
	; v77 = ptradd v76, v10
	add	x11, x10, #2
	; v78 = load.i16 v77
	ldrh	w11, [x11]
	; v79 = sext.i64 v78
	sxth	x11, w11
	; v80 = iconst.i64 772
	; v81 = icmp.ne v79, v80
	; v82 = iconst.i32 13
	movz	w12, #0xd
	; v83 = select v81, v82, v75
	cmp	x11, #772
	csel	w9, w12, w9, ne
	; v84 = iconst.i64 4
	; v85 = ptradd v76, v84
	add	x10, x10, #4
	; v86 = load.i32 v85
	ldr	w10, [x10]
	; v87 = sext.i64 v86
	sxtw	x10, w10
	; v88 = iconst.i64 -2
	; v89 = icmp.ne v87, v88
	; v90 = iconst.i32 14
	movz	w11, #0xe
	; v91 = select v89, v90, v83
	cmn	x10, #2
	csel	w19, w11, w9, ne
	; v92 = symbol_addr @fp
	adrp	x25, _fp@PAGE
	add	x25, x25, _fp@PAGEOFF
	; v93 = load.ptr v92
	ldr	x9, [x25]
	; v94 = iconst.i64 50
	movz	x10, #0x32
	; v95 = call_indirect v93(v94, v18) : (i64, i64) -> i64 cc(c)
	mov	x0, x10
	mov	x1, x22
	blr	x9
	mov	x9, x0
	; v96 = icmp.ne v95, v13
	; v97 = iconst.i32 15
	movz	w10, #0xf
	; v98 = select v96, v97, v91
	cmp	x9, #42
	csel	w19, w10, w19, ne
	; store v49, v92
	str	x23, [x25]
	; v99 = load.ptr v92
	ldr	x9, [x25]
	; v100 = call_indirect v99(v50, v10) : (i64, i64) -> i64 cc(c)
	mov	x0, x24
	mov	x1, x20
	blr	x9
	mov	x9, x0
	; v101 = icmp.ne v100, v13
	; v102 = iconst.i32 16
	movz	w10, #0x10
	; v103 = select v101, v102, v98
	cmp	x9, #42
	csel	w19, w10, w19, ne
	; v104 = symbol_addr @bump
	adrp	x9, _bump@PAGE
	add	x9, x9, _bump@PAGEOFF
	; call_indirect v104() : () cc(c)
	blr	x9
	; v105 = call @c_read_counter()
	bl	_c_read_counter
	mov	x9, x0
	; v106 = iconst.i64 43
	; v107 = icmp.ne v105, v106
	; v108 = iconst.i32 17
	movz	w10, #0x11
	; v109 = select v107, v108, v103
	cmp	x9, #43
	csel	w19, w10, w19, ne
	; v110 = call @chain(v17)
	mov	x0, x21
	bl	_chain
	mov	x9, x0
	; v111 = iconst.i64 18
	; v112 = icmp.ne v110, v111
	; v113 = iconst.i32 18
	movz	w10, #0x12
	; v114 = select v112, v113, v109
	cmp	x9, #18
	csel	w9, w10, w19, ne
	; return v114
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
	.section	__TEXT,__text
	.globl	_chain
	.p2align	2
_chain:
Lchain_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v3=x11 v4=x19 v5=x20 v6=x9 v7=x21 v8=x22 v9=x9 v10=x9 v11=x9
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
	mov	x9, x0
Lchain_block0:
	; v1 = iconst.i64 8
	; v2 = ptradd v0, v1
	add	x10, x9, #8
	; v3 = load.ptr v0
	ldr	x11, [x9]
	; v4 = load.ptr v2
	ldr	x19, [x10]
	; v5 = load.ptr v0
	ldr	x20, [x9]
	; v6 = iconst.i64 1
	movz	x9, #0x1
	; v7 = iconst.i64 10
	movz	x21, #0xa
	; v8 = iconst.i64 3
	movz	x22, #0x3
	; v9 = call_indirect v3(v6, v7) : (i64, i64) -> i64 cc(c)
	mov	x0, x9
	mov	x1, x21
	blr	x11
	mov	x9, x0
	; v10 = call_indirect v4(v9, v8) : (i64, i64) -> i64 cc(c)
	mov	x0, x9
	mov	x1, x22
	blr	x19
	mov	x9, x0
	; v11 = call_indirect v5(v10, v7) : (i64, i64) -> i64 cc(c)
	mov	x0, x9
	mov	x1, x21
	blr	x20
	mov	x9, x0
	; return v11
	mov	x0, x9
	sub	sp, x29, #32
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lchain_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_add
	.p2align	2
_add:
Ladd_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ladd_block0:
	; v2 = iadd v0, v1
	add	x9, x9, x10
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ladd_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_sub
	.p2align	2
_sub:
Lsub_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Lsub_block0:
	; v2 = isub v0, v1
	sub	x9, x9, x10
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lsub_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_bump
	.p2align	2
_bump:
Lbump_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lbump_block0:
	; v0 = symbol_addr @counter
	adrp	x9, _counter@PAGE
	add	x9, x9, _counter@PAGEOFF
	; v1 = load.i64 v0
	ldr	x10, [x9]
	; v2 = iconst.i64 1
	; v3 = iadd v1, v2
	add	x10, x10, #1
	; store v3, v0
	str	x10, [x9]
	; return
	ldp	x29, x30, [sp], #16
	ret
Lbump_end:
	.cfi_endproc
	.globl	_greeting
	.section	__TEXT,__const
	.p2align	0
_greeting:
	.byte	72, 105, 32, 102, 114, 111, 109, 32, 100, 97, 116, 97, 33, 0
	.globl	_tail
	.section	__DATA,__const
	.p2align	3
_tail:
	.quad	_greeting+3
	.globl	_counter
	.section	__DATA,__data
	.p2align	3
_counter:
	.quad	40
	.globl	_ops
	.section	__DATA,__const
	.p2align	3
_ops:
	.quad	_add
	.quad	_sub
	.globl	_scratch
	.zerofill	__DATA,__bss,_scratch,64,3
	.globl	_mixed
	.section	__TEXT,__const
	.p2align	2
_mixed:
	.byte	1
	.byte	255
	.short	772
	.long	4294967294
	.space	4
	.globl	_fp
	.section	__DATA,__data
	.p2align	3
_fp:
	.quad	_sub
	.subsections_via_symbols
