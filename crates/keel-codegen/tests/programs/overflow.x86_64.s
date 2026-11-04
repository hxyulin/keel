	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_uadd_i8"
	.p2align	4
	.type	"k_uadd_i8",@function
"k_uadd_i8":
.Lk_uadd_i8_begin:
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
.Lk_uadd_i8_block0:
	# v3, v4 = uadd_overflow v0, v1
	movzx	eax, sil
	movzx	ecx, dil
	add	eax, ecx
	movzx	ecx, al
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_uadd_i8_end:
	.cfi_endproc
	.size "k_uadd_i8", .-"k_uadd_i8"
	.section	".text","ax",@progbits
	.globl "k_sadd_i8"
	.p2align	4
	.type	"k_sadd_i8",@function
"k_sadd_i8":
.Lk_sadd_i8_begin:
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
.Lk_sadd_i8_block0:
	# v3, v4 = sadd_overflow v0, v1
	movsx	eax, sil
	movsx	ecx, dil
	add	eax, ecx
	movsx	ecx, al
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movsx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sadd_i8_end:
	.cfi_endproc
	.size "k_sadd_i8", .-"k_sadd_i8"
	.section	".text","ax",@progbits
	.globl "k_usub_i8"
	.p2align	4
	.type	"k_usub_i8",@function
"k_usub_i8":
.Lk_usub_i8_begin:
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
.Lk_usub_i8_block0:
	# v3, v4 = usub_overflow v0, v1
	movzx	eax, sil
	movzx	ecx, dil
	sub	eax, ecx
	movzx	ecx, al
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_usub_i8_end:
	.cfi_endproc
	.size "k_usub_i8", .-"k_usub_i8"
	.section	".text","ax",@progbits
	.globl "k_ssub_i8"
	.p2align	4
	.type	"k_ssub_i8",@function
"k_ssub_i8":
.Lk_ssub_i8_begin:
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
.Lk_ssub_i8_block0:
	# v3, v4 = ssub_overflow v0, v1
	movsx	eax, sil
	movsx	ecx, dil
	sub	eax, ecx
	movsx	ecx, al
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movsx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ssub_i8_end:
	.cfi_endproc
	.size "k_ssub_i8", .-"k_ssub_i8"
	.section	".text","ax",@progbits
	.globl "k_umul_i8"
	.p2align	4
	.type	"k_umul_i8",@function
"k_umul_i8":
.Lk_umul_i8_begin:
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
.Lk_umul_i8_block0:
	# v3, v4 = umul_overflow v0, v1
	movzx	eax, sil
	movzx	ecx, dil
	mul	ecx
	movzx	ecx, al
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umul_i8_end:
	.cfi_endproc
	.size "k_umul_i8", .-"k_umul_i8"
	.section	".text","ax",@progbits
	.globl "k_smul_i8"
	.p2align	4
	.type	"k_smul_i8",@function
"k_smul_i8":
.Lk_smul_i8_begin:
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
.Lk_smul_i8_block0:
	# v3, v4 = smul_overflow v0, v1
	movsx	eax, sil
	movsx	ecx, dil
	imul	eax, ecx
	movsx	ecx, al
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movsx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smul_i8_end:
	.cfi_endproc
	.size "k_smul_i8", .-"k_smul_i8"
	.section	".text","ax",@progbits
	.globl "k_uadd_i16"
	.p2align	4
	.type	"k_uadd_i16",@function
"k_uadd_i16":
.Lk_uadd_i16_begin:
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
.Lk_uadd_i16_block0:
	# v3, v4 = uadd_overflow v0, v1
	movzx	eax, si
	movzx	ecx, di
	add	eax, ecx
	movzx	ecx, ax
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_uadd_i16_end:
	.cfi_endproc
	.size "k_uadd_i16", .-"k_uadd_i16"
	.section	".text","ax",@progbits
	.globl "k_sadd_i16"
	.p2align	4
	.type	"k_sadd_i16",@function
