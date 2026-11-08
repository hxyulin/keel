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
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 216
	mov	qword ptr [rbp + -224], rdi
	mov	qword ptr [rbp + -216], rsi
	mov	qword ptr [rbp + -208], rdx
	mov	qword ptr [rbp + -200], rcx
	mov	qword ptr [rbp + -192], r8
	mov	qword ptr [rbp + -184], r9
	test	al, al
	je	".Lk_sum_saved"
	movaps	xmmword ptr [rbp + -176], xmm0
	movaps	xmmword ptr [rbp + -160], xmm1
	movaps	xmmword ptr [rbp + -144], xmm2
	movaps	xmmword ptr [rbp + -128], xmm3
	movaps	xmmword ptr [rbp + -112], xmm4
	movaps	xmmword ptr [rbp + -96], xmm5
	movaps	xmmword ptr [rbp + -80], xmm6
	movaps	xmmword ptr [rbp + -64], xmm7
.Lk_sum_saved:
	mov	rsi, rdi
.Lk_sum_block0:
	# v1 = stack_addr ss0
	lea	rdi, [rbp + -48]
	# intrinsic va_start(v1)
	mov	dword ptr [rdi], 8
	mov	dword ptr [rdi + 4], 48
	lea	rax, [rbp + 16]
	mov	qword ptr [rdi + 8], rax
	lea	rax, [rbp + -224]
	mov	qword ptr [rdi + 16], rax
	# v2 = iconst.i64 0
	xor	r8d, r8d
	# v3 = iconst.i32 0
	xor	r9d, r9d
	# jump block1(v3, v2)
	mov	r11, r8
	mov	r8, r9
	mov	r9, r11
.Lk_sum_block1:
	# v6 = icmp.slt v4, v0
	# brif v6, block2, block3
	cmp	r8d, esi
	jge	".Lk_sum_block3"
.Lk_sum_block2:
	# v7 = va_arg.i64 v1
	mov	ecx, dword ptr [rdi]
	cmp	ecx, 48
	jae	".Lk_sum_inst7_stack"
	mov	rax, qword ptr [rdi + 16]
	add	rax, rcx
	add	ecx, 8
	mov	dword ptr [rdi], ecx
	jmp	".Lk_sum_inst7_load"
.Lk_sum_inst7_stack:
	mov	rax, qword ptr [rdi + 8]
	lea	rcx, [rax + 8]
	mov	qword ptr [rdi + 8], rcx
.Lk_sum_inst7_load:
	mov	rbx, qword ptr [rax]
	# v8 = iadd v5, v7
	add	r9, rbx
	# v9 = iconst.i32 1
	# v10 = iadd v4, v9
	add	r8d, 1
	# jump block1(v10, v8)
	jmp	".Lk_sum_block1"
.Lk_sum_block3:
	# return v5
	mov	rax, r9
	lea	rsp, [rbp + -8]
	pop	rbx
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
	xor	r8d, r8d
	# jump block1(v3, v2)
.Lk_pairs_block1:
	# v6 = icmp.slt v4, v0
	# brif v6, block2, block3
	cmp	r8d, esi
	jge	".Lk_pairs_block3"
.Lk_pairs_block2:
	# v7 = va_arg.i32 v1
	mov	ecx, dword ptr [rdi]
	cmp	ecx, 48
	jae	".Lk_pairs_inst7_stack"
	mov	rax, qword ptr [rdi + 16]
	add	rax, rcx
	add	ecx, 8
	mov	dword ptr [rdi], ecx
	jmp	".Lk_pairs_inst7_load"
.Lk_pairs_inst7_stack:
	mov	rax, qword ptr [rdi + 8]
	lea	rcx, [rax + 8]
	mov	qword ptr [rdi + 8], rcx
.Lk_pairs_inst7_load:
	mov	r9d, dword ptr [rax]
	# v8 = va_arg.f64 v1
	mov	ecx, dword ptr [rdi + 4]
	cmp	ecx, 176
	jae	".Lk_pairs_inst8_stack"
	mov	rax, qword ptr [rdi + 16]
	add	rax, rcx
	add	ecx, 16
	mov	dword ptr [rdi + 4], ecx
	jmp	".Lk_pairs_inst8_load"
.Lk_pairs_inst8_stack:
	mov	rax, qword ptr [rdi + 8]
	lea	rcx, [rax + 8]
	mov	qword ptr [rdi + 8], rcx
.Lk_pairs_inst8_load:
	movsd	xmm1, qword ptr [rax]
	# v9 = sitofp.f64 v7
	xorps	xmm2, xmm2
	cvtsi2sd	xmm2, r9d
	# v10 = fmul v9, v8
	movaps	xmm13, xmm2
	mulsd	xmm13, xmm1
	movaps	xmm1, xmm13
	# v11 = fadd v5, v10
	addsd	xmm0, xmm1
	# v12 = iconst.i32 1
	# v13 = iadd v4, v12
	add	r8d, 1
	# jump block1(v13, v11)
	jmp	".Lk_pairs_block1"
.Lk_pairs_block3:
	# return v5
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
	# v14 = iadd v13, v11
	add	rdi, r9
	# v15 = iadd v14, v12
	add	rdi, r8
	# v16 = iadd v15, v7
	add	rsi, rdi
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
