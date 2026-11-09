	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "across"
	.p2align	4
	.type	"across",@function
"across":
.Lacross_begin:
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
	push	r13
	.cfi_offset 13, -40
	push	r14
	.cfi_offset 14, -48
	push	r15
	.cfi_offset 15, -56
	sub	rsp, 8
	mov	rbx, rdi
.Lacross_block0:
	# v1 = iconst.i64 3
	# v2 = imul v0, v1
	imul	r12, rbx, 3
	# v3 = iconst.i64 5
	# v4 = iadd v0, v3
	lea	r13, [rbx + 5]
	# v5 = xor v2, v4
	mov	r14, r12
	xor	r14, r13
	# v6 = trunc.i32 v0
	mov	rsi, rbx
	# v7 = iconst.i32 7
	# v8 = iadd v6, v7
	lea	r15d, [rsi + 7]
	# call @clobber()
	call	"clobber"@PLT
	# v9 = sext.i64 v8
	movsxd	rsi, r15d
	# v10 = iadd v2, v4
	add	r12, r13
	# v11 = iadd v5, v10
	add	r14, r12
	# v12 = iadd v9, v11
	add	rsi, r14
	# v13 = iadd v0, v12
	add	rbx, rsi
	# return v13
	mov	rax, rbx
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lacross_end:
	.cfi_endproc
	.size "across", .-"across"
	.section	".text","ax",@progbits
	.globl "nested"
	.p2align	4
	.type	"nested",@function
"nested":
.Lnested_begin:
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
	push	r13
	.cfi_offset 13, -40
	sub	rsp, 8
	mov	rbx, rdi
.Lnested_block0:
	# v1 = iconst.i64 1
	# v2 = iadd v0, v1
	lea	r12, [rbx + 1]
	# v4 = shl v0, v1
	# v5 = call @across(v0)
	mov	rdi, rbx
	call	"across"
	mov	r13, rax
	# v6 = call @across(v2)
	mov	rdi, r12
	call	"across"
	mov	rsi, rax
	# v7 = iadd v5, v6
	add	r13, rsi
	# v8 = iadd v2, v7
	add	r12, r13
	# v9 = iadd v4, v8
	lea	rsi, [r12 + rbx * 2]
	# v10 = iadd v0, v9
	add	rbx, rsi
	# return v10
	mov	rax, rbx
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lnested_end:
	.cfi_endproc
	.size "nested", .-"nested"
	.section	".text","ax",@progbits
	.globl "call_loop"
	.p2align	4
	.type	"call_loop",@function
"call_loop":
.Lcall_loop_begin:
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
	push	r13
	.cfi_offset 13, -40
	push	r14
	.cfi_offset 14, -48
	mov	rbx, rdi
.Lcall_loop_block0:
	# v2 = iconst.i64 0
	xor	edi, edi
	# v3 = iconst.i64 10
	# v4 = imul v1, v3
	imul	r12, rsi, 10
	# jump block1(v2, v2)
	mov	r13, rdi
	mov	r14, rdi
.Lcall_loop_block1:
	# v7 = icmp.ugt v0, v5
	# brif v7, block2, block3
	cmp	rbx, r13
	jbe	".Lcall_loop_block3"
.Lcall_loop_block2:
	# v8 = call @twice(v5)
	mov	rdi, r13
	call	"twice"@PLT
	mov	rsi, rax
	# v9 = iadd v6, v8
	add	r14, rsi
	# v10 = iadd v4, v9
	lea	rsi, [r12 + r14 * 1]
	# v11 = iconst.i64 1
	# v12 = iadd v5, v11
	add	r13, 1
	# jump block1(v12, v10)
	mov	r14, rsi
	jmp	".Lcall_loop_block1"
.Lcall_loop_block3:
	# return v6
	mov	rax, r14
	lea	rsp, [rbp + -32]
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lcall_loop_end:
	.cfi_endproc
	.size "call_loop", .-"call_loop"
	.section	".text","ax",@progbits
	.globl "memset_loop"
	.p2align	4
	.type	"memset_loop",@function
"memset_loop":
.Lmemset_loop_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rsi, rdi
.Lmemset_loop_block0:
	# v3 = iconst.i64 0
	xor	edi, edi
	# v6 = icmp.ugt v0, v3
	# brif v6, block2(v3, v3), block3(v3)
	cmp	rsi, 0
	jbe	".Lmemset_loop_block3"
	mov	r8, rdi
.Lmemset_loop_block2:
	# v7 = trunc.i8 v15
	mov	r9, rdi
	# v12 = uext.i64 v7
	movzx	r9d, r9b
	# v13 = iconst.i64 72340172838076673
	movabs	rbx, 72340172838076673
	# v14 = imul v12, v13
	imul	r9, rbx
	# v9 = iadd v16, v14
	add	r8, r9
	# v10 = iconst.i64 1
	# v11 = iadd v15, v10
	add	rdi, 1
	# v19 = icmp.ugt v0, v11
	# brif v19, block2(v11, v9), block3(v9)
	cmp	rsi, rdi
	ja	".Lmemset_loop_block2"
	mov	rdi, r8
.Lmemset_loop_block3:
	# return v18
	mov	rax, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lmemset_loop_end:
	.cfi_endproc
	.size "memset_loop", .-"memset_loop"
	.section	".note.GNU-stack","",@progbits
