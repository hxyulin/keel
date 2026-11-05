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
	push	r12
	.cfi_offset 12, -32
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot32p_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# jump block1(v2, v3, v4)
.Lhot32p_block1:
	# v8 = icmp.ult v7, v0
	# brif v8, block2, block3(v6)
	cmp	ebx, esi
	jb	".Lhot32p_block2"
	mov	rsi, r9
.Lhot32p_block3:
	# return v15
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lhot32p_block2:
	# v9 = imul v5, v1
	mov	r12, r8
	imul	r12d, edi
	# v10 = iadd v6, v9
	add	r9d, r12d
	# v11 = iconst.i32 1
	# v12 = iadd v5, v11
	add	r8d, 1
	# v13 = iconst.i32 1
	# v14 = iadd v7, v13
	add	ebx, 1
	# jump block1(v12, v10, v14)
	jmp	".Lhot32p_block1"
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
	push	r12
	.cfi_offset 12, -32
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot32m_block0:
	# v4 = iconst.i32 0
	xor	ebx, ebx
	# jump block1(v2, v3, v4)
.Lhot32m_block1:
	# v8 = icmp.ult v7, v0
	# brif v8, block2, block3(v6)
	cmp	ebx, esi
	jb	".Lhot32m_block2"
	mov	rsi, r9
.Lhot32m_block3:
	# return v15
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lhot32m_block2:
	# v9 = imul v5, v1
	mov	r12, r8
	imul	r12d, edi
	# v10 = iadd v6, v9
	add	r9d, r12d
	# v11 = iconst.i32 -7
	# v12 = iadd v5, v11
	add	r8d, 4294967289
	# v13 = iconst.i32 1
	# v14 = iadd v7, v13
	add	ebx, 1
	# jump block1(v12, v10, v14)
	jmp	".Lhot32m_block1"
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
	# jump block1(v2, v3, v4)
.Lhot32z_block1:
	# v8 = icmp.ult v7, v0
	# brif v8, block2, block3(v6)
	cmp	ebx, esi
	jb	".Lhot32z_block2"
	mov	rsi, r9
.Lhot32z_block3:
	# return v15
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lhot32z_block2:
	# v9 = imul v5, v1
	mov	r12, r8
	imul	r12d, edi
	# v10 = iadd v6, v9
	add	r9d, r12d
	# v11 = iconst.i32 0
	# v12 = iadd v5, v11
	add	r8d, 0
	# v13 = iconst.i32 1
	# v14 = iadd v7, v13
	add	ebx, 1
	# jump block1(v12, v10, v14)
	jmp	".Lhot32z_block1"
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
	push	r12
	.cfi_offset 12, -32
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot64p_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# jump block1(v2, v3, v4)
.Lhot64p_block1:
	# v8 = icmp.ult v7, v0
	# brif v8, block2, block3(v6)
	cmp	rbx, rsi
	jb	".Lhot64p_block2"
	mov	rsi, r9
.Lhot64p_block3:
	# return v15
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lhot64p_block2:
	# v9 = imul v5, v1
	mov	r12, r8
	imul	r12, rdi
	# v10 = iadd v6, v9
	add	r9, r12
	# v11 = iconst.i64 1
	# v12 = iadd v5, v11
	add	r8, 1
	# v13 = iconst.i64 1
	# v14 = iadd v7, v13
	add	rbx, 1
	# jump block1(v12, v10, v14)
	jmp	".Lhot64p_block1"
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
	push	r12
	.cfi_offset 12, -32
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lhot64m_block0:
	# v4 = iconst.i64 0
	xor	ebx, ebx
	# jump block1(v2, v3, v4)
.Lhot64m_block1:
	# v8 = icmp.ult v7, v0
	# brif v8, block2, block3(v6)
	cmp	rbx, rsi
	jb	".Lhot64m_block2"
	mov	rsi, r9
.Lhot64m_block3:
	# return v15
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lhot64m_block2:
	# v9 = imul v5, v1
	mov	r12, r8
	imul	r12, rdi
	# v10 = iadd v6, v9
	add	r9, r12
	# v11 = iconst.i64 -7
	# v12 = iadd v5, v11
	add	r8, -7
	# v13 = iconst.i64 1
	# v14 = iadd v7, v13
	add	rbx, 1
	# jump block1(v12, v10, v14)
	jmp	".Lhot64m_block1"
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
	# jump block1(v2, v3, v4)
.Lhot64z_block1:
	# v8 = icmp.ult v7, v0
	# brif v8, block2, block3(v6)
	cmp	rbx, rsi
	jb	".Lhot64z_block2"
	mov	rsi, r9
.Lhot64z_block3:
	# return v15
	mov	rax, rsi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lhot64z_block2:
	# v9 = imul v5, v1
	mov	r12, r8
	imul	r12, rdi
	# v10 = iadd v6, v9
	add	r9, r12
	# v11 = iconst.i64 0
	# v12 = iadd v5, v11
	add	r8, 0
	# v13 = iconst.i64 1
	# v14 = iadd v7, v13
	add	rbx, 1
	# jump block1(v12, v10, v14)
	jmp	".Lhot64z_block1"
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
	push	r13
	.cfi_offset 13, -40
	sub	rsp, 8
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
	# jump block1(v2, v4, v5)
.Loffset32_block1:
	# v9 = icmp.ult v7, v0
	# brif v9, block2, block3(v8)
	cmp	ebx, esi
	jb	".Loffset32_block2"
	mov	rsi, r12
.Loffset32_block3:
	# return v16
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Loffset32_block2:
	# v10 = iadd v6, v3
	lea	r13d, [r8 + r9 * 1]
	# v11 = sext.i64 v10
	movsxd	r13, r13d
	# v12 = iadd v8, v11
	add	r12, r13
	# v13 = iadd v6, v1
	add	r8d, edi
	# v14 = iconst.i32 1
	# v15 = iadd v7, v14
	add	ebx, 1
	# jump block1(v13, v15, v12)
	jmp	".Loffset32_block1"
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
	push	r13
	.cfi_offset 13, -40
	sub	rsp, 8
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
	# jump block1(v2, v4, v5)
.Loffset64_block1:
	# v9 = icmp.ult v7, v0
	# brif v9, block2, block3(v8)
	cmp	rbx, rsi
	jb	".Loffset64_block2"
	mov	rsi, r12
.Loffset64_block3:
	# return v15
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Loffset64_block2:
	# v10 = iadd v6, v3
	lea	r13, [r8 + r9 * 1]
	# v11 = iadd v8, v10
	add	r12, r13
	# v12 = iadd v6, v1
	add	r8, rdi
	# v13 = iconst.i64 1
	# v14 = iadd v7, v13
	add	rbx, 1
	# jump block1(v12, v14, v11)
	jmp	".Loffset64_block1"
.Loffset64_end:
	.cfi_endproc
	.size "offset64", .-"offset64"
	.section	".note.GNU-stack","",@progbits
