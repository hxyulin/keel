	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_pass"
	.p2align	4
	.type	"k_pass",@function
"k_pass":
.Lk_pass_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_pass_block0:
	# v1 = trunc.i8 v0
	mov	rdi, rsi
	# v2 = trunc.i16 v0
	# v3 = call @c_raw(v1, v2, v1, v2)
	mov	rdx, rdi
	mov	rcx, rsi
	movsx	edi, dil
	movsx	esi, si
	movzx	edx, dl
	movzx	ecx, cx
	call	"c_raw"@PLT
	mov	rsi, rax
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_end:
	.cfi_endproc
	.size "k_pass", .-"k_pass"
	.section	".text","ax",@progbits
	.globl "k_pass_stack"
	.p2align	4
	.type	"k_pass_stack",@function
"k_pass_stack":
.Lk_pass_stack_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 48
	mov	rsi, rdi
.Lk_pass_stack_block0:
	# v1 = trunc.i8 v0
	mov	rdi, rsi
	# v2 = trunc.i16 v0
	mov	r8, rsi
	# v3 = trunc.i32 v0
	mov	r9, rsi
	# v4 = call @c_stack(v0, v0, v0, v0, v0, v0, v0, v0, v1, v2, v1, v3)
	mov	qword ptr [rsp], rsi
	mov	qword ptr [rsp + 8], rsi
	mov	qword ptr [rsp + 16], rdi
	mov	qword ptr [rsp + 24], r8
	mov	qword ptr [rsp + 32], rdi
	mov	qword ptr [rsp + 40], r9
	mov	rdi, rsi
	mov	rdx, rsi
	mov	rcx, rsi
	mov	r8, rsi
	mov	r9, rsi
	call	"c_stack"@PLT
	mov	rsi, rax
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_stack_end:
	.cfi_endproc
	.size "k_pass_stack", .-"k_pass_stack"
	.section	".text","ax",@progbits
	.globl "k_ret_s8"
	.p2align	4
	.type	"k_ret_s8",@function
"k_ret_s8":
.Lk_ret_s8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_ret_s8_block0:
	# v1 = trunc.i8 v0
	# return v1
	mov	rax, rsi
	movsx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ret_s8_end:
	.cfi_endproc
	.size "k_ret_s8", .-"k_ret_s8"
	.section	".text","ax",@progbits
	.globl "k_ret_u8"
	.p2align	4
	.type	"k_ret_u8",@function
"k_ret_u8":
.Lk_ret_u8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_ret_u8_block0:
	# v1 = trunc.i8 v0
	# return v1
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ret_u8_end:
	.cfi_endproc
	.size "k_ret_u8", .-"k_ret_u8"
	.section	".text","ax",@progbits
	.globl "k_ret_s16"
	.p2align	4
	.type	"k_ret_s16",@function
"k_ret_s16":
.Lk_ret_s16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_ret_s16_block0:
	# v1 = trunc.i16 v0
	# return v1
	mov	rax, rsi
	movsx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ret_s16_end:
	.cfi_endproc
	.size "k_ret_s16", .-"k_ret_s16"
	.section	".text","ax",@progbits
	.globl "k_ret_u16"
	.p2align	4
	.type	"k_ret_u16",@function
"k_ret_u16":
.Lk_ret_u16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_ret_u16_block0:
	# v1 = trunc.i16 v0
	# return v1
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ret_u16_end:
	.cfi_endproc
	.size "k_ret_u16", .-"k_ret_u16"
	.section	".text","ax",@progbits
	.globl "k_use_ret"
	.p2align	4
	.type	"k_use_ret",@function
"k_use_ret":
.Lk_use_ret_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_use_ret_block0:
	# v0 = call @c_ret_s8()
	call	"c_ret_s8"@PLT
	mov	rsi, rax
	# v1 = sext.i32 v0
	movsx	esi, sil
	# v2 = iconst.i32 1
	# v3 = iadd v1, v2
	add	esi, 1
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_use_ret_end:
	.cfi_endproc
	.size "k_use_ret", .-"k_use_ret"
	.section	".note.GNU-stack","",@progbits
