	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "null_probe"
	.p2align	4
	.type	"null_probe",@function
"null_probe":
.Lnull_probe_begin:
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
	sub	rsp, 32
	mov	rbx, rdi
	mov	r12, rsi
	mov	r13, rdx
.Lnull_probe_block0:
	# v3 = iconst.i64 0
	xor	esi, esi
	# v4 = inttoptr.ptr v3
	mov	r14, rsi
	# call @clobber()
	call	"clobber"@PLT
	# v5 = icmp.slt v0, v1
	cmp	rbx, r12
	setl	bl
	# v6 = select v5, v4, v2
	test	bl, bl
	mov	rax, r13
	cmovne	rax, r14
	mov	r12, rax
	# v7 = call @null_observe(v4, v6, v4, v4, v4, v4, v4, v4, v4, v4)
	mov	qword ptr [rsp], r14
	mov	qword ptr [rsp + 8], r14
	mov	qword ptr [rsp + 16], r14
	mov	qword ptr [rsp + 24], r14
	mov	rdi, r14
	mov	rsi, r12
	mov	rdx, r14
	mov	rcx, r14
	mov	r8, r14
	mov	r9, r14
	call	"null_observe"@PLT
	mov	rsi, rax
	# v14 = select v5, v4, v6
	test	bl, bl
	mov	rax, r12
	cmovne	rax, r14
	mov	rdi, rax
	# v15 = select v5, v6, v4
	test	bl, bl
	mov	rax, r14
	cmovne	rax, r12
	mov	r8, rax
	# v10 = ptrtoint.i64 v14
	# v11 = ptrtoint.i64 v15
	# v12 = iadd v10, v11
	add	rdi, r8
	# v13 = iadd v7, v12
	add	rsi, rdi
	# return v13
	mov	rax, rsi
	lea	rsp, [rbp + -32]
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lnull_probe_end:
	.cfi_endproc
	.size "null_probe", .-"null_probe"
	.section	".text","ax",@progbits
	.globl "null_return"
	.p2align	4
	.type	"null_return",@function
"null_return":
.Lnull_return_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lnull_return_block0:
	# v0 = iconst.i64 0
	xor	esi, esi
	# v1 = inttoptr.ptr v0
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lnull_return_end:
	.cfi_endproc
	.size "null_return", .-"null_return"
	.section	".text","ax",@progbits
	.globl "null_cycle"
	.p2align	4
	.type	"null_cycle",@function
"null_cycle":
.Lnull_cycle_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lnull_cycle_block0:
	# jump block1(v0, v1, v2)
.Lnull_cycle_block1:
	# v9 = iconst.i64 1
	# v10 = isub v7, v9
	sub	r8, 1
	# brif v10, block1(v6, v5, v10), block2
	test	r8, r8
	je	".Lnull_cycle_block2"
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
	jmp	".Lnull_cycle_block1"
.Lnull_cycle_block2:
	# v15 = isub v5, v6
	sub	rsi, rdi
	# return v15
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lnull_cycle_end:
	.cfi_endproc
	.size "null_cycle", .-"null_cycle"
	.section	".text","ax",@progbits
	.globl "null_branch"
	.p2align	4
	.type	"null_branch",@function
"null_branch":
.Lnull_branch_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lnull_branch_block0:
	# v1 = iconst.i64 0
	xor	edi, edi
	# v2 = inttoptr.ptr v1
	# v3 = icmp.eq v0, v2
	# brif v3, block1, block2
	cmp	rsi, rdi
	je	".Lnull_branch_block1"
.Lnull_branch_block2:
	# v5 = iconst.i64 91
	mov	esi, 91
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lnull_branch_block1:
	# v4 = iconst.i64 37
	mov	esi, 37
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lnull_branch_end:
	.cfi_endproc
	.size "null_branch", .-"null_branch"
	.section	".text","ax",@progbits
	.globl "null_setjmp"
	.p2align	4
	.type	"null_setjmp",@function
"null_setjmp":
.Lnull_setjmp_begin:
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
.Lnull_setjmp_block0:
	# v3 = call @setjmp(v0)
	mov	rdi, rbx
	call	"setjmp"@PLT
	mov	rsi, rax
	# brif v3, block2, block1
	test	esi, esi
	jne	".Lnull_setjmp_block2"
.Lnull_setjmp_block1:
	# v4 = iconst.i32 1
	mov	esi, 1
	# call @longjmp(v0, v4)
	mov	rdi, rbx
	call	"longjmp"@PLT
	# trap
	ud2
.Lnull_setjmp_block2:
	# v1 = iconst.i64 0
	xor	esi, esi
	# v2 = inttoptr.ptr v1
	# return v2
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lnull_setjmp_end:
	.cfi_endproc
	.size "null_setjmp", .-"null_setjmp"
	.section	".text","ax",@progbits
	.globl "null_stackrestore"
	.p2align	4
	.type	"null_stackrestore",@function
"null_stackrestore":
.Lnull_stackrestore_begin:
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
	mov	rsi, rdi
.Lnull_stackrestore_block0:
	# v1 = iconst.i64 0
	xor	edi, edi
	# v2 = inttoptr.ptr v1
	mov	rbx, rdi
	# v3 = intrinsic stack_save()
	mov	r12, rsp
	# v4 = intrinsic stack_alloc(v0)
	lea	rax, [rsi + 15]
	and	rax, -16
	sub	rsp, rax
	lea	rsi, [rsp]
	# store v2, v4
	mov	qword ptr [rsi], rbx
	# call @clobber()
	call	"clobber"@PLT
	# intrinsic stack_restore(v3)
	mov	rsp, r12
	# return v2
	mov	rax, rbx
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lnull_stackrestore_end:
	.cfi_endproc
	.size "null_stackrestore", .-"null_stackrestore"
	.section	".note.GNU-stack","",@progbits
