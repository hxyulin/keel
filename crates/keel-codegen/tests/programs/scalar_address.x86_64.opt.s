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
	# v3 = load.i8 v2
	movzx	esi, byte ptr [rsi + 8]
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
	# store v1, v3
	mov	word ptr [rsi + 14], di
	# v12 = iconst.i64 14
	# v13 = ptradd v0, v12
	# v6 = load.i16 v13
	movzx	esi, word ptr [rsi + 14]
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
	mov	rbx, rdi
.Lscalar_call_block0:
	# v1 = iconst.i64 8
	# v2 = ptradd v0, v1
	# call @scalar_touch(v0)
	mov	rdi, rbx
	call	"scalar_touch"@PLT
	# v3 = load.i8 v2
	movzx	esi, byte ptr [rbx + 8]
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
	# v8 = ptradd v1, v6
	# v9 = ptradd v2, v6
	# v10 = ptradd v3, v6
	# v11 = ptradd v4, v6
	# v12 = ptradd v5, v6
	# v13 = load.i8 v7
	movzx	esi, byte ptr [rsi + 8]
	# v14 = load.i8 v8
	movzx	edi, byte ptr [rdi + 8]
	# v15 = load.i8 v9
	movzx	r8d, byte ptr [r8 + 8]
	# v16 = load.i8 v10
	movzx	r9d, byte ptr [r9 + 8]
	# v17 = load.i8 v11
	movzx	ebx, byte ptr [rbx + 8]
	# v18 = load.i8 v12
	movzx	r12d, byte ptr [r12 + 8]
	# v19 = uext.i64 v13
	# v20 = uext.i64 v14
	# v21 = uext.i64 v15
	# v22 = uext.i64 v16
	# v23 = uext.i64 v17
	# v24 = uext.i64 v18
	# v25 = iadd v19, v20
	add	rsi, rdi
	# v26 = iadd v21, v22
	add	r8, r9
	# v27 = iadd v23, v24
	add	rbx, r12
	# v28 = iadd v25, v26
	add	rsi, r8
	# v29 = iadd v27, v28
	add	rbx, rsi
	# return v29
	mov	rax, rbx
	lea	rsp, [rbp + -16]
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
	# v5 = icmp.sge v0, v1
	# brif v5, block1, block2
	cmp	rsi, rdi
	jge	".Lscalar_bool_block1"
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
	# v3 = iconst.i32 0
	# v6 = icmp.ne v2, v3
	test	esi, 15
	setne	sil
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
