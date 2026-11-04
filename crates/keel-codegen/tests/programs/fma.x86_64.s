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
	# v0 = fconst.f64 0.1
	movabs	rax, 4591870180066957722
	movq	xmm0, rax
	# v1 = fconst.f64 10.0
	movabs	rax, 4621819117588971520
	movq	xmm1, rax
	# v2 = fconst.f64 -1.0
	movabs	rax, 13830554455654793216
	movq	xmm2, rax
	# v3 = fma v0, v1, v2
	call	"fma"@PLT
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
	# v0 = fconst.f64 0x7ff8000000000123
	movabs	rax, 9221120237041090851
	movq	xmm0, rax
	# v1 = fconst.f64 -2.0
	movabs	rax, 13835058055282163712
	movq	xmm1, rax
	# v2 = fcopysign v0, v1
	movq	rax, xmm0
	movq	rcx, xmm1
	btr	rax, 63
	shr	rcx, 63
	shl	rcx, 63
	or	rax, rcx
	movq	xmm0, rax
	# v3 = bitcast.i64 v2
	movq	rsi, xmm0
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
	# v0 = fconst.f32 0xffc00042
	movabs	rax, 4290773058
	movq	xmm0, rax
	# v1 = fconst.f32 0.0
	xorps	xmm1, xmm1
	# v2 = fcopysign v0, v1
	movd	eax, xmm0
	movd	ecx, xmm1
	btr	eax, 31
	shr	ecx, 31
	shl	ecx, 31
	or	eax, ecx
	movd	xmm0, eax
	# v3 = bitcast.i32 v2
	movd	esi, xmm0
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_const_copysign_f32_end:
	.cfi_endproc
	.size "k_const_copysign_f32", .-"k_const_copysign_f32"
	.section	".note.GNU-stack","",@progbits
