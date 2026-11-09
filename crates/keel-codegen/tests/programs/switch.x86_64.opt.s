	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "dense"
	.p2align	4
	.type	"dense",@function
"dense":
.Ldense_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Ldense_block0:
	# v1 = iconst.i32 100
	mov	edi, 100
	# switch v0, block5, [0: block1, 1: block2, 2: block1, 4: block3(v0), 5: block4, 7: block2, 8: block3(v1)]
	mov	eax, esi
	cmp	eax, 8
	ja	".Ldense_block5"
	lea	rcx, [rip + ".Ldense_inst1_table"]
	movsxd	rax, dword ptr [rcx + rax * 4]
	add	rax, rcx
	jmp	rax
	.p2align	2
.Ldense_inst1_table:
	.long ".Ldense_block1"-".Ldense_inst1_table"
	.long ".Ldense_block2"-".Ldense_inst1_table"
	.long ".Ldense_block1"-".Ldense_inst1_table"
	.long ".Ldense_block5"-".Ldense_inst1_table"
	.long ".Ldense_block3"-".Ldense_inst1_table"
	.long ".Ldense_block4"-".Ldense_inst1_table"
	.long ".Ldense_block5"-".Ldense_inst1_table"
	.long ".Ldense_block2"-".Ldense_inst1_table"
	.long ".Ldense_inst1_edge7"-".Ldense_inst1_table"
.Ldense_inst1_edge7:
	mov	rsi, rdi
	jmp	".Ldense_block3"
.Ldense_block4:
	# v7 = iconst.i32 50
	mov	esi, 50
	# return v7
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ldense_block3:
	# v5 = iconst.i32 1000
	# v6 = iadd v4, v5
	add	esi, 1000
	# return v6
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ldense_block2:
	# v3 = iconst.i32 20
	mov	esi, 20
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ldense_block1:
	# v2 = iconst.i32 10
	mov	esi, 10
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ldense_block5:
	# v8 = iconst.i32 -1
	mov	esi, 4294967295
	# return v8
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ldense_end:
	.cfi_endproc
	.size "dense", .-"dense"
	.section	".text","ax",@progbits
	.globl "dense_off"
	.p2align	4
	.type	"dense_off",@function
"dense_off":
.Ldense_off_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Ldense_off_block0:
	# switch v0, block2, [1000: block1, 1001: block2, 1002: block1, 1003: block1, 1005: block2]
	mov	rax, rsi
	sub	rax, 1000
	cmp	rax, 5
	ja	".Ldense_off_block2"
	lea	rcx, [rip + ".Ldense_off_inst0_table"]
	movsxd	rax, dword ptr [rcx + rax * 4]
	add	rax, rcx
	jmp	rax
	.p2align	2
.Ldense_off_inst0_table:
	.long ".Ldense_off_block1"-".Ldense_off_inst0_table"
	.long ".Ldense_off_block2"-".Ldense_off_inst0_table"
	.long ".Ldense_off_block1"-".Ldense_off_inst0_table"
	.long ".Ldense_off_block1"-".Ldense_off_inst0_table"
	.long ".Ldense_off_block2"-".Ldense_off_inst0_table"
	.long ".Ldense_off_block2"-".Ldense_off_inst0_table"
.Ldense_off_block1:
	# v1 = iconst.i64 7
	mov	esi, 7
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ldense_off_block2:
	# v5 = iconst.i64 1
	# v4 = shl v0, v5
	shl	rsi, 1
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ldense_off_end:
	.cfi_endproc
	.size "dense_off", .-"dense_off"
	.section	".text","ax",@progbits
	.globl "dense_big"
	.p2align	4
	.type	"dense_big",@function
"dense_big":
.Ldense_big_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Ldense_big_block0:
	# v1 = iconst.i32 1
	mov	edi, 1
	# v2 = iconst.i32 2
	mov	r8d, 2
	# v3 = iconst.i32 3
	mov	r9d, 3
	# switch v0, block2(v0), [70000: block2(v1), 70001: block2(v2), 70002: block2(v3), 70003: block1, 70004: block2(v1)]
	mov	eax, esi
	sub	eax, 70000
	cmp	eax, 4
	ja	".Ldense_big_block2"
	lea	rcx, [rip + ".Ldense_big_inst3_table"]
	movsxd	rax, dword ptr [rcx + rax * 4]
	add	rax, rcx
	jmp	rax
	.p2align	2
.Ldense_big_inst3_table:
	.long ".Ldense_big_inst3_edge1"-".Ldense_big_inst3_table"
	.long ".Ldense_big_inst3_edge2"-".Ldense_big_inst3_table"
	.long ".Ldense_big_inst3_edge3"-".Ldense_big_inst3_table"
	.long ".Ldense_big_block1"-".Ldense_big_inst3_table"
	.long ".Ldense_big_inst3_edge5"-".Ldense_big_inst3_table"
.Ldense_big_inst3_edge1:
	mov	rsi, rdi
	jmp	".Ldense_big_block2"
