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
	# v0 = iconst.i32 72
	mov	esi, 72
	# v1 = call @putchar(v0)
	mov	rdi, rsi
	call	"putchar"@PLT
	mov	rsi, rax
	# v2 = iconst.i32 105
	mov	esi, 105
	# v3 = call @putchar(v2)
	mov	rdi, rsi
	call	"putchar"@PLT
	mov	rsi, rax
	# v4 = iconst.i32 33
	mov	esi, 33
	# v5 = call @putchar(v4)
	mov	rdi, rsi
	call	"putchar"@PLT
	mov	rsi, rax
	# v6 = iconst.i32 10
	mov	esi, 10
	# v7 = call @putchar(v6)
	mov	rdi, rsi
	call	"putchar"@PLT
	mov	rsi, rax
	# v8 = iconst.i32 0
	xor	esi, esi
	# return v8
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
