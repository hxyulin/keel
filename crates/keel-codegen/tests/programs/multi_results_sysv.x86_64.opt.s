	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_mixed"
	.p2align	4
	.type	"k_mixed",@function
"k_mixed":
.Lk_mixed_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_mixed_block0:
	# v2 = fconst.f64 0.5
	movabs	rax, 4602678819172646912
	movq	xmm1, rax
	# v3 = fadd v1, v2
	addsd	xmm0, xmm1
	# return v3, v0
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_mixed_end:
	.cfi_endproc
	.size "k_mixed", .-"k_mixed"
	.section	".text","ax",@progbits
	.globl "k_narrow_float"
	.p2align	4
	.type	"k_narrow_float",@function
"k_narrow_float":
.Lk_narrow_float_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_narrow_float_block0:
	# return v0, v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_narrow_float_end:
	.cfi_endproc
	.size "k_narrow_float", .-"k_narrow_float"
	.section	".text","ax",@progbits
	.globl "k_two_sse"
	.p2align	4
	.type	"k_two_sse",@function
"k_two_sse":
.Lk_two_sse_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_two_sse_block0:
	# return v0, v1
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_two_sse_end:
	.cfi_endproc
	.size "k_two_sse", .-"k_two_sse"
	.section	".text","ax",@progbits
	.globl "k_call_mixed"
	.p2align	4
	.type	"k_call_mixed",@function
"k_call_mixed":
.Lk_call_mixed_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
	movsd	qword ptr [rbp + -16], xmm0
.Lk_call_mixed_block0:
	# v2, v3 = call @c_mixed(v0, v1)
	mov	rdi, rsi
	movsd	xmm0, qword ptr [rbp + -16]
	call	"c_mixed"@PLT
	mov	rsi, rax
	# v4 = sitofp.f64 v2
	xorps	xmm1, xmm1
	cvtsi2sd	xmm1, rsi
	# v5 = fadd v4, v3
	addsd	xmm1, xmm0
	# v6 = fadd v5, v1
	movsd	xmm14, qword ptr [rbp + -16]
	addsd	xmm1, xmm14
	# return v6
	movaps	xmm0, xmm1
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_call_mixed_end:
	.cfi_endproc
	.size "k_call_mixed", .-"k_call_mixed"
	.section	".note.GNU-stack","",@progbits
