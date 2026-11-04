	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "tree_iadd32"
	.p2align	4
	.type	"tree_iadd32",@function
"tree_iadd32":
.Ltree_iadd32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_iadd32_block0:
	# v2 = iconst.i32 34
	# v3 = shl v1, v2
	# v4 = iadd v0, v3
	lea	esi, [rsi + rdi * 4]
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_iadd32_end:
	.cfi_endproc
	.size "tree_iadd32", .-"tree_iadd32"
	.section	".text","ax",@progbits
	.globl "tree_isub32"
	.p2align	4
	.type	"tree_isub32",@function
"tree_isub32":
.Ltree_isub32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_isub32_block0:
	# v2 = iconst.i32 34
	# v3 = shl v1, v2
	shl	edi, 2
	# v4 = isub v0, v3
	sub	esi, edi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_isub32_end:
	.cfi_endproc
	.size "tree_isub32", .-"tree_isub32"
	.section	".text","ax",@progbits
	.globl "tree_and32"
	.p2align	4
	.type	"tree_and32",@function
"tree_and32":
.Ltree_and32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_and32_block0:
	# v2 = iconst.i32 34
	# v3 = shl v1, v2
	shl	edi, 2
	# v4 = and v0, v3
	and	esi, edi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_and32_end:
	.cfi_endproc
	.size "tree_and32", .-"tree_and32"
	.section	".text","ax",@progbits
	.globl "tree_or32"
	.p2align	4
	.type	"tree_or32",@function
"tree_or32":
.Ltree_or32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_or32_block0:
	# v2 = iconst.i32 34
	# v3 = shl v1, v2
	shl	edi, 2
	# v4 = or v0, v3
	or	esi, edi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_or32_end:
	.cfi_endproc
	.size "tree_or32", .-"tree_or32"
	.section	".text","ax",@progbits
	.globl "tree_xor32"
	.p2align	4
	.type	"tree_xor32",@function
"tree_xor32":
.Ltree_xor32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_xor32_block0:
	# v2 = iconst.i32 34
	# v3 = shl v1, v2
	shl	edi, 2
	# v4 = xor v0, v3
	xor	esi, edi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_xor32_end:
	.cfi_endproc
	.size "tree_xor32", .-"tree_xor32"
	.section	".text","ax",@progbits
	.globl "tree_muladd32"
	.p2align	4
	.type	"tree_muladd32",@function
"tree_muladd32":
.Ltree_muladd32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_muladd32_block0:
	# v3 = imul v0, v1
	imul	esi, edi
	# v4 = iadd v2, v3
	add	r8d, esi
	# return v4
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_muladd32_end:
	.cfi_endproc
	.size "tree_muladd32", .-"tree_muladd32"
	.section	".text","ax",@progbits
	.globl "tree_mulsub32"
	.p2align	4
	.type	"tree_mulsub32",@function
"tree_mulsub32":
.Ltree_mulsub32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_mulsub32_block0:
	# v3 = imul v0, v1
	imul	esi, edi
	# v4 = isub v2, v3
	sub	r8d, esi
	# return v4
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_mulsub32_end:
	.cfi_endproc
	.size "tree_mulsub32", .-"tree_mulsub32"
	.section	".text","ax",@progbits
	.globl "tree_iadd64"
	.p2align	4
	.type	"tree_iadd64",@function
"tree_iadd64":
.Ltree_iadd64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_iadd64_block0:
	# v2 = iconst.i64 66
	# v3 = shl v1, v2
	# v4 = iadd v0, v3
	lea	rsi, [rsi + rdi * 4]
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_iadd64_end:
	.cfi_endproc
	.size "tree_iadd64", .-"tree_iadd64"
	.section	".text","ax",@progbits
	.globl "tree_isub64"
	.p2align	4
	.type	"tree_isub64",@function
"tree_isub64":
.Ltree_isub64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_isub64_block0:
	# v2 = iconst.i64 66
	# v3 = shl v1, v2
	shl	rdi, 2
	# v4 = isub v0, v3
	sub	rsi, rdi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_isub64_end:
	.cfi_endproc
	.size "tree_isub64", .-"tree_isub64"
	.section	".text","ax",@progbits
	.globl "tree_and64"
	.p2align	4
	.type	"tree_and64",@function
