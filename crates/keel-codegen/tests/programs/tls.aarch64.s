	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_bump
	.p2align	2
_k_bump:
Lk_bump_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x19 v4=x20 v5=x9 v6=x10 v7=x9 v8=x10 v9=x10 v10=x9 v11=x9 v12=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
Lk_bump_block0:
	; v0 = symbol_addr @counter
	adrp	x0, _counter@TLVPPAGE
	ldr	x0, [x0, _counter@TLVPPAGEOFF]
	ldr	x16, [x0]
	blr	x16
	mov	x9, x0
	; v1 = load.i64 v0
	ldr	x10, [x9]
	; v2 = iconst.i64 1
	; v3 = iadd v1, v2
	add	x19, x10, #1
	; store v3, v0
	str	x19, [x9]
	; v4 = symbol_addr @zeroed
	adrp	x0, _zeroed@TLVPPAGE
	ldr	x0, [x0, _zeroed@TLVPPAGEOFF]
	ldr	x16, [x0]
	blr	x16
	mov	x20, x0
	; call @clobber()
	bl	_clobber
	; v5 = load.i32 v4
	ldr	w9, [x20]
	; v6 = iconst.i32 2
	; v7 = iadd v5, v6
	add	w9, w9, #2
	; store v7, v4
	str	w9, [x20]
	; v8 = symbol_addr @c_tls
	adrp	x0, _c_tls@TLVPPAGE
	ldr	x0, [x0, _c_tls@TLVPPAGEOFF]
	ldr	x16, [x0]
	blr	x16
	mov	x10, x0
	; v9 = load.i64 v8
	ldr	x10, [x10]
	; v10 = sext.i64 v7
	sxtw	x9, w9
	; v11 = iadd v3, v10
	add	x9, x19, x9
	; v12 = iadd v11, v9
	add	x9, x9, x10
	; return v12
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_bump_end:
	.cfi_endproc
	.section	__DATA,__thread_data,thread_local_regular
	.p2align	3
_counter$tlv$init:
	.quad	5
	.section	__DATA,__thread_vars,thread_local_variables
	.globl	_counter
	.p2align	3
_counter:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_counter$tlv$init
	.tbss	_zeroed$tlv$init, 4, 2
	.section	__DATA,__thread_vars,thread_local_variables
	.globl	_zeroed
	.p2align	3
_zeroed:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_zeroed$tlv$init
	.subsections_via_symbols
