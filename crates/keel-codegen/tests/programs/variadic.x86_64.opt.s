	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_sum"
	.p2align	4
	.type	"k_sum",@function
"k_sum":
.Lk_sum_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 208
	mov	qword ptr [rbp + -208], rdi
	mov	qword ptr [rbp + -200], rsi
	mov	qword ptr [rbp + -192], rdx
	mov	qword ptr [rbp + -184], rcx
	mov	qword ptr [rbp + -176], r8
	mov	qword ptr [rbp + -168], r9
	test	al, al
	je	".Lk_sum_saved"
	movaps	xmmword ptr [rbp + -160], xmm0
	movaps	xmmword ptr [rbp + -144], xmm1
	movaps	xmmword ptr [rbp + -128], xmm2
	movaps	xmmword ptr [rbp + -112], xmm3
	movaps	xmmword ptr [rbp + -96], xmm4
	movaps	xmmword ptr [rbp + -80], xmm5
	movaps	xmmword ptr [rbp + -64], xmm6
	movaps	xmmword ptr [rbp + -48], xmm7
.Lk_sum_saved:
	mov	rsi, rdi
.Lk_sum_block0:
	# v1 = stack_addr ss0
	lea	rdi, [rbp + -32]
	# intrinsic va_start(v1)
	mov	dword ptr [rdi], 8
	mov	dword ptr [rdi + 4], 48
	lea	rax, [rbp + 16]
	mov	qword ptr [rdi + 8], rax
	lea	rax, [rbp + -208]
	mov	qword ptr [rdi + 16], rax
	# v2 = iconst.i64 0
	xor	edi, edi
	# v3 = iconst.i32 0
	xor	r8d, r8d
	# v6 = icmp.sgt v0, v3
	# brif v6, block2(v3, v2), block3(v2)
	cmp	esi, 0
	jle	".Lk_sum_block3"
.Lk_sum_block2:
	# v13 = stack_addr ss0
	lea	r9, [rbp + -32]
	# v7 = va_arg.i64 v13
	mov	ecx, dword ptr [r9]
	cmp	ecx, 48
	jae	".Lk_sum_inst7_stack"
	mov	rax, qword ptr [r9 + 16]
	add	rax, rcx
	add	ecx, 8
	mov	dword ptr [r9], ecx
	jmp	".Lk_sum_inst7_load"
.Lk_sum_inst7_stack:
	mov	rax, qword ptr [r9 + 8]
	lea	rcx, [rax + 8]
	mov	qword ptr [r9 + 8], rcx
.Lk_sum_inst7_load:
	mov	r9, qword ptr [rax]
	# v8 = iadd v15, v7
	add	rdi, r9
	# v9 = iconst.i32 1
	# v10 = iadd v14, v9
	add	r8d, 1
	# v18 = icmp.sgt v0, v10
	# brif v18, block2(v10, v8), block3(v8)
	cmp	esi, r8d
	jg	".Lk_sum_block2"
.Lk_sum_block3:
	# return v17
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sum_end:
	.cfi_endproc
	.size "k_sum", .-"k_sum"
	.section	".text","ax",@progbits
	.globl "k_pairs"
	.p2align	4
	.type	"k_pairs",@function
"k_pairs":
.Lk_pairs_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 208
	mov	qword ptr [rbp + -208], rdi
	mov	qword ptr [rbp + -200], rsi
	mov	qword ptr [rbp + -192], rdx
	mov	qword ptr [rbp + -184], rcx
	mov	qword ptr [rbp + -176], r8
	mov	qword ptr [rbp + -168], r9
	test	al, al
	je	".Lk_pairs_saved"
	movaps	xmmword ptr [rbp + -160], xmm0
	movaps	xmmword ptr [rbp + -144], xmm1
	movaps	xmmword ptr [rbp + -128], xmm2
	movaps	xmmword ptr [rbp + -112], xmm3
	movaps	xmmword ptr [rbp + -96], xmm4
	movaps	xmmword ptr [rbp + -80], xmm5
	movaps	xmmword ptr [rbp + -64], xmm6
	movaps	xmmword ptr [rbp + -48], xmm7
.Lk_pairs_saved:
	mov	rsi, rdi
.Lk_pairs_block0:
	# v1 = stack_addr ss0
	lea	rdi, [rbp + -32]
	# intrinsic va_start(v1)
	mov	dword ptr [rdi], 8
	mov	dword ptr [rdi + 4], 48
	lea	rax, [rbp + 16]
	mov	qword ptr [rdi + 8], rax
	lea	rax, [rbp + -208]
	mov	qword ptr [rdi + 16], rax
	# v2 = fconst.f64 0.0
	xorps	xmm0, xmm0
	# v3 = iconst.i32 0
	xor	edi, edi
	# v6 = icmp.sgt v0, v3
	# brif v6, block2(v3, v2), block3(v2)
	cmp	esi, 0
	jle	".Lk_pairs_block3"
.Lk_pairs_block2:
	# v16 = stack_addr ss0
	lea	r8, [rbp + -32]
	# v7 = va_arg.i32 v16
	mov	ecx, dword ptr [r8]
	cmp	ecx, 48
	jae	".Lk_pairs_inst7_stack"
	mov	rax, qword ptr [r8 + 16]
	add	rax, rcx
	add	ecx, 8
	mov	dword ptr [r8], ecx
	jmp	".Lk_pairs_inst7_load"
.Lk_pairs_inst7_stack:
	mov	rax, qword ptr [r8 + 8]
	lea	rcx, [rax + 8]
	mov	qword ptr [r8 + 8], rcx
