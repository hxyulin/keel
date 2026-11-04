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
	lea	rdi, [r12 + r13 * 1]
	# v11 = iadd v10, v5
	add	rdi, r14
	# v12 = iadd v11, v9
	add	rsi, rdi
	# v13 = iadd v12, v0
	add	rsi, rbx
	# return v13
	mov	rax, rsi
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
	push	r14
	.cfi_offset 14, -48
	mov	rbx, rdi
.Lnested_block0:
	# v1 = iconst.i64 1
	# v2 = iadd v0, v1
	lea	r12, [rbx + 1]
	# v3 = iconst.i64 2
	# v4 = imul v0, v3
	imul	r13, rbx, 2
	# v5 = call @across(v0)
	mov	rdi, rbx
	call	"across"
	mov	r14, rax
	# v6 = call @across(v2)
	mov	rdi, r12
	call	"across"
	mov	rsi, rax
	# v7 = iadd v5, v6
	add	rsi, r14
	# v8 = iadd v7, v2
	add	rsi, r12
	# v9 = iadd v8, v4
	add	rsi, r13
	# v10 = iadd v9, v0
	add	rsi, rbx
	# return v10
	mov	rax, rsi
	lea	rsp, [rbp + -32]
	pop	r14
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
	# v7 = icmp.ult v5, v0
	# brif v7, block2, block3
	cmp	r13, rbx
	jb	".Lcall_loop_block2"
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
.Lcall_loop_block2:
	# v8 = call @twice(v5)
	mov	rdi, r13
	call	"twice"@PLT
	mov	rsi, rax
	# v9 = iadd v6, v8
	add	rsi, r14
	# v10 = iadd v9, v4
	add	rsi, r12
	# v11 = iconst.i64 1
	# v12 = iadd v5, v11
	lea	rdi, [r13 + 1]
	# jump block1(v12, v10)
	mov	r13, rdi
	mov	r14, rsi
	jmp	".Lcall_loop_block1"
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
.Lmemset_loop_block0:
	# v1 = stack_addr ss0
	lea	r12, [rbp + -48]
	# v2 = iconst.i64 8
	mov	r13d, 8
	# v3 = iconst.i64 0
	xor	esi, esi
	# jump block1(v3, v3)
	mov	r14, rsi
	mov	r15, rsi
.Lmemset_loop_block1:
	# v6 = icmp.ult v4, v0
	# brif v6, block2, block3
	cmp	r14, rbx
	jb	".Lmemset_loop_block2"
.Lmemset_loop_block3:
	# return v5
	mov	rax, r15
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lmemset_loop_block2:
	# v7 = trunc.i8 v4
	mov	rsi, r14
	# intrinsic memset(v1, v7, v2)
	mov	rdi, r12
	mov	rdx, r13
	call	"memset"@PLT
	# v8 = load.i64 v1
	mov	rsi, qword ptr [rbp + -48]
	# v9 = iadd v5, v8
	add	rsi, r15
	# v10 = iconst.i64 1
	# v11 = iadd v4, v10
	lea	rdi, [r14 + 1]
	# jump block1(v11, v9)
	mov	r14, rdi
	mov	r15, rsi
	jmp	".Lmemset_loop_block1"
.Lmemset_loop_end:
	.cfi_endproc
	.size "memset_loop", .-"memset_loop"
	.section	".note.GNU-stack","",@progbits
