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
	# v0 = iconst.i32 -2147483648
	mov	esi, 2147483648
	# v1 = iconst.i32 -1
	mov	edi, 4294967295
	# v2 = sdiv v0, v1
	mov	eax, esi
	test	edi, edi
	je	".Lmain_trap"
	cmp	edi, -1
	jne	".Lmain_inst2_divide"
	cmp	eax, -2147483648
	je	".Lmain_trap"
.Lmain_inst2_divide:
	cdq
	idiv	edi
.Lmain_inst2_div:
	mov	rsi, rax
	# return v2
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
