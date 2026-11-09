	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "hot32p"
	.p2align	4
	.type	"hot32p",@function
"hot32p":
.Lhot32p_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot32p_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# v16 = imul v1, v2
	imul	r8d, edi
	# v8 = icmp.ugt v0, v4
	# brif v8, block2(v3, v4, v16), block3(v3)
	cmp	esi, 0
	jbe	".Lhot32p_block3"
.Lhot32p_block2:
	# v10 = iadd v21, v23
	add	r9d, r8d
	# v11 = iconst.i32 1
	# v14 = iadd v22, v11
	add	ebx, 1
	# v20 = iadd v1, v23
	add	r8d, edi
	# v27 = icmp.ugt v0, v14
	# brif v27, block2(v10, v14, v20), block3(v10)
	cmp	esi, ebx
	ja	".Lhot32p_block2"
.Lhot32p_block3:
	# return v24
	mov	rax, r9
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lhot32p_end:
	.cfi_endproc
	.size "hot32p", .-"hot32p"
	.section	".text","ax",@progbits
	.globl "hot32m"
	.p2align	4
	.type	"hot32m",@function
"hot32m":
.Lhot32m_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot32m_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# v16 = imul v1, v2
	imul	r8d, edi
	# v17 = iconst.i32 -7
	# v18 = imul v1, v17
	imul	edi, edi, 4294967289
	# v8 = icmp.ugt v0, v4
	# brif v8, block2(v3, v4, v16), block3(v3)
	cmp	esi, 0
	jbe	".Lhot32m_block3"
.Lhot32m_block2:
	# v10 = iadd v21, v23
	add	r9d, r8d
	# v13 = iconst.i32 1
	# v14 = iadd v22, v13
	add	ebx, 1
	# v20 = iadd v18, v23
	add	r8d, edi
	# v27 = icmp.ugt v0, v14
	# brif v27, block2(v10, v14, v20), block3(v10)
	cmp	esi, ebx
	ja	".Lhot32m_block2"
.Lhot32m_block3:
	# return v24
	mov	rax, r9
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lhot32m_end:
	.cfi_endproc
	.size "hot32m", .-"hot32m"
	.section	".text","ax",@progbits
	.globl "hot32z"
	.p2align	4
	.type	"hot32z",@function
"hot32z":
.Lhot32z_begin:
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot32z_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# v8 = icmp.ugt v0, v4
	# brif v8, block2(v3, v4), block3(v3)
	cmp	esi, 0
	jbe	".Lhot32z_block3"
.Lhot32z_block2:
	# v9 = imul v1, v2
	mov	r12, rdi
	imul	r12d, r8d
	# v10 = iadd v16, v9
	add	r9d, r12d
	# v13 = iconst.i32 1
	# v14 = iadd v17, v13
	add	ebx, 1
	# v20 = icmp.ugt v0, v14
	# brif v20, block2(v10, v14), block3(v10)
	cmp	esi, ebx
	ja	".Lhot32z_block2"
.Lhot32z_block3:
	# return v18
	mov	rax, r9
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lhot32z_end:
	.cfi_endproc
	.size "hot32z", .-"hot32z"
	.section	".text","ax",@progbits
	.globl "hot64p"
	.p2align	4
	.type	"hot64p",@function
"hot64p":
.Lhot64p_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot64p_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# v16 = imul v1, v2
	imul	r8, rdi
	# v8 = icmp.ugt v0, v4
	# brif v8, block2(v3, v4, v16), block3(v3)
	cmp	rsi, 0
	jbe	".Lhot64p_block3"
.Lhot64p_block2:
	# v10 = iadd v21, v23
	add	r9, r8
	# v11 = iconst.i64 1
	# v14 = iadd v22, v11
	add	rbx, 1
	# v20 = iadd v1, v23
	add	r8, rdi
	# v27 = icmp.ugt v0, v14
	# brif v27, block2(v10, v14, v20), block3(v10)
	cmp	rsi, rbx
	ja	".Lhot64p_block2"
.Lhot64p_block3:
	# return v24
	mov	rax, r9
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lhot64p_end:
	.cfi_endproc
	.size "hot64p", .-"hot64p"
	.section	".text","ax",@progbits
	.globl "hot64m"
	.p2align	4
	.type	"hot64m",@function
