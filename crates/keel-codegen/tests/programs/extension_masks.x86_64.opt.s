	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "mask_uext32"
	.p2align	4
	.type	"mask_uext32",@function
"mask_uext32":
.Lmask_uext32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lmask_uext32_block0:
	# v1 = trunc.i32 v0
	# v2 = uext.i64 v1
	mov	esi, esi
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmask_uext32_end:
	.cfi_endproc
	.size "mask_uext32", .-"mask_uext32"
	.section	".text","ax",@progbits
	.globl "mask_sext32"
	.p2align	4
	.type	"mask_sext32",@function
"mask_sext32":
.Lmask_sext32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lmask_sext32_block0:
	# v1 = trunc.i32 v0
	# v2 = iconst.i32 2147483647
	# v3 = and v1, v2
	and	esi, 2147483647
	# v4 = uext.i64 v3
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmask_sext32_end:
	.cfi_endproc
	.size "mask_sext32", .-"mask_sext32"
	.section	".text","ax",@progbits
	.globl "mask_sign32"
	.p2align	4
	.type	"mask_sign32",@function
"mask_sign32":
.Lmask_sign32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lmask_sign32_block0:
	# v1 = trunc.i32 v0
	# v2 = iconst.i32 -2147483648
	# v3 = and v1, v2
	and	esi, 2147483648
	# v4 = sext.i64 v3
	movsxd	rsi, esi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmask_sign32_end:
	.cfi_endproc
	.size "mask_sign32", .-"mask_sign32"
	.section	".text","ax",@progbits
	.globl "mask_wide8"
	.p2align	4
	.type	"mask_wide8",@function
"mask_wide8":
.Lmask_wide8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lmask_wide8_block0:
	# v1 = trunc.i8 v0
	# v5 = uext.i64 v1
	movzx	esi, sil
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmask_wide8_end:
	.cfi_endproc
	.size "mask_wide8", .-"mask_wide8"
	.section	".text","ax",@progbits
	.globl "mask_wide_sign"
	.p2align	4
	.type	"mask_wide_sign",@function
"mask_wide_sign":
.Lmask_wide_sign_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lmask_wide_sign_block0:
	# v5 = iconst.i128 0
	xor	eax, eax
	xor	edx, edx
	mov	rsi, rax
	mov	rdi, rdx
	# v6 = trunc.i64 v5
	# return v6
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmask_wide_sign_end:
	.cfi_endproc
	.size "mask_wide_sign", .-"mask_wide_sign"
	.section	".note.GNU-stack","",@progbits
