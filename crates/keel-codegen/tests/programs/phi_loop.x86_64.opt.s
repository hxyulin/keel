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
	# jump block1(v2)
.Lphi_loop_block1:
	# v6 = icmp.ugt v1, v5
	# brif v6, block2, block3
	cmp	rdi, r8
	ja	".Lphi_loop_block2"
.Lphi_loop_block3:
	# return v0
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lphi_loop_block2:
	# v3 = iconst.i64 1
	# v7 = iadd v5, v3
	add	r8, 1
	# jump block1(v7)
	jmp	".Lphi_loop_block1"
.Lphi_loop_end:
	.cfi_endproc
	.size "phi_loop", .-"phi_loop"
	.section	".note.GNU-stack","",@progbits
