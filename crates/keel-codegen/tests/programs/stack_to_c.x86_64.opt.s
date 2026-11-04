	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "main"
	.p2align	4
	.type	"main",@function
"main":
.Lmain_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 56
.Lmain_block0:
	# v0 = stack_addr ss0
	lea	rsi, [rbp + -64]
	# v1 = iconst.i64 10
	mov	edi, 10
	# call @fill_squares(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"fill_squares"@PLT
	# v2 = iconst.i64 0
	xor	esi, esi
	# jump block2(v2, v2)
	mov	rdi, rsi
.Lmain_block2:
	# v28 = iconst.i64 2
	# v7 = shl v35, v28
	# v34 = stack_addr ss0
	lea	r8, [rbp + -64]
	# v8 = ptradd v34, v7
	# v9 = load.i32 v8
	mov	r8d, dword ptr [r8 + rsi * 4]
	# v10 = sext.i64 v9
	movsxd	r8, r8d
	# v11 = iadd v36, v10
	add	rdi, r8
	# v12 = iconst.i64 1
	# v13 = iadd v35, v12
	add	rsi, 1
	# v39 = iconst.i64 10
	# v40 = icmp.ult v13, v39
	# brif v40, block2(v13, v11), block3(v11)
	cmp	rsi, 10
	jb	".Lmain_block2"
	mov	rbx, rdi
.Lmain_block3:
	# v14 = stack_addr ss1
	lea	rsi, [rbp + -24]
	# v15 = iconst.i8 111
	# store v15, v14
	mov	byte ptr [rbp + -24], 111
	# v16 = iconst.i64 1
	# v17 = ptradd v14, v16
	# v18 = iconst.i8 107
	# store v18, v17
	mov	byte ptr [rsi + 1], 107
	# v19 = iconst.i64 2
	# v20 = ptradd v14, v19
	# v21 = iconst.i8 0
	# store v21, v20
	mov	byte ptr [rsi + 2], 0
	# v22 = call @puts(v14)
	mov	rdi, rsi
	call	"puts"@PLT
	mov	rsi, rax
	# v23 = iconst.i64 285
	# v24 = icmp.ne v38, v23
	cmp	rbx, 285
	setne	sil
	# v27 = uext.i32 v24
	movzx	esi, sil
	# return v27
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