"k_sadd_i16":
.Lk_sadd_i16_begin:
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
.Lk_sadd_i16_block0:
	# v3, v4 = sadd_overflow v0, v1
	movsx	eax, si
	movsx	ecx, di
	add	eax, ecx
	movsx	ecx, ax
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movsx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sadd_i16_end:
	.cfi_endproc
	.size "k_sadd_i16", .-"k_sadd_i16"
	.section	".text","ax",@progbits
	.globl "k_usub_i16"
	.p2align	4
	.type	"k_usub_i16",@function
"k_usub_i16":
.Lk_usub_i16_begin:
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
.Lk_usub_i16_block0:
	# v3, v4 = usub_overflow v0, v1
	movzx	eax, si
	movzx	ecx, di
	sub	eax, ecx
	movzx	ecx, ax
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_usub_i16_end:
	.cfi_endproc
	.size "k_usub_i16", .-"k_usub_i16"
	.section	".text","ax",@progbits
	.globl "k_ssub_i16"
	.p2align	4
	.type	"k_ssub_i16",@function
"k_ssub_i16":
.Lk_ssub_i16_begin:
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
.Lk_ssub_i16_block0:
	# v3, v4 = ssub_overflow v0, v1
	movsx	eax, si
	movsx	ecx, di
	sub	eax, ecx
	movsx	ecx, ax
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movsx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ssub_i16_end:
	.cfi_endproc
	.size "k_ssub_i16", .-"k_ssub_i16"
	.section	".text","ax",@progbits
	.globl "k_umul_i16"
	.p2align	4
	.type	"k_umul_i16",@function
"k_umul_i16":
.Lk_umul_i16_begin:
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
.Lk_umul_i16_block0:
	# v3, v4 = umul_overflow v0, v1
	movzx	eax, si
	movzx	ecx, di
	mul	ecx
	movzx	ecx, ax
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umul_i16_end:
	.cfi_endproc
	.size "k_umul_i16", .-"k_umul_i16"
	.section	".text","ax",@progbits
	.globl "k_smul_i16"
	.p2align	4
	.type	"k_smul_i16",@function
"k_smul_i16":
.Lk_smul_i16_begin:
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
.Lk_smul_i16_block0:
	# v3, v4 = smul_overflow v0, v1
	movsx	eax, si
	movsx	ecx, di
	imul	eax, ecx
	movsx	ecx, ax
	cmp	eax, ecx
	setne	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	movsx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smul_i16_end:
	.cfi_endproc
	.size "k_smul_i16", .-"k_smul_i16"
	.section	".text","ax",@progbits
	.globl "k_uadd_i32"
	.p2align	4
	.type	"k_uadd_i32",@function
"k_uadd_i32":
.Lk_uadd_i32_begin:
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
.Lk_uadd_i32_block0:
	# v3, v4 = uadd_overflow v0, v1
	mov	rax, rsi
	add	eax, edi
	setb	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_uadd_i32_end:
	.cfi_endproc
	.size "k_uadd_i32", .-"k_uadd_i32"
	.section	".text","ax",@progbits
	.globl "k_sadd_i32"
	.p2align	4
	.type	"k_sadd_i32",@function
"k_sadd_i32":
.Lk_sadd_i32_begin:
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
.Lk_sadd_i32_block0:
	# v3, v4 = sadd_overflow v0, v1
	mov	rax, rsi
	add	eax, edi
	seto	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sadd_i32_end:
	.cfi_endproc
	.size "k_sadd_i32", .-"k_sadd_i32"
	.section	".text","ax",@progbits
	.globl "k_usub_i32"
	.p2align	4
	.type	"k_usub_i32",@function
"k_usub_i32":
.Lk_usub_i32_begin:
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
.Lk_usub_i32_block0:
	# v3, v4 = usub_overflow v0, v1
	mov	rax, rsi
	sub	eax, edi
	setb	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_usub_i32_end:
	.cfi_endproc
	.size "k_usub_i32", .-"k_usub_i32"
	.section	".text","ax",@progbits
	.globl "k_ssub_i32"
	.p2align	4
	.type	"k_ssub_i32",@function
