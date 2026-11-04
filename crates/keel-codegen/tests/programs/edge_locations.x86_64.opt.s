	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "edge_locations"
	.p2align	4
	.type	"edge_locations",@function
"edge_locations":
.Ledge_locations_begin:
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
.Ledge_locations_block0:
	# brif v0, block1(v1), block1(v2)
	test	rsi, rsi
	je	".Ledge_locations_inst0_else"
	mov	rsi, rdi
	jmp	".Ledge_locations_block1"
.Ledge_locations_inst0_else:
	mov	rsi, r8
.Ledge_locations_block1:
	# v4 = iadd v1, v3
	add	rdi, rsi
	# return v4
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Ledge_locations_end:
	.cfi_endproc
	.size "edge_locations", .-"edge_locations"
	.section	".text","ax",@progbits
	.globl "empty_branch"
	.p2align	4
	.type	"empty_branch",@function
"empty_branch":
.Lempty_branch_begin:
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
.Lempty_branch_block0:
	# brif v0, block1, block2
	test	rsi, rsi
	jne	".Lempty_branch_block1"
.Lempty_branch_block2:
	# v4 = iadd v1, v2
	add	rdi, r8
	# return v4
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lempty_branch_block1:
	# v3 = isub v1, v2
	sub	rdi, r8
	# return v3
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lempty_branch_end:
	.cfi_endproc
	.size "empty_branch", .-"empty_branch"
	.section	".text","ax",@progbits
	.globl "empty_switch"
	.p2align	4
	.type	"empty_switch",@function
"empty_switch":
.Lempty_switch_begin:
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
.Lempty_switch_block0:
	# switch v0, block1, [1: block2, 2: block2, 3: block2, 4: block2]
	mov	rax, rsi
	sub	rax, 1
	cmp	rax, 3
	ja	".Lempty_switch_block1"
	lea	rcx, [rip + ".Lempty_switch_inst0_table"]
	movsxd	rax, dword ptr [rcx + rax * 4]
	add	rax, rcx
	jmp	rax
	.p2align	2
.Lempty_switch_inst0_table:
	.long ".Lempty_switch_block2"-".Lempty_switch_inst0_table"
	.long ".Lempty_switch_block2"-".Lempty_switch_inst0_table"
	.long ".Lempty_switch_block2"-".Lempty_switch_inst0_table"
	.long ".Lempty_switch_block2"-".Lempty_switch_inst0_table"
.Lempty_switch_block2:
	# v4 = iadd v1, v2
	add	rdi, r8
	# return v4
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lempty_switch_block1:
	# v3 = isub v1, v2
	sub	rdi, r8
	# return v3
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lempty_switch_end:
	.cfi_endproc
	.size "empty_switch", .-"empty_switch"
	.section	".note.GNU-stack","",@progbits
