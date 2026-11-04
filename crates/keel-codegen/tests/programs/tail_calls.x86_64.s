	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_sum"
	.p2align	4
	.type	"k_sum",@function
"k_sum":
.Lk_sum_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sum_block0:
	# v2 = iconst.i64 0
	# v3 = icmp.eq v0, v2
	# brif v3, block1, block2
	cmp	rsi, 0
	je	".Lk_sum_block1"
.Lk_sum_block2:
	# v4 = iadd v1, v0
	add	rdi, rsi
	# v5 = iconst.i64 1
	# v6 = isub v0, v5
	sub	rsi, 1
	# return_call @k_sum(v6, v4)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	mov	rsp, rbp
	pop	rbp
	jmp	"k_sum"
.Lk_sum_block1:
	# return v1
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sum_end:
	.cfi_endproc
	.size "k_sum", .-"k_sum"
	.section	".text","ax",@progbits
	.globl "k_even"
	.p2align	4
	.type	"k_even",@function
"k_even":
.Lk_even_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_even_block0:
	# v1 = iconst.i64 0
	# v2 = icmp.eq v0, v1
	# brif v2, block1, block2
	cmp	rsi, 0
	je	".Lk_even_block1"
.Lk_even_block2:
	# v4 = iconst.i64 1
	# v5 = isub v0, v4
	sub	rsi, 1
	# return_call @k_odd(v5)
	mov	rdi, rsi
	mov	rsp, rbp
	pop	rbp
	jmp	"k_odd"
.Lk_even_block1:
	# v3 = iconst.i32 1
	mov	esi, 1
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_even_end:
	.cfi_endproc
	.size "k_even", .-"k_even"
	.section	".text","ax",@progbits
	.globl "k_odd"
	.p2align	4
	.type	"k_odd",@function
"k_odd":
.Lk_odd_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_odd_block0:
	# v1 = iconst.i64 0
	# v2 = icmp.eq v0, v1
	# brif v2, block1, block2
	cmp	rsi, 0
	je	".Lk_odd_block1"
.Lk_odd_block2:
	# v4 = iconst.i64 1
	# v5 = isub v0, v4
	sub	rsi, 1
	# return_call @k_even(v5)
	mov	rdi, rsi
	mov	rsp, rbp
	pop	rbp
	jmp	"k_even"
.Lk_odd_block1:
	# v3 = iconst.i32 0
	xor	esi, esi
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_odd_end:
	.cfi_endproc
	.size "k_odd", .-"k_odd"
	.section	".text","ax",@progbits
	.globl "k_indirect"
	.p2align	4
	.type	"k_indirect",@function
"k_indirect":
.Lk_indirect_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_indirect_block0:
	# v2 = iconst.i64 0
	# v3 = icmp.eq v1, v2
	# brif v3, block1, block2
	cmp	rdi, 0
	je	".Lk_indirect_block1"
.Lk_indirect_block2:
	# v5 = iconst.i64 1
	# v6 = isub v1, v5
	sub	rdi, 1
	# return_call_indirect v0(v0, v6) : (ptr, i64) -> i64 cc(c)
	mov	r10, rsi
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	mov	rsp, rbp
	pop	rbp
	jmp	r10
.Lk_indirect_block1:
	# v4 = iconst.i64 77
	mov	esi, 77
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_indirect_end:
	.cfi_endproc
	.size "k_indirect", .-"k_indirect"
	.section	".text","ax",@progbits
	.globl "k_to_c"
	.p2align	4
	.type	"k_to_c",@function
"k_to_c":
.Lk_to_c_begin:
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
	push	r14
	.cfi_offset 14, -48
	push	r15
	.cfi_offset 15, -56
	sub	rsp, 40
	mov	qword ptr [rbp + -64], rdi
.Lk_to_c_block0:
	# v8 = iconst.i64 1
	# v9 = iadd v0, v8
	mov	rax, qword ptr [rbp + -64]
	add	rax, 1
	mov	qword ptr [rbp + -56], rax
	# v10 = iadd v9, v8
	mov	rax, qword ptr [rbp + -56]
	add	rax, 1
	mov	qword ptr [rbp + -48], rax
	# v11 = iadd v10, v8
	mov	rax, qword ptr [rbp + -48]
	lea	r14, [rax + 1]
	# v12 = iadd v11, v8
	lea	r15, [r14 + 1]
	# v13 = iadd v12, v8
	lea	rbx, [r15 + 1]
	# v14 = iadd v13, v8
	lea	r12, [rbx + 1]
	# v15 = iadd v14, v8
	lea	r13, [r12 + 1]
	# call @clobber()
	call	"clobber"@PLT
	# return_call @c_sum8(v0, v9, v10, v11, v12, v13, v14, v15)
	mov	qword ptr [rsp], r12
	mov	qword ptr [rsp + 8], r13
	mov	rdi, qword ptr [rbp + -64]
	mov	rsi, qword ptr [rbp + -56]
	mov	rdx, qword ptr [rbp + -48]
	mov	rcx, r14
	mov	r8, r15
	mov	r9, rbx
	mov	rax, qword ptr [rsp]
	mov	qword ptr [rbp + 16], rax
	mov	rax, qword ptr [rsp + 8]
	mov	qword ptr [rbp + 24], rax
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	jmp	"c_sum8"@PLT
.Lk_to_c_end:
	.cfi_endproc
	.size "k_to_c", .-"k_to_c"
	.section	".text","ax",@progbits
	.globl "k_to_c_indirect"
	.p2align	4
	.type	"k_to_c_indirect",@function
"k_to_c_indirect":
.Lk_to_c_indirect_begin:
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
	sub	rsp, 16
	mov	rbx, rdi
	mov	r12, rsi
.Lk_to_c_indirect_block0:
	# call @clobber()
	call	"clobber"@PLT
	# return_call_indirect v0(v1, v1, v1, v1, v1, v1, v1, v1) : (i64, i64, i64, i64, i64, i64, i64, i64) -> i64 cc(c)
	mov	qword ptr [rsp], r12
	mov	qword ptr [rsp + 8], r12
	mov	rdi, r12
	mov	rsi, r12
	mov	rdx, r12
	mov	rcx, r12
	mov	r8, r12
	mov	r9, r12
	mov	r10, rbx
	mov	rax, qword ptr [rsp]
	mov	qword ptr [rbp + 16], rax
	mov	rax, qword ptr [rsp + 8]
	mov	qword ptr [rbp + 24], rax
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	jmp	r10
.Lk_to_c_indirect_end:
	.cfi_endproc
	.size "k_to_c_indirect", .-"k_to_c_indirect"
	.section	".text","ax",@progbits
	.globl "k_narrow"
	.p2align	4
	.type	"k_narrow",@function
"k_narrow":
.Lk_narrow_begin:
	.cfi_startproc
	.cfi_def_cfa 7, 8
	jmp	"k_negate"
.Lk_narrow_end:
	.cfi_endproc
	.size "k_narrow", .-"k_narrow"
	.section	".text","ax",@progbits
	.globl "k_negate"
	.p2align	4
	.type	"k_negate",@function
"k_negate":
.Lk_negate_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_negate_block0:
	# v1 = ineg v0
	neg	esi
	# return v1
	mov	rax, rsi
	movsx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_negate_end:
	.cfi_endproc
	.size "k_negate", .-"k_negate"
	.section	".note.GNU-stack","",@progbits