"k_ssub_i32":
.Lk_ssub_i32_begin:
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
.Lk_ssub_i32_block0:
	# v3, v4 = ssub_overflow v0, v1
	mov	rax, rsi
	sub	eax, edi
	seto	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ssub_i32_end:
	.cfi_endproc
	.size "k_ssub_i32", .-"k_ssub_i32"
	.section	".text","ax",@progbits
	.globl "k_umul_i32"
	.p2align	4
	.type	"k_umul_i32",@function
"k_umul_i32":
.Lk_umul_i32_begin:
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
.Lk_umul_i32_block0:
	# v3, v4 = umul_overflow v0, v1
	mov	rax, rsi
	mul	edi
	setb	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umul_i32_end:
	.cfi_endproc
	.size "k_umul_i32", .-"k_umul_i32"
	.section	".text","ax",@progbits
	.globl "k_smul_i32"
	.p2align	4
	.type	"k_smul_i32",@function
"k_smul_i32":
.Lk_smul_i32_begin:
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
.Lk_smul_i32_block0:
	# v3, v4 = smul_overflow v0, v1
	mov	rax, rsi
	imul	eax, edi
	seto	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smul_i32_end:
	.cfi_endproc
	.size "k_smul_i32", .-"k_smul_i32"
	.section	".text","ax",@progbits
	.globl "k_uadd_i64"
	.p2align	4
	.type	"k_uadd_i64",@function
"k_uadd_i64":
.Lk_uadd_i64_begin:
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
.Lk_uadd_i64_block0:
	# v3, v4 = uadd_overflow v0, v1
	mov	rax, rsi
	add	rax, rdi
	setb	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_uadd_i64_end:
	.cfi_endproc
	.size "k_uadd_i64", .-"k_uadd_i64"
	.section	".text","ax",@progbits
	.globl "k_sadd_i64"
	.p2align	4
	.type	"k_sadd_i64",@function
"k_sadd_i64":
.Lk_sadd_i64_begin:
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
.Lk_sadd_i64_block0:
	# v3, v4 = sadd_overflow v0, v1
	mov	rax, rsi
	add	rax, rdi
	seto	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sadd_i64_end:
	.cfi_endproc
	.size "k_sadd_i64", .-"k_sadd_i64"
	.section	".text","ax",@progbits
	.globl "k_usub_i64"
	.p2align	4
	.type	"k_usub_i64",@function
"k_usub_i64":
.Lk_usub_i64_begin:
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
.Lk_usub_i64_block0:
	# v3, v4 = usub_overflow v0, v1
	mov	rax, rsi
	sub	rax, rdi
	setb	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_usub_i64_end:
	.cfi_endproc
	.size "k_usub_i64", .-"k_usub_i64"
	.section	".text","ax",@progbits
	.globl "k_ssub_i64"
	.p2align	4
	.type	"k_ssub_i64",@function
"k_ssub_i64":
.Lk_ssub_i64_begin:
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
.Lk_ssub_i64_block0:
	# v3, v4 = ssub_overflow v0, v1
	mov	rax, rsi
	sub	rax, rdi
	seto	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ssub_i64_end:
	.cfi_endproc
	.size "k_ssub_i64", .-"k_ssub_i64"
	.section	".text","ax",@progbits
	.globl "k_umul_i64"
	.p2align	4
	.type	"k_umul_i64",@function
"k_umul_i64":
.Lk_umul_i64_begin:
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
.Lk_umul_i64_block0:
	# v3, v4 = umul_overflow v0, v1
	mov	rax, rsi
	mul	rdi
	setb	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umul_i64_end:
	.cfi_endproc
	.size "k_umul_i64", .-"k_umul_i64"
	.section	".text","ax",@progbits
	.globl "k_smul_i64"
	.p2align	4
	.type	"k_smul_i64",@function
