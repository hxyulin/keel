	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_printf"
	.p2align	4
	.type	"k_printf",@function
"k_printf":
.Lk_printf_begin:
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
	je	".Lk_printf_saved"
	movaps	xmmword ptr [rbp + -160], xmm0
	movaps	xmmword ptr [rbp + -144], xmm1
	movaps	xmmword ptr [rbp + -128], xmm2
	movaps	xmmword ptr [rbp + -112], xmm3
	movaps	xmmword ptr [rbp + -96], xmm4
	movaps	xmmword ptr [rbp + -80], xmm5
	movaps	xmmword ptr [rbp + -64], xmm6
	movaps	xmmword ptr [rbp + -48], xmm7
.Lk_printf_saved:
	mov	rsi, rdi
.Lk_printf_block0:
	# v1 = stack_addr ss0
	lea	rdi, [rbp + -32]
	# intrinsic va_start(v1)
	mov	dword ptr [rdi], 8
	mov	dword ptr [rdi + 4], 48
	lea	rax, [rbp + 16]
	mov	qword ptr [rdi + 8], rax
	lea	rax, [rbp + -208]
	mov	qword ptr [rdi + 16], rax
	# v2 = call @vprintf(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"vprintf"@PLT
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_printf_end:
	.cfi_endproc
	.size "k_printf", .-"k_printf"
	.section	".note.GNU-stack","",@progbits