.Ldense_big_inst3_edge2:
	mov	rsi, r8
	jmp	".Ldense_big_block2"
.Ldense_big_inst3_edge3:
	mov	rsi, r9
	jmp	".Ldense_big_block2"
.Ldense_big_inst3_edge5:
	mov	rsi, rdi
	jmp	".Ldense_big_block2"
.Ldense_big_block1:
	# v4 = iconst.i32 99
	mov	esi, 99
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ldense_big_block2:
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ldense_big_end:
	.cfi_endproc
	.size "dense_big", .-"dense_big"
	.section	".text","ax",@progbits
	.globl "sparse"
	.p2align	4
	.type	"sparse",@function
"sparse":
.Lsparse_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lsparse_block0:
	# switch v0, block4, [-1: block1, 4886718345: block2, 5000: block3, 3: block1]
	cmp	rsi, -1
	je	".Lsparse_block1"
	movabs	rcx, 4886718345
	cmp	rsi, rcx
	je	".Lsparse_block2"
	cmp	rsi, 5000
	je	".Lsparse_block3"
	cmp	rsi, 3
	je	".Lsparse_block1"
	jmp	".Lsparse_block4"
.Lsparse_block3:
	# v3 = iconst.i64 33
	mov	esi, 33
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsparse_block2:
	# v2 = iconst.i64 22
	mov	esi, 22
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsparse_block1:
	# v1 = iconst.i64 11
	mov	esi, 11
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsparse_block4:
	# v4 = iconst.i64 44
	mov	esi, 44
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsparse_end:
	.cfi_endproc
	.size "sparse", .-"sparse"
	.section	".text","ax",@progbits
	.globl "narrow"
	.p2align	4
	.type	"narrow",@function
"narrow":
.Lnarrow_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lnarrow_block0:
	# switch v0, block3, [-128: block1, 255: block2, 0: block1, 127: block2]
	movzx	eax, sil
	cmp	eax, 128
	je	".Lnarrow_block1"
	cmp	eax, 255
	je	".Lnarrow_block2"
	cmp	eax, 0
	je	".Lnarrow_block1"
	cmp	eax, 127
	je	".Lnarrow_block2"
	jmp	".Lnarrow_block3"
.Lnarrow_block2:
	# v2 = iconst.i32 2
	mov	esi, 2
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lnarrow_block1:
	# v1 = iconst.i32 1
	mov	esi, 1
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lnarrow_block3:
	# v3 = iconst.i32 3
	mov	esi, 3
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lnarrow_end:
	.cfi_endproc
	.size "narrow", .-"narrow"
	.section	".text","ax",@progbits
	.globl "machine"
	.p2align	4
	.type	"machine",@function
"machine":
.Lmachine_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lmachine_block0:
	# v1 = iconst.i64 0
	xor	edi, edi
	# jump block1(v1, v1, v1)
	mov	r8, rdi
	mov	r9, rdi
.Lmachine_block1:
	# v5 = icmp.ugt v0, v2
	# brif v5, block2, block7(v4)
	cmp	rsi, rdi
	jbe	".Lmachine_block7"
.Lmachine_block2:
	# v6 = iconst.i64 1
	# v7 = iadd v2, v6
	add	rdi, 1
	# switch v3, block6, [0: block3, 1: block4, 2: block5]
	cmp	r8, 0
	je	".Lmachine_block3"
	cmp	r8, 1
	je	".Lmachine_block4"
	cmp	r8, 2
	je	".Lmachine_block5"
	jmp	".Lmachine_block6"
.Lmachine_block5:
	# v18 = isub v4, v7
	sub	r9, rdi
	# v29 = iconst.i64 0
	xor	r8d, r8d
	# jump block1(v7, v29, v18)
	jmp	".Lmachine_block1"
.Lmachine_block4:
	# v13 = iconst.i64 3
	# v14 = imul v4, v13
	imul	r9, r9, 3
	# v15 = iconst.i64 2
	mov	r8d, 2
	# jump block1(v7, v15, v14)
	jmp	".Lmachine_block1"
.Lmachine_block3:
	# v28 = iconst.i64 1
	mov	r8d, 1
	# v10 = iadd v4, v28
	add	r9, 1
	# jump block1(v7, v28, v10)
	jmp	".Lmachine_block1"
.Lmachine_block6:
	# v22 = iconst.i64 -1
	mov	rsi, -1
	# jump block7(v22)
	mov	r9, rsi
.Lmachine_block7:
	# return v23
	mov	rax, r9
	mov	rsp, rbp
	pop	rbp
	ret
.Lmachine_end:
	.cfi_endproc
	.size "machine", .-"machine"
	.section	".text","ax",@progbits
	.globl "constant"
	.p2align	4
	.type	"constant",@function
"constant":
.Lconstant_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lconstant_block0:
	# v2 = iconst.i32 1
	mov	esi, 1
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lconstant_end:
	.cfi_endproc
	.size "constant", .-"constant"
	.section	".note.GNU-stack","",@progbits
