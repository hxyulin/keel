	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_fma_f64"
	.p2align	4
	.type	"k_fma_f64",@function
"k_fma_f64":
.Lk_fma_f64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_fma_f64_block0:
	# v3 = fma v0, v1, v2
	call	"fma"@PLT
	# return v3
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_fma_f64_end:
	.cfi_endproc
	.size "k_fma_f64", .-"k_fma_f64"
	.section	".text","ax",@progbits
	.globl "k_fma_f32"
	.p2align	4
	.type	"k_fma_f32",@function
"k_fma_f32":
.Lk_fma_f32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_fma_f32_block0:
	# v3 = fma v0, v1, v2
	call	"fmaf"@PLT
	# return v3
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_fma_f32_end:
	.cfi_endproc
	.size "k_fma_f32", .-"k_fma_f32"
	.section	".text","ax",@progbits
	.globl "k_copysign_f64"
	.p2align	4
	.type	"k_copysign_f64",@function
"k_copysign_f64":
.Lk_copysign_f64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_copysign_f64_block0:
	# v2 = fcopysign v0, v1
	movq	rax, xmm0
	movq	rcx, xmm1
	btr	rax, 63
	shr	rcx, 63
	shl	rcx, 63
	or	rax, rcx
	movq	xmm0, rax
	# return v2
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_copysign_f64_end:
	.cfi_endproc
	.size "k_copysign_f64", .-"k_copysign_f64"
	.section	".text","ax",@progbits
	.globl "k_copysign_f32"
	.p2align	4
	.type	"k_copysign_f32",@function
"k_copysign_f32":
.Lk_copysign_f32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_copysign_f32_block0:
	# v2 = fcopysign v0, v1
	movd	eax, xmm0
	movd	ecx, xmm1
	btr	eax, 31
	shr	ecx, 31
	shl	ecx, 31
	or	eax, ecx
	movd	xmm0, eax
	# return v2
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_copysign_f32_end:
	.cfi_endproc
	.size "k_copysign_f32", .-"k_copysign_f32"
	.section	".text","ax",@progbits
	.globl "k_const_fma"
	.p2align	4
	.type	"k_const_fma",@function
"k_const_fma":
.Lk_const_fma_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_const_fma_block0:
	# v3 = fconst.f64 5.551115123125783e-17
	movabs	rax, 4363988038922010624
	movq	xmm0, rax
	# return v3
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_const_fma_end:
	.cfi_endproc
	.size "k_const_fma", .-"k_const_fma"
	.section	".text","ax",@progbits
	.globl "k_const_copysign"
	.p2align	4
	.type	"k_const_copysign",@function
"k_const_copysign":
.Lk_const_copysign_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_const_copysign_block0:
	# v3 = iconst.i64 -2251799813684957
	movabs	rsi, 18444492273895866659
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_const_copysign_end:
	.cfi_endproc
	.size "k_const_copysign", .-"k_const_copysign"
	.section	".text","ax",@progbits
	.globl "k_const_copysign_f32"
	.p2align	4
	.type	"k_const_copysign_f32",@function
"k_const_copysign_f32":
.Lk_const_copysign_f32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_const_copysign_f32_block0:
	# v3 = iconst.i32 2143289410
	mov	esi, 2143289410
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_const_copysign_f32_end:
	.cfi_endproc
	.size "k_const_copysign_f32", .-"k_const_copysign_f32"
	.section	".note.GNU-stack","",@progbits
