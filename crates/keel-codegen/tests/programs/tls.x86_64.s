	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_bump"
	.p2align	4
	.type	"k_bump",@function
"k_bump":
.Lk_bump_begin:
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
.Lk_bump_block0:
	# v0 = symbol_addr @counter
	mov	rsi, qword ptr fs:[0]
	lea	rsi, [rsi + "counter"@TPOFF]
	# v1 = load.i64 v0
	mov	rdi, qword ptr [rsi]
	# v2 = iconst.i64 1
	# v3 = iadd v1, v2
	lea	rbx, [rdi + 1]
	# store v3, v0
	mov	qword ptr [rsi], rbx
	# v4 = symbol_addr @zeroed
	mov	r12, qword ptr fs:[0]
	lea	r12, [r12 + "zeroed"@TPOFF]
	# call @clobber()
	call	"clobber"@PLT
	# v5 = load.i32 v4
	mov	esi, dword ptr [r12]
	# v6 = iconst.i32 2
	# v7 = iadd v5, v6
	add	esi, 2
	# store v7, v4
	mov	dword ptr [r12], esi
	# v8 = symbol_addr @c_tls
	mov	rdi, qword ptr [rip + "c_tls"@GOTTPOFF]
	add	rdi, qword ptr fs:[0]
	# v9 = load.i64 v8
	mov	rdi, qword ptr [rdi]
	# v10 = sext.i64 v7
	movsxd	rsi, esi
	# v11 = iadd v3, v10
	add	rsi, rbx
	# v12 = iadd v11, v9
	add	rsi, rdi
	# return v12
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_bump_end:
	.cfi_endproc
	.size "k_bump", .-"k_bump"
	.section	".tdata","waT",@progbits
	.globl "counter"
	.type	"counter",@object
	.size "counter", 8
	.p2align	3
"counter":
	.quad 5
	.section	".tbss","waT",@nobits
	.globl "zeroed"
	.type	"zeroed",@object
	.size "zeroed", 4
	.p2align	2
"zeroed":
	.space	4
	.section	".note.GNU-stack","",@progbits
