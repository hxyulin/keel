	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "main"
	.p2align	4
	.type	"main",@function
"main":
.Lmain_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lmain_block0:
	# v0 = iconst.i128 1
	mov	eax, 1
	xor	edx, edx
	mov	rsi, rax
	mov	rdi, rdx
	# v1 = iconst.i32 127
	mov	r8d, 127
	# v2 = shl v0, v1
	mov	rcx, r8
	mov	rax, rsi
	mov	rdx, rdi
	shld	rdx, rax, cl
	shl	rax, cl
	test	cl, 64
	je	".Lmain_inst2_shifted"
	mov	rdx, rax
	xor	eax, eax
.Lmain_inst2_shifted:
	mov	rsi, rax
	mov	rdi, rdx
	# v3 = iconst.i128 -1
	mov	rax, -1
	mov	rdx, -1
	mov	r8, rax
	mov	r9, rdx
	# v4 = sdiv v2, v3
	mov	rax, r8
	or	rax, r9
	je	".Lmain_trap"
	mov	rax, r8
	and	rax, r9
	cmp	rax, -1
	jne	".Lmain_inst4_divok"
	mov	rax, rsi
	test	rax, rax
	jne	".Lmain_inst4_divok"
	movabs	rax, 9223372036854775808
	cmp	rax, rdi
	je	".Lmain_trap"
.Lmain_inst4_divok:
	mov	rdx, r8
	mov	rcx, r9
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"__divti3"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# v5 = trunc.i32 v4
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmain_trap:
	ud2
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