"k_smul_i64":
.Lk_smul_i64_begin:
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
.Lk_smul_i64_block0:
	# v3, v4 = smul_overflow v0, v1
	mov	rax, rsi
	imul	rax, rdi
	seto	cl
	mov	rsi, rax
	mov	rdi, rcx
	# store v4, v2
	mov	byte ptr [r8], dil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smul_i64_end:
	.cfi_endproc
	.size "k_smul_i64", .-"k_smul_i64"
	.section	".text","ax",@progbits
	.globl "k_uadd_i128"
	.p2align	4
	.type	"k_uadd_i128",@function
"k_uadd_i128":
.Lk_uadd_i128_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r9, rcx
	mov	rbx, r8
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_uadd_i128_block0:
	# v3, v4 = uadd_overflow v0, v1
	mov	rax, rsi
	add	rax, r8
	mov	rdx, rdi
	adc	rdx, r9
	setb	cl
	mov	rsi, rax
	mov	rdi, rdx
	mov	r8, rcx
	# store v4, v2
	mov	byte ptr [rbx], r8b
	# return v3
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_uadd_i128_end:
	.cfi_endproc
	.size "k_uadd_i128", .-"k_uadd_i128"
	.section	".text","ax",@progbits
	.globl "k_sadd_i128"
	.p2align	4
	.type	"k_sadd_i128",@function
"k_sadd_i128":
.Lk_sadd_i128_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r9, rcx
	mov	rbx, r8
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sadd_i128_block0:
	# v3, v4 = sadd_overflow v0, v1
	mov	rax, rsi
	add	rax, r8
	mov	rdx, rdi
	adc	rdx, r9
	seto	cl
	mov	rsi, rax
	mov	rdi, rdx
	mov	r8, rcx
	# store v4, v2
	mov	byte ptr [rbx], r8b
	# return v3
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_sadd_i128_end:
	.cfi_endproc
	.size "k_sadd_i128", .-"k_sadd_i128"
	.section	".text","ax",@progbits
	.globl "k_usub_i128"
	.p2align	4
	.type	"k_usub_i128",@function
"k_usub_i128":
.Lk_usub_i128_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r9, rcx
	mov	rbx, r8
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_usub_i128_block0:
	# v3, v4 = usub_overflow v0, v1
	mov	rax, rsi
	sub	rax, r8
	mov	rdx, rdi
	sbb	rdx, r9
	setb	cl
	mov	rsi, rax
	mov	rdi, rdx
	mov	r8, rcx
	# store v4, v2
	mov	byte ptr [rbx], r8b
	# return v3
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_usub_i128_end:
	.cfi_endproc
	.size "k_usub_i128", .-"k_usub_i128"
	.section	".text","ax",@progbits
	.globl "k_ssub_i128"
	.p2align	4
	.type	"k_ssub_i128",@function
"k_ssub_i128":
.Lk_ssub_i128_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r9, rcx
	mov	rbx, r8
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_ssub_i128_block0:
	# v3, v4 = ssub_overflow v0, v1
	mov	rax, rsi
	sub	rax, r8
	mov	rdx, rdi
	sbb	rdx, r9
	seto	cl
	mov	rsi, rax
	mov	rdi, rdx
	mov	r8, rcx
	# store v4, v2
	mov	byte ptr [rbx], r8b
	# return v3
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_ssub_i128_end:
	.cfi_endproc
	.size "k_ssub_i128", .-"k_ssub_i128"
	.section	".text","ax",@progbits
	.globl "k_live"
	.p2align	4
	.type	"k_live",@function
"k_live":
.Lk_live_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_live_block0:
	# v2, v3 = uadd_overflow v0, v1
	mov	rax, rsi
	add	rax, rdi
	setb	cl
	mov	rbx, rax
	mov	r12, rcx
	# call @clobber()
	call	"clobber"@PLT
	# v4 = uext.i64 v3
	movzx	esi, r12b
	# v5 = iadd v2, v2
	lea	rdi, [rbx + rbx * 1]
	# v6 = iadd v5, v4
	add	rsi, rdi
	# return v6
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_live_end:
	.cfi_endproc
	.size "k_live", .-"k_live"
	.section	".note.GNU-stack","",@progbits
