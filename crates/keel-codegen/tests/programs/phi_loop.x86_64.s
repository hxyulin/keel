	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "phi_loop"
	.p2align	4
	.type	"phi_loop",@function
"phi_loop":
.Lphi_loop_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lphi_loop_block0:
	# v2 = iconst.i64 0
	xor	r8d, r8d
	# v3 = iconst.i64 1
	# jump block1(v0, v2)
.Lphi_loop_block1:
	# v6 = icmp.ult v5, v1
	# brif v6, block2, block3(v4)
	cmp	r8, rdi
	jae	".Lphi_loop_block3"
.Lphi_loop_block2:
	# v7 = iadd v5, v3
	add	r8, 1
	# jump block1(v4, v7)
	jmp	".Lphi_loop_block1"
.Lphi_loop_block3:
	# return v8
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lphi_loop_end:
	.cfi_endproc
	.size "phi_loop", .-"phi_loop"
	.section	".text","ax",@progbits
	.globl "edge_transfers"
	.p2align	4
	.type	"edge_transfers",@function
"edge_transfers":
.Ledge_transfers_begin:
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ledge_transfers_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# jump block1(v4, v2, v3)
	mov	r11, r8
	mov	r8, rbx
	mov	rbx, r9
	mov	r9, r11
.Ledge_transfers_block1:
	# v8 = icmp.ult v5, v1
	# brif v8, block2, block6
	cmp	r8, rdi
	jae	".Ledge_transfers_block6"
.Ledge_transfers_block2:
	# v9 = ptradd v0, v5
	lea	r12, [rsi + r8 * 1]
	# v10 = load.i8 v9
	movzx	r12d, byte ptr [r12]
	# v11 = iconst.i8 0
	# v12 = icmp.eq v10, v11
	# brif v12, block3, block4
	cmp	r12b, 0
	je	".Ledge_transfers_block3"
.Ledge_transfers_block4:
	# v14 = xor v6, v7
	xor	r9, rbx
	# jump block5(v7, v14)
	mov	r11, r9
	mov	r9, rbx
	mov	rbx, r11
	jmp	".Ledge_transfers_block5"
.Ledge_transfers_block3:
	# v13 = iadd v6, v7
	add	rbx, r9
	# jump block5(v13, v6)
	mov	r11, r9
	mov	r9, rbx
	mov	rbx, r11
.Ledge_transfers_block5:
	# v17 = iconst.i64 1
	# v18 = iadd v5, v17
	add	r8, 1
	# jump block1(v18, v15, v16)
	jmp	".Ledge_transfers_block1"
.Ledge_transfers_block6:
	# v19 = xor v6, v7
	mov	rsi, r9
	xor	rsi, rbx
	# return v19
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Ledge_transfers_end:
	.cfi_endproc
	.size "edge_transfers", .-"edge_transfers"
	.section	".note.GNU-stack","",@progbits
