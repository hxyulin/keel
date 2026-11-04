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
	# v0 = iconst.i32 1
	mov	esi, 1
	# brif v0, block1, block2
	test	esi, esi
	jne	".Lmain_block1"
.Lmain_block2:
	# return v0
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmain_block1:
	# trap
	ud2
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
