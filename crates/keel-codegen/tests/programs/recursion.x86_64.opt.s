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
	push	r12
	.cfi_offset 12, -32
.Lmain_block0:
	# v1 = iconst.i64 10
	mov	esi, 10
	# v2 = call @fib(v1)
	mov	rdi, rsi
	call	"fib"
	mov	rsi, rax
	# v3 = iconst.i64 55
	# v4 = icmp.ne v2, v3
	cmp	rsi, 55
	setne	sil
	# v6 = uext.i32 v4
	movzx	ebx, sil
	# v7 = iconst.i64 20
	mov	r12d, 20
	# v8 = call @fib(v7)
	mov	rdi, r12
	call	"fib"
	mov	rsi, rax
	# v9 = iconst.i64 6765
	# v10 = icmp.ne v8, v9
	cmp	rsi, 6765
	setne	sil
	# v11 = iconst.i32 2
	mov	edi, 2
	# v12 = select v10, v11, v6
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rbx, rax
	# v14 = call @fact(v7)
	mov	rdi, r12
	call	"fact"
	mov	rsi, rax
	# v15 = iconst.i64 2432902008176640000
	movabs	rdi, 2432902008176640000
	# v16 = icmp.ne v14, v15
	cmp	rsi, rdi
	setne	sil
	# v17 = iconst.i32 3
	mov	edi, 3
	# v18 = select v16, v17, v12
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rsi, rax
	# return v18
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".text","ax",@progbits
	.globl "fib"
	.p2align	4
	.type	"fib",@function
"fib":
.Lfib_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	push	r12
	.cfi_offset 12, -32
	mov	rbx, rdi
.Lfib_block0:
	# v1 = iconst.i64 2
	# v2 = icmp.ult v0, v1
	# brif v2, block2(v0), block1
	cmp	rbx, 2
	jae	".Lfib_block1"
	mov	rsi, rbx
	jmp	".Lfib_block2"
.Lfib_block1:
	# v3 = iconst.i64 1
	# v4 = isub v0, v3
	mov	rsi, rbx
	sub	rsi, 1
	# v5 = call @fib(v4)
	mov	rdi, rsi
	call	"fib"
	mov	r12, rax
	# v12 = iconst.i64 2
	# v6 = isub v0, v12
	sub	rbx, 2
	# v7 = call @fib(v6)
	mov	rdi, rbx
	call	"fib"
	mov	rsi, rax
	# v8 = iadd v5, v7
	add	r12, rsi
	# jump block2(v8)
	mov	rsi, r12
.Lfib_block2:
	# return v9
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lfib_end:
	.cfi_endproc
	.size "fib", .-"fib"
	.section	".text","ax",@progbits
	.globl "fact"
	.p2align	4
	.type	"fact",@function
"fact":
.Lfact_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rbx, rdi
.Lfact_block0:
	# v1 = iconst.i64 0
	# v2 = icmp.eq v0, v1
	cmp	rbx, 0
	sete	sil
	# v3 = iconst.i64 1
	mov	edi, 1
	# brif v2, block2(v3), block1
	test	sil, sil
	je	".Lfact_block1"
	mov	rsi, rdi
	jmp	".Lfact_block2"
.Lfact_block1:
	# v10 = iconst.i64 1
	# v4 = isub v0, v10
	mov	rsi, rbx
	sub	rsi, 1
	# v5 = call @fact(v4)
	mov	rdi, rsi
	call	"fact"
	mov	rsi, rax
	# v6 = imul v0, v5
	imul	rbx, rsi
	# jump block2(v6)
	mov	rsi, rbx
.Lfact_block2:
	# return v7
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lfact_end:
	.cfi_endproc
	.size "fact", .-"fact"
	.section	".note.GNU-stack","",@progbits
