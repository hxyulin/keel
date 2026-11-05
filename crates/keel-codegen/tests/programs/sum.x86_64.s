	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "sum"
	.p2align	4
	.type	"sum",@function
"sum":
.Lsum_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsum_block0:
	# v2 = iconst.i64 0
	xor	r8d, r8d
	# jump block1(v2, v2)
	mov	r9, r8
.Lsum_block1:
	# v5 = icmp.ult v3, v1
	# brif v5, block2, block3(v4)
	cmp	r8, rdi
	jb	".Lsum_block2"
	mov	rsi, r9
.Lsum_block3:
	# return v14
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lsum_block2:
	# v6 = iconst.i64 4
	# v7 = imul v3, v6
	imul	rbx, r8, 4
	# v8 = ptradd v0, v7
	add	rbx, rsi
	# v9 = load.i32 v8
	mov	ebx, dword ptr [rbx]
	# v10 = sext.i64 v9
	movsxd	rbx, ebx
	# v11 = iadd v4, v10
	add	r9, rbx
	# v12 = iconst.i64 1
	# v13 = iadd v3, v12
	add	r8, 1
	# jump block1(v13, v11)
	jmp	".Lsum_block1"
.Lsum_end:
	.cfi_endproc
	.size "sum", .-"sum"
	.section	".note.GNU-stack","",@progbits