"hot64m":
.Lhot64m_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot64m_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# v16 = imul v1, v2
	imul	r8, rdi
	# v17 = iconst.i64 -7
	# v18 = imul v1, v17
	imul	rdi, rdi, -7
	# v8 = icmp.ugt v0, v4
	# brif v8, block2(v3, v4, v16), block3(v3)
	cmp	rsi, 0
	jbe	".Lhot64m_block3"
.Lhot64m_block2:
	# v10 = iadd v21, v23
	add	r9, r8
	# v13 = iconst.i64 1
	# v14 = iadd v22, v13
	add	rbx, 1
	# v20 = iadd v18, v23
	add	r8, rdi
	# v27 = icmp.ugt v0, v14
	# brif v27, block2(v10, v14, v20), block3(v10)
	cmp	rsi, rbx
	ja	".Lhot64m_block2"
.Lhot64m_block3:
	# return v24
	mov	rax, r9
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lhot64m_end:
	.cfi_endproc
	.size "hot64m", .-"hot64m"
	.section	".text","ax",@progbits
	.globl "hot64z"
	.p2align	4
	.type	"hot64z",@function
"hot64z":
.Lhot64z_begin:
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot64z_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# v8 = icmp.ugt v0, v4
	# brif v8, block2(v3, v4), block3(v3)
	cmp	rsi, 0
	jbe	".Lhot64z_block3"
.Lhot64z_block2:
	# v9 = imul v1, v2
	mov	r12, rdi
	imul	r12, r8
	# v10 = iadd v16, v9
	add	r9, r12
	# v13 = iconst.i64 1
	# v14 = iadd v17, v13
	add	rbx, 1
	# v20 = icmp.ugt v0, v14
	# brif v20, block2(v10, v14), block3(v10)
	cmp	rsi, rbx
	ja	".Lhot64z_block2"
.Lhot64z_block3:
	# return v18
	mov	rax, r9
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lhot64z_end:
	.cfi_endproc
	.size "hot64z", .-"hot64z"
	.section	".text","ax",@progbits
	.globl "offset32"
	.p2align	4
	.type	"offset32",@function
"offset32":
.Loffset32_begin:
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Loffset32_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# v5 = iconst.i64 17
	mov	r12d, 17
	# v17 = iadd v2, v3
	add	r8d, r9d
	# v9 = icmp.ugt v0, v4
	# brif v9, block2(v17, v4, v5), block3(v5)
	cmp	esi, 0
	jbe	".Loffset32_block3"
.Loffset32_block2:
	# v11 = sext.i64 v18
	movsxd	r9, r8d
	# v12 = iadd v20, v11
	add	r12, r9
	# v13 = iadd v1, v18
	add	r8d, edi
	# v14 = iconst.i32 1
	# v15 = iadd v19, v14
	add	ebx, 1
	# v24 = icmp.ugt v0, v15
	# brif v24, block2(v13, v15, v12), block3(v12)
	cmp	esi, ebx
	ja	".Loffset32_block2"
.Loffset32_block3:
	# return v23
	mov	rax, r12
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Loffset32_end:
	.cfi_endproc
	.size "offset32", .-"offset32"
	.section	".text","ax",@progbits
	.globl "offset64"
	.p2align	4
	.type	"offset64",@function
"offset64":
.Loffset64_begin:
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Loffset64_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# v5 = iconst.i64 17
	mov	r12d, 17
	# v16 = iadd v2, v3
	add	r8, r9
	# v9 = icmp.ugt v0, v4
	# brif v9, block2(v16, v4, v5), block3(v5)
	cmp	rsi, 0
	jbe	".Loffset64_block3"
.Loffset64_block2:
	# v11 = iadd v17, v19
	lea	r9, [r8 + r12 * 1]
	# v12 = iadd v1, v17
	add	r8, rdi
	# v13 = iconst.i64 1
	# v14 = iadd v18, v13
	add	rbx, 1
	# v23 = icmp.ugt v0, v14
	# brif v23, block2(v12, v14, v11), block3(v11)
	cmp	rsi, rbx
	jbe	".Loffset64_inst10_else"
	mov	r12, r9
	jmp	".Loffset64_block2"
.Loffset64_inst10_else:
	mov	r12, r9
.Loffset64_block3:
	# return v22
	mov	rax, r12
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Loffset64_end:
	.cfi_endproc
	.size "offset64", .-"offset64"
	.section	".note.GNU-stack","",@progbits
