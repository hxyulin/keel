	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.p2align	4
	.type	"checked_peek",@function
"checked_peek":
.Lchecked_peek_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 160
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lchecked_peek_block0:
	# v2 = iconst.i64 16
	# v3 = ptradd v0, v2
	lea	r8, [rsi + 16]
	# v4 = load.i64 v3
	mov	r8, qword ptr [r8]
	# v5, v6 = uadd_overflow v1, v4
	mov	rax, rdi
	add	rax, r8
	setb	cl
	mov	rdi, rax
	mov	r8, rcx
	# v7 = iconst.i8 1
	# v8 = xor v6, v7
	xor	r8d, 1
	# brif v8, block1, block2
	test	r8b, r8b
	je	".Lchecked_peek_block2"
.Lchecked_peek_block1:
	# v9 = iconst.i64 8
	# v10 = ptradd v0, v9
	lea	r8, [rsi + 8]
	# v11 = load.i64 v10
	mov	r8, qword ptr [r8]
	# v12 = icmp.uge v5, v11
	# brif v12, block3, block4
	cmp	rdi, r8
	jae	".Lchecked_peek_block3"
.Lchecked_peek_block4:
	# v16 = icmp.ult v5, v11
	# brif v16, block5, block6
	cmp	rdi, r8
	jae	".Lchecked_peek_block6"
.Lchecked_peek_block5:
	# v17 = load.ptr v0
	mov	rsi, qword ptr [rsi]
	# v18 = ptradd v17, v5
	add	rsi, rdi
	# v19 = load.i8 v18
	movzx	esi, byte ptr [rsi]
	# return v19
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lchecked_peek_block6:
	# v20 = stack_addr ss1
	lea	rsi, [rbp + -96]
	# v21 = iconst.i64 2
	# store v21, v20
	mov	qword ptr [rbp + -96], 2
	# call @checked_panic(v20)
	mov	rdi, rsi
	call	"checked_panic"@PLT
	# trap
	ud2
.Lchecked_peek_block3:
	# v15 = iconst.i8 0
	xor	esi, esi
	# return v15
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lchecked_peek_block2:
	# v13 = stack_addr ss0
	lea	rsi, [rbp + -160]
	# v14 = iconst.i64 1
	# store v14, v13
	mov	qword ptr [rbp + -160], 1
	# call @checked_panic(v13)
	mov	rdi, rsi
	call	"checked_panic"@PLT
	# trap
	ud2
.Lchecked_peek_end:
	.cfi_endproc
	.size "checked_peek", .-"checked_peek"
	.section	".text","ax",@progbits
	.globl "checked_peek_public"
	.p2align	4
	.type	"checked_peek_public",@function
"checked_peek_public":
.Lchecked_peek_public_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lchecked_peek_public_block0:
	# v2 = call @checked_peek(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"checked_peek"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lchecked_peek_public_end:
	.cfi_endproc
	.size "checked_peek_public", .-"checked_peek_public"
	.section	".note.GNU-stack","",@progbits