.Lk_pairs_inst7_load:
	mov	r9d, dword ptr [rax]
	# v8 = va_arg.f64 v16
	mov	ecx, dword ptr [r8 + 4]
	cmp	ecx, 176
	jae	".Lk_pairs_inst8_stack"
	mov	rax, qword ptr [r8 + 16]
	add	rax, rcx
	add	ecx, 16
	mov	dword ptr [r8 + 4], ecx
	jmp	".Lk_pairs_inst8_load"
.Lk_pairs_inst8_stack:
	mov	rax, qword ptr [r8 + 8]
	lea	rcx, [rax + 8]
	mov	qword ptr [r8 + 8], rcx
.Lk_pairs_inst8_load:
	movsd	xmm1, qword ptr [rax]
	# v9 = sitofp.f64 v7
	xorps	xmm2, xmm2
	cvtsi2sd	xmm2, r9d
	# v10 = fmul v9, v8
	mulsd	xmm2, xmm1
	# v11 = fadd v18, v10
	addsd	xmm0, xmm2
	# v12 = iconst.i32 1
	# v13 = iadd v17, v12
	add	edi, 1
	# v21 = icmp.sgt v0, v13
	# brif v21, block2(v13, v11), block3(v11)
	cmp	esi, edi
	jg	".Lk_pairs_block2"
.Lk_pairs_block3:
	# return v20
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pairs_end:
	.cfi_endproc
	.size "k_pairs", .-"k_pairs"
	.section	".text","ax",@progbits
	.globl "k_tail"
	.p2align	4
	.type	"k_tail",@function
"k_tail":
.Lk_tail_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 208
	mov	qword ptr [rbp + -208], rdi
	mov	qword ptr [rbp + -200], rsi
	mov	qword ptr [rbp + -192], rdx
	mov	qword ptr [rbp + -184], rcx
	mov	qword ptr [rbp + -176], r8
	mov	qword ptr [rbp + -168], r9
	test	al, al
	je	".Lk_tail_saved"
	movaps	xmmword ptr [rbp + -160], xmm0
	movaps	xmmword ptr [rbp + -144], xmm1
	movaps	xmmword ptr [rbp + -128], xmm2
	movaps	xmmword ptr [rbp + -112], xmm3
	movaps	xmmword ptr [rbp + -96], xmm4
	movaps	xmmword ptr [rbp + -80], xmm5
	movaps	xmmword ptr [rbp + -64], xmm6
	movaps	xmmword ptr [rbp + -48], xmm7
.Lk_tail_saved:
	mov	rsi, qword ptr [rbp + 24]
	mov	rdi, qword ptr [rbp + 32]
.Lk_tail_block0:
	# v9 = stack_addr ss0
	lea	r8, [rbp + -32]
	# intrinsic va_start(v9)
	mov	dword ptr [r8], 48
	mov	dword ptr [r8 + 4], 48
	lea	rax, [rbp + 40]
	mov	qword ptr [r8 + 8], rax
	lea	rax, [rbp + -208]
	mov	qword ptr [r8 + 16], rax
	# v10 = va_arg.ptr v9
	mov	ecx, dword ptr [r8]
	cmp	ecx, 48
	jae	".Lk_tail_inst2_stack"
	mov	rax, qword ptr [r8 + 16]
	add	rax, rcx
	add	ecx, 8
	mov	dword ptr [r8], ecx
	jmp	".Lk_tail_inst2_load"
.Lk_tail_inst2_stack:
	mov	rax, qword ptr [r8 + 8]
	lea	rcx, [rax + 8]
	mov	qword ptr [r8 + 8], rcx
.Lk_tail_inst2_load:
	mov	r9, qword ptr [rax]
	# v11 = load.i64 v10
	mov	r9, qword ptr [r9]
	# v12 = va_arg.i64 v9
	mov	ecx, dword ptr [r8]
	cmp	ecx, 48
	jae	".Lk_tail_inst4_stack"
	mov	rax, qword ptr [r8 + 16]
	add	rax, rcx
	add	ecx, 8
	mov	dword ptr [r8], ecx
	jmp	".Lk_tail_inst4_load"
.Lk_tail_inst4_stack:
	mov	rax, qword ptr [r8 + 8]
	lea	rcx, [rax + 8]
	mov	qword ptr [r8 + 8], rcx
.Lk_tail_inst4_load:
	mov	r8, qword ptr [rax]
	# v13 = sext.i64 v8
	movsxd	rdi, edi
	# v14 = iadd v11, v13
	add	r9, rdi
	# v15 = iadd v12, v14
	add	r8, r9
	# v16 = iadd v7, v15
	add	rsi, r8
	# return v16
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_tail_end:
	.cfi_endproc
	.size "k_tail", .-"k_tail"
	.section	".text","ax",@progbits
	.globl "k_sum_three"
	.p2align	4
	.type	"k_sum_three",@function
"k_sum_three":
.Lk_sum_three_begin:
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
.Lk_sum_three_block0:
	# v3 = iconst.i32 3
	mov	r9d, 3
	# v4 = call @k_sum(v3, v0, v1, v2)
	mov	rdx, rdi
	mov	rdi, r9
	mov	rcx, r8
	mov	eax, 0
	call	"k_sum"
	mov	rsi, rax
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sum_three_end:
	.cfi_endproc
	.size "k_sum_three", .-"k_sum_three"
	.section	".note.GNU-stack","",@progbits
