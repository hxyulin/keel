	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "scalar_read8"
	.p2align	4
	.type	"scalar_read8",@function
"scalar_read8":
.Lscalar_read8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lscalar_read8_block0:
	# v1 = iconst.i64 8
	# v2 = ptradd v0, v1
	add	rsi, 8
	# v3 = load.i8 v2
	movzx	esi, byte ptr [rsi]
	# v4 = sext.i64 v3
	movsx	rsi, sil
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lscalar_read8_end:
	.cfi_endproc
	.size "scalar_read8", .-"scalar_read8"
	.section	".text","ax",@progbits
	.globl "scalar_write16"
	.p2align	4
	.type	"scalar_write16",@function
"scalar_write16":
.Lscalar_write16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lscalar_write16_block0:
	# v2 = iconst.i64 14
	# v3 = ptradd v0, v2
	lea	r8, [rsi + 14]
	# store v1, v3
	mov	word ptr [r8], di
	# v4 = iconst.i64 14
	# v5 = ptradd v0, v4
	add	rsi, 14
	# v6 = load.i16 v5
	movzx	esi, word ptr [rsi]
	# v7 = uext.i64 v6
	# return v7
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lscalar_write16_end:
	.cfi_endproc
	.size "scalar_write16", .-"scalar_write16"
	.section	".text","ax",@progbits
	.globl "scalar_call"
	.p2align	4
	.type	"scalar_call",@function
"scalar_call":
.Lscalar_call_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rsi, rdi
.Lscalar_call_block0:
	# v1 = iconst.i64 8
	# v2 = ptradd v0, v1
	lea	rbx, [rsi + 8]
	# call @scalar_touch(v0)
	mov	rdi, rsi
	call	"scalar_touch"@PLT
	# v3 = load.i8 v2
	movzx	esi, byte ptr [rbx]
	# v4 = sext.i64 v3
	movsx	rsi, sil
	# return v4
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lscalar_call_end:
	.cfi_endproc
	.size "scalar_call", .-"scalar_call"
	.section	".text","ax",@progbits
	.globl "scalar_pressure"
	.p2align	4
	.type	"scalar_pressure",@function
"scalar_pressure":
.Lscalar_pressure_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lscalar_pressure_block0:
	# v6 = iconst.i64 8
	# v7 = ptradd v0, v6
	add	rsi, 8
	# v8 = ptradd v1, v6
	add	rdi, 8
	# v9 = ptradd v2, v6
	add	r8, 8
	# v10 = ptradd v3, v6
	add	r9, 8
	# v11 = ptradd v4, v6
	add	rbx, 8
	# v12 = ptradd v5, v6
	add	r12, 8
	# v13 = load.i8 v7
	movzx	esi, byte ptr [rsi]
	# v14 = load.i8 v8
	movzx	edi, byte ptr [rdi]
	# v15 = load.i8 v9
	movzx	r8d, byte ptr [r8]
	# v16 = load.i8 v10
	movzx	r9d, byte ptr [r9]
	# v17 = load.i8 v11
	movzx	ebx, byte ptr [rbx]
	# v18 = load.i8 v12
	movzx	r12d, byte ptr [r12]
	# v19 = uext.i64 v13
	# v20 = uext.i64 v14
	# v21 = uext.i64 v15
	# v22 = uext.i64 v16
	# v23 = uext.i64 v17
	# v24 = uext.i64 v18
	# v25 = iadd v19, v20
	add	rsi, rdi
	# v26 = iadd v21, v22
	lea	rdi, [r8 + r9 * 1]
	# v27 = iadd v23, v24
	lea	r8, [rbx + r12 * 1]
	# v28 = iadd v25, v26
	add	rsi, rdi
	# v29 = iadd v28, v27
	add	rsi, r8
	# return v29
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lscalar_pressure_end:
	.cfi_endproc
	.size "scalar_pressure", .-"scalar_pressure"
	.section	".text","ax",@progbits
	.globl "scalar_bool"
	.p2align	4
	.type	"scalar_bool",@function
"scalar_bool":
.Lscalar_bool_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lscalar_bool_block0:
	# v2 = icmp.slt v0, v1
	cmp	rsi, rdi
	setl	r8b
	# v3 = uext.i32 v2
	movzx	r8d, r8b
	# v4 = iconst.i32 0
	# v5 = icmp.eq v3, v4
	cmp	r8d, 0
	sete	r8b
	# brif v5, block1, block2
	test	r8b, r8b
	jne	".Lscalar_bool_block1"
.Lscalar_bool_block2:
	# return v1
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lscalar_bool_block1:
	# return v0
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lscalar_bool_end:
	.cfi_endproc
	.size "scalar_bool", .-"scalar_bool"
	.section	".text","ax",@progbits
	.globl "scalar_bool_value"
	.p2align	4
	.type	"scalar_bool_value",@function
"scalar_bool_value":
.Lscalar_bool_value_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lscalar_bool_value_block0:
	# v1 = iconst.i32 15
	# v2 = and v0, v1
	and	esi, 15
	# v3 = iconst.i32 0
	# v4 = icmp.eq v2, v3
	cmp	esi, 0
	sete	sil
	# v5 = uext.i32 v4
	movzx	esi, sil
	# v6 = icmp.eq v5, v3
	cmp	esi, 0
	sete	sil
	# v7 = uext.i32 v6
	movzx	esi, sil
	# return v7
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lscalar_bool_value_end:
	.cfi_endproc
	.size "scalar_bool_value", .-"scalar_bool_value"
	.section	".note.GNU-stack","",@progbits
