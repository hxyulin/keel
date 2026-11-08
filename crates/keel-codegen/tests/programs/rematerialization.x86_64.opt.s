	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "remat_probe"
	.p2align	4
	.type	"remat_probe",@function
"remat_probe":
.Lremat_probe_begin:
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
	sub	rsp, 56
	mov	qword ptr [rbp + -64], rdi
	mov	qword ptr [rbp + -56], rsi
.Lremat_probe_block0:
	# v2 = iconst.i64 0
	xor	eax, eax
	mov	qword ptr [rbp + -48], rax
	# v3 = iconst.i64 17
	mov	r14d, 17
	# v4 = iconst.i64 31
	mov	r15d, 31
	# v5 = iconst.i64 255
	mov	r12d, 255
	# v6 = iconst.i64 65535
	mov	ebx, 65535
	# call @clobber()
	call	"clobber"@PLT
	# v8 = select v0, v2, v1
	mov	rax, qword ptr [rbp + -64]
	test	rax, rax
	mov	rcx, qword ptr [rbp + -48]
	mov	rdx, qword ptr [rbp + -56]
	mov	rax, rdx
	cmovne	rax, rcx
	mov	r13, rax
	# v9 = call @remat_sum(v8, v3, v4, v5, v6, v2, v3, v4, v5, v6)
	mov	qword ptr [rsp], r14
	mov	qword ptr [rsp + 8], r15
	mov	qword ptr [rsp + 16], r12
	mov	qword ptr [rsp + 24], rbx
	mov	rdi, r13
	mov	rsi, r14
	mov	rdx, r15
	mov	rcx, r12
	mov	r8, rbx
	mov	r9, qword ptr [rbp + -48]
	call	"remat_sum"@PLT
	mov	rsi, rax
	# v18 = select v0, v3, v4
	mov	rax, qword ptr [rbp + -64]
	test	rax, rax
	mov	rax, r15
	cmovne	rax, r14
	mov	rdi, rax
	# v14 = iadd v8, v9
	add	r13, rsi
	# v15 = iadd v14, v18
	add	r13, rdi
	# v16 = iadd v1, v15
	mov	rax, qword ptr [rbp + -56]
	lea	rsi, [rax + r13 * 1]
	# return v16
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lremat_probe_end:
	.cfi_endproc
	.size "remat_probe", .-"remat_probe"
	.section	".text","ax",@progbits
	.globl "remat_shift"
	.p2align	4
	.type	"remat_shift",@function
"remat_shift":
.Lremat_shift_begin:
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
	mov	r12, rsi
.Lremat_shift_block0:
	# v1 = iconst.i64 7
	mov	r13d, 7
	# call @clobber()
	call	"clobber"@PLT
	# v2 = shl v0, v1
	mov	rcx, r13
	mov	rax, rbx
	mov	rdx, r12
	shld	rdx, rax, cl
	shl	rax, cl
	test	cl, 64
	je	".Lremat_shift_inst2_shifted"
	mov	rdx, rax
	xor	eax, eax
.Lremat_shift_inst2_shifted:
	mov	rbx, rax
	mov	r12, rdx
	# return v2
	mov	rax, rbx
	mov	rdx, r12
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lremat_shift_end:
	.cfi_endproc
	.size "remat_shift", .-"remat_shift"
	.section	".note.GNU-stack","",@progbits