"tree_and64":
.Ltree_and64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_and64_block0:
	# v2 = iconst.i64 66
	# v3 = shl v1, v2
	shl	rdi, 2
	# v4 = and v0, v3
	and	rsi, rdi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_and64_end:
	.cfi_endproc
	.size "tree_and64", .-"tree_and64"
	.section	".text","ax",@progbits
	.globl "tree_or64"
	.p2align	4
	.type	"tree_or64",@function
"tree_or64":
.Ltree_or64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_or64_block0:
	# v2 = iconst.i64 66
	# v3 = shl v1, v2
	shl	rdi, 2
	# v4 = or v0, v3
	or	rsi, rdi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_or64_end:
	.cfi_endproc
	.size "tree_or64", .-"tree_or64"
	.section	".text","ax",@progbits
	.globl "tree_xor64"
	.p2align	4
	.type	"tree_xor64",@function
"tree_xor64":
.Ltree_xor64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_xor64_block0:
	# v2 = iconst.i64 66
	# v3 = shl v1, v2
	shl	rdi, 2
	# v4 = xor v0, v3
	xor	rsi, rdi
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_xor64_end:
	.cfi_endproc
	.size "tree_xor64", .-"tree_xor64"
	.section	".text","ax",@progbits
	.globl "tree_muladd64"
	.p2align	4
	.type	"tree_muladd64",@function
"tree_muladd64":
.Ltree_muladd64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_muladd64_block0:
	# v3 = imul v0, v1
	imul	rsi, rdi
	# v4 = iadd v2, v3
	add	r8, rsi
	# return v4
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_muladd64_end:
	.cfi_endproc
	.size "tree_muladd64", .-"tree_muladd64"
	.section	".text","ax",@progbits
	.globl "tree_mulsub64"
	.p2align	4
	.type	"tree_mulsub64",@function
"tree_mulsub64":
.Ltree_mulsub64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_mulsub64_block0:
	# v3 = imul v0, v1
	imul	rsi, rdi
	# v4 = isub v2, v3
	sub	r8, rsi
	# return v4
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_mulsub64_end:
	.cfi_endproc
	.size "tree_mulsub64", .-"tree_mulsub64"
	.section	".text","ax",@progbits
	.globl "tree_across_call"
	.p2align	4
	.type	"tree_across_call",@function
"tree_across_call":
.Ltree_across_call_begin:
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
	mov	rbx, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_across_call_block0:
	# v3 = imul v0, v1
	mov	r12, rsi
	imul	r12, rdi
	# call @selection_clobber()
	call	"selection_clobber"@PLT
	# v4 = iadd v2, v3
	add	rbx, r12
	# return v4
	mov	rax, rbx
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Ltree_across_call_end:
	.cfi_endproc
	.size "tree_across_call", .-"tree_across_call"
	.section	".text","ax",@progbits
	.globl "tree_shared"
	.p2align	4
	.type	"tree_shared",@function
"tree_shared":
.Ltree_shared_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_shared_block0:
	# v3 = imul v0, v1
	imul	rsi, rdi
	# v4 = iadd v2, v3
	add	r8, rsi
	# v5 = xor v3, v4
	xor	rsi, r8
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_shared_end:
	.cfi_endproc
	.size "tree_shared", .-"tree_shared"
	.section	".text","ax",@progbits
	.globl "tree_constant"
	.p2align	4
	.type	"tree_constant",@function
"tree_constant":
.Ltree_constant_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ltree_constant_block0:
	# v2 = iconst.i64 7
	# v3 = imul v0, v2
	imul	rsi, rsi, 7
	# v4 = iadd v1, v3
	add	rdi, rsi
	# return v4
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Ltree_constant_end:
	.cfi_endproc
	.size "tree_constant", .-"tree_constant"
	.section	".note.GNU-stack","",@progbits
