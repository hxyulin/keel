	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_pass_s8"
	.p2align	4
	.type	"k_pass_s8",@function
"k_pass_s8":
.Lk_pass_s8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_pass_s8_block0:
	# v1 = call @c_s8(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rbp + -16], rax
	mov	rdi, qword ptr [rbp + -16]
	call	"c_s8"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_s8_end:
	.cfi_endproc
	.size "k_pass_s8", .-"k_pass_s8"
	.section	".text","ax",@progbits
	.globl "k_pass_s16"
	.p2align	4
	.type	"k_pass_s16",@function
"k_pass_s16":
.Lk_pass_s16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_pass_s16_block0:
	# v1 = call @c_s16(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rbp + -16], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rbp + -8], rax
	mov	rdi, qword ptr [rbp + -16]
	mov	rsi, qword ptr [rbp + -8]
	call	"c_s16"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_s16_end:
	.cfi_endproc
	.size "k_pass_s16", .-"k_pass_s16"
	.section	".text","ax",@progbits
	.globl "k_pass_s24"
	.p2align	4
	.type	"k_pass_s24",@function
"k_pass_s24":
.Lk_pass_s24_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 32
	mov	rsi, rdi
.Lk_pass_s24_block0:
	# v1 = call @c_s24(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rsp], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rsp + 8], rax
	mov	rax, qword ptr [rsi + 16]
	mov	qword ptr [rsp + 16], rax
	call	"c_s24"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_s24_end:
	.cfi_endproc
	.size "k_pass_s24", .-"k_pass_s24"
	.section	".text","ax",@progbits
	.globl "k_pass_d2"
	.p2align	4
	.type	"k_pass_d2",@function
"k_pass_d2":
.Lk_pass_d2_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_pass_d2_block0:
	# v1 = call @c_d2(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rbp + -16], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rbp + -8], rax
	movsd	xmm0, qword ptr [rbp + -16]
	movsd	xmm1, qword ptr [rbp + -8]
	call	"c_d2"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_d2_end:
	.cfi_endproc
	.size "k_pass_d2", .-"k_pass_d2"
	.section	".text","ax",@progbits
	.globl "k_pass_f3"
	.p2align	4
	.type	"k_pass_f3",@function
"k_pass_f3":
.Lk_pass_f3_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_pass_f3_block0:
	# v1 = call @c_f3(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rbp + -16], rax
	mov	eax, dword ptr [rsi + 8]
	mov	qword ptr [rbp + -8], rax
	movsd	xmm0, qword ptr [rbp + -16]
	movsd	xmm1, qword ptr [rbp + -8]
	call	"c_f3"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_f3_end:
	.cfi_endproc
	.size "k_pass_f3", .-"k_pass_f3"
	.section	".text","ax",@progbits
	.globl "k_pass_f4"
	.p2align	4
	.type	"k_pass_f4",@function
"k_pass_f4":
.Lk_pass_f4_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_pass_f4_block0:
	# v1 = call @c_f4(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rbp + -16], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rbp + -8], rax
	movsd	xmm0, qword ptr [rbp + -16]
	movsd	xmm1, qword ptr [rbp + -8]
	call	"c_f4"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_f4_end:
	.cfi_endproc
	.size "k_pass_f4", .-"k_pass_f4"
	.section	".text","ax",@progbits
	.globl "k_pass_dl"
	.p2align	4
	.type	"k_pass_dl",@function
"k_pass_dl":
.Lk_pass_dl_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_pass_dl_block0:
	# v1 = call @c_dl(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rbp + -16], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rbp + -8], rax
	movsd	xmm0, qword ptr [rbp + -16]
	mov	rdi, qword ptr [rbp + -8]
	call	"c_dl"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_dl_end:
	.cfi_endproc
	.size "k_pass_dl", .-"k_pass_dl"
	.section	".text","ax",@progbits
	.globl "k_pass_c3"
	.p2align	4
	.type	"k_pass_c3",@function
"k_pass_c3":
.Lk_pass_c3_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_pass_c3_block0:
	# v1 = call @c_c3(v0)
	movzx	eax, word ptr [rsi]
	movzx	edx, byte ptr [rsi + 2]
	shl	rdx, 16
	or	rax, rdx
	mov	qword ptr [rbp + -16], rax
	mov	rdi, qword ptr [rbp + -16]
	call	"c_c3"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_c3_end:
	.cfi_endproc
	.size "k_pass_c3", .-"k_pass_c3"
	.section	".text","ax",@progbits
	.globl "k_pass_n8"
	.p2align	4
	.type	"k_pass_n8",@function
"k_pass_n8":
.Lk_pass_n8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_pass_n8_block0:
	# v1 = call @c_n8(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rbp + -16], rax
	mov	rdi, qword ptr [rbp + -16]
	call	"c_n8"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_n8_end:
	.cfi_endproc
	.size "k_pass_n8", .-"k_pass_n8"
	.section	".text","ax",@progbits
	.globl "k_pass_big"
	.p2align	4
	.type	"k_pass_big",@function
"k_pass_big":
.Lk_pass_big_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 48
	mov	rsi, rdi
.Lk_pass_big_block0:
	# v1 = call @c_big(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rsp], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rsp + 8], rax
	mov	rax, qword ptr [rsi + 16]
	mov	qword ptr [rsp + 16], rax
	mov	rax, qword ptr [rsi + 24]
	mov	qword ptr [rsp + 24], rax
	mov	rax, qword ptr [rsi + 32]
	mov	qword ptr [rsp + 32], rax
	call	"c_big"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_big_end:
	.cfi_endproc
	.size "k_pass_big", .-"k_pass_big"
	.section	".text","ax",@progbits
	.globl "k_s8"
	.p2align	4
	.type	"k_s8",@function
"k_s8":
.Lk_s8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	qword ptr [rbp + -16], rdi
	lea	rsi, [rbp + -16]
.Lk_s8_block0:
	# v1 = call @h_s8(v0)
	mov	rdi, rsi
	call	"h_s8"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_s8_end:
	.cfi_endproc
	.size "k_s8", .-"k_s8"
	.section	".text","ax",@progbits
	.globl "k_s16"
	.p2align	4
	.type	"k_s16",@function
"k_s16":
.Lk_s16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	qword ptr [rbp + -16], rdi
	mov	qword ptr [rbp + -8], rsi
	lea	rsi, [rbp + -16]
.Lk_s16_block0:
	# v1 = call @h_s16(v0)
	mov	rdi, rsi
	call	"h_s16"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_s16_end:
	.cfi_endproc
	.size "k_s16", .-"k_s16"
	.section	".text","ax",@progbits
	.globl "k_s24"
	.p2align	4
	.type	"k_s24",@function
"k_s24":
.Lk_s24_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	lea	rsi, [rbp + 16]
.Lk_s24_block0:
	# v1 = call @h_s24(v0)
	mov	rdi, rsi
	call	"h_s24"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_s24_end:
	.cfi_endproc
	.size "k_s24", .-"k_s24"
	.section	".text","ax",@progbits
	.globl "k_d2"
	.p2align	4
	.type	"k_d2",@function
"k_d2":
.Lk_d2_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	movsd	qword ptr [rbp + -16], xmm0
	movsd	qword ptr [rbp + -8], xmm1
	lea	rsi, [rbp + -16]
.Lk_d2_block0:
	# v1 = call @h_d2(v0)
	mov	rdi, rsi
	call	"h_d2"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_d2_end:
	.cfi_endproc
	.size "k_d2", .-"k_d2"
	.section	".text","ax",@progbits
	.globl "k_f3"
	.p2align	4
	.type	"k_f3",@function
"k_f3":
.Lk_f3_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	movsd	qword ptr [rbp + -16], xmm0
	movsd	qword ptr [rbp + -8], xmm1
	lea	rsi, [rbp + -16]
.Lk_f3_block0:
	# v1 = call @h_f3(v0)
	mov	rdi, rsi
	call	"h_f3"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_f3_end:
	.cfi_endproc
	.size "k_f3", .-"k_f3"
	.section	".text","ax",@progbits
	.globl "k_f4"
	.p2align	4
	.type	"k_f4",@function
"k_f4":
.Lk_f4_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	movsd	qword ptr [rbp + -16], xmm0
	movsd	qword ptr [rbp + -8], xmm1
	lea	rsi, [rbp + -16]
.Lk_f4_block0:
	# v1 = call @h_f4(v0)
	mov	rdi, rsi
	call	"h_f4"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_f4_end:
	.cfi_endproc
	.size "k_f4", .-"k_f4"
	.section	".text","ax",@progbits
	.globl "k_dl"
	.p2align	4
	.type	"k_dl",@function
"k_dl":
.Lk_dl_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	movsd	qword ptr [rbp + -16], xmm0
	mov	qword ptr [rbp + -8], rdi
	lea	rsi, [rbp + -16]
.Lk_dl_block0:
	# v1 = call @h_dl(v0)
	mov	rdi, rsi
	call	"h_dl"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_dl_end:
	.cfi_endproc
	.size "k_dl", .-"k_dl"
	.section	".text","ax",@progbits
	.globl "k_c3"
	.p2align	4
	.type	"k_c3",@function
"k_c3":
.Lk_c3_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	qword ptr [rbp + -16], rdi
	lea	rsi, [rbp + -16]
.Lk_c3_block0:
	# v1 = call @h_c3(v0)
	mov	rdi, rsi
	call	"h_c3"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_c3_end:
	.cfi_endproc
	.size "k_c3", .-"k_c3"
	.section	".text","ax",@progbits
	.globl "k_n8"
	.p2align	4
	.type	"k_n8",@function
"k_n8":
.Lk_n8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	qword ptr [rbp + -16], rdi
	lea	rsi, [rbp + -16]
.Lk_n8_block0:
	# v1 = call @h_n8(v0)
	mov	rdi, rsi
	call	"h_n8"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_n8_end:
	.cfi_endproc
	.size "k_n8", .-"k_n8"
	.section	".text","ax",@progbits
	.globl "k_big"
	.p2align	4
	.type	"k_big",@function
"k_big":
.Lk_big_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	lea	rsi, [rbp + 16]
.Lk_big_block0:
	# v1 = call @h_big(v0)
	mov	rdi, rsi
	call	"h_big"@PLT
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_big_end:
	.cfi_endproc
	.size "k_big", .-"k_big"
	.section	".text","ax",@progbits
	.globl "k_after7"
	.p2align	4
	.type	"k_after7",@function
"k_after7":
.Lk_after7_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
	mov	r13, qword ptr [rbp + 16]
	lea	r14, [rbp + 24]
	mov	r15, qword ptr [rbp + 40]
.Lk_after7_block0:
	# v9 = call @h_after7(v0, v1, v2, v3, v4, v5, v6, v7, v8)
	mov	qword ptr [rsp], r13
	mov	qword ptr [rsp + 8], r14
	mov	qword ptr [rsp + 16], r15
	mov	rdx, r8
	mov	rcx, r9
	mov	r8, rbx
	mov	r9, r12
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"h_after7"@PLT
	mov	rsi, rax
	# return v9
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_after7_end:
	.cfi_endproc
	.size "k_after7", .-"k_after7"
	.section	".text","ax",@progbits
	.globl "k_fafter"
	.p2align	4
	.type	"k_fafter",@function
"k_fafter":
.Lk_fafter_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	lea	rsi, [rbp + 16]
.Lk_fafter_block0:
	# v9 = call @h_fafter(v0, v1, v2, v3, v4, v5, v6, v7, v8)
	mov	rdi, rsi
	call	"h_fafter"@PLT
	mov	rsi, rax
	# return v9
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_fafter_end:
	.cfi_endproc
	.size "k_fafter", .-"k_fafter"
	.section	".text","ax",@progbits
	.globl "k_after8"
	.p2align	4
	.type	"k_after8",@function
"k_after8":
.Lk_after8_begin:
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
	sub	rsp, 88
	movsd	qword ptr [rbp + -80], xmm0
	movsd	qword ptr [rbp + -72], xmm1
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
	mov	r13, qword ptr [rbp + 16]
	mov	r14, qword ptr [rbp + 24]
	lea	r15, [rbp + 32]
	lea	rax, [rbp + -80]
	mov	qword ptr [rbp + -64], rax
	lea	rax, [rbp + 40]
	mov	qword ptr [rbp + -56], rax
	mov	rax, qword ptr [rbp + 80]
	mov	qword ptr [rbp + -48], rax
.Lk_after8_block0:
	# v12 = call @h_after8(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11)
	mov	qword ptr [rsp], r13
	mov	qword ptr [rsp + 8], r14
	mov	qword ptr [rsp + 16], r15
	mov	rax, qword ptr [rbp + -64]
	mov	qword ptr [rsp + 24], rax
	mov	rax, qword ptr [rbp + -56]
	mov	qword ptr [rsp + 32], rax
	mov	rax, qword ptr [rbp + -48]
	mov	qword ptr [rsp + 40], rax
	mov	rdx, r8
	mov	rcx, r9
	mov	r8, rbx
	mov	r9, r12
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"h_after8"@PLT
	mov	rsi, rax
	# return v12
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_after8_end:
	.cfi_endproc
	.size "k_after8", .-"k_after8"
	.section	".text","ax",@progbits
	.globl "k_pass_after7"
	.p2align	4
	.type	"k_pass_after7",@function
"k_pass_after7":
.Lk_pass_after7_begin:
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
	mov	rsi, rdi
.Lk_pass_after7_block0:
	# v1 = iconst.i64 1
	mov	edi, 1
	# v2 = iconst.i64 2
	mov	r8d, 2
	# v3 = iconst.i64 3
	mov	r9d, 3
	# v4 = iconst.i64 4
	mov	ebx, 4
	# v5 = iconst.i64 5
	mov	r12d, 5
	# v6 = iconst.i64 6
	mov	r13d, 6
	# v7 = iconst.i64 7
	mov	r14d, 7
	# v8 = iconst.i64 8
	mov	r15d, 8
	# v9 = call @c_after7(v1, v2, v3, v4, v5, v6, v7, v0, v8)
	mov	qword ptr [rsp], r14
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rsp + 8], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rsp + 16], rax
	mov	qword ptr [rsp + 24], r15
	mov	rsi, r8
	mov	rdx, r9
	mov	rcx, rbx
	mov	r8, r12
	mov	r9, r13
	call	"c_after7"@PLT
	mov	rsi, rax
	# return v9
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_pass_after7_end:
	.cfi_endproc
	.size "k_pass_after7", .-"k_pass_after7"
	.section	".text","ax",@progbits
	.globl "k_pass_fafter"
	.p2align	4
	.type	"k_pass_fafter",@function
"k_pass_fafter":
.Lk_pass_fafter_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_pass_fafter_block0:
	# v1 = fconst.f64 1.0
	movabs	rax, 4607182418800017408
	movq	xmm0, rax
	# v2 = fconst.f64 2.0
	movabs	rax, 4611686018427387904
	movq	xmm1, rax
	# v3 = fconst.f64 3.0
	movabs	rax, 4613937818241073152
	movq	xmm2, rax
	# v4 = fconst.f64 4.0
	movabs	rax, 4616189618054758400
	movq	xmm3, rax
	# v5 = fconst.f64 5.0
	movabs	rax, 4617315517961601024
	movq	xmm4, rax
	# v6 = fconst.f64 6.0
	movabs	rax, 4618441417868443648
	movq	xmm5, rax
	# v7 = fconst.f64 7.0
	movabs	rax, 4619567317775286272
	movq	xmm6, rax
	# v8 = fconst.f32 0.5
	movabs	rax, 1056964608
	movq	xmm7, rax
	# v9 = call @c_fafter(v1, v2, v3, v4, v5, v6, v7, v0, v8)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rsp], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rsp + 8], rax
	call	"c_fafter"@PLT
	mov	rsi, rax
	# return v9
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_fafter_end:
	.cfi_endproc
	.size "k_pass_fafter", .-"k_pass_fafter"
	.section	".text","ax",@progbits
	.globl "k_pass_after8"
	.p2align	4
	.type	"k_pass_after8",@function
"k_pass_after8":
.Lk_pass_after8_begin:
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
	sub	rsp, 120
	mov	qword ptr [rbp + -80], rdi
	mov	qword ptr [rbp + -72], rsi
	mov	qword ptr [rbp + -64], rdx
.Lk_pass_after8_block0:
	# v3 = iconst.i64 1
	mov	r9d, 1
	# v4 = iconst.i64 2
	mov	ebx, 2
	# v5 = iconst.i64 3
	mov	r12d, 3
	# v6 = iconst.i64 4
	mov	r13d, 4
	# v7 = iconst.i64 5
	mov	r14d, 5
	# v8 = iconst.i64 6
	mov	r15d, 6
	# v9 = iconst.i64 7
	mov	r8d, 7
	# v10 = iconst.i64 8
	mov	edi, 8
	# v11 = iconst.i32 -9
	mov	esi, 4294967287
	# v12 = call @c_after8(v3, v4, v5, v6, v7, v8, v9, v10, v0, v1, v2, v11)
	mov	qword ptr [rsp], r8
	mov	qword ptr [rsp + 8], rdi
	mov	rcx, qword ptr [rbp + -80]
	mov	ax, word ptr [rcx]
	mov	word ptr [rsp + 16], ax
	mov	al, byte ptr [rcx + 2]
	mov	byte ptr [rsp + 18], al
	mov	rcx, qword ptr [rbp + -72]
	mov	rax, qword ptr [rcx]
	mov	qword ptr [rbp + -56], rax
	mov	eax, dword ptr [rcx + 8]
	mov	qword ptr [rbp + -48], rax
	mov	rcx, qword ptr [rbp + -64]
	mov	rax, qword ptr [rcx]
	mov	qword ptr [rsp + 24], rax
	mov	rax, qword ptr [rcx + 8]
	mov	qword ptr [rsp + 32], rax
	mov	rax, qword ptr [rcx + 16]
	mov	qword ptr [rsp + 40], rax
	mov	rax, qword ptr [rcx + 24]
	mov	qword ptr [rsp + 48], rax
	mov	rax, qword ptr [rcx + 32]
	mov	qword ptr [rsp + 56], rax
	mov	qword ptr [rsp + 64], rsi
	mov	rdi, r9
	mov	rsi, rbx
	mov	rdx, r12
	mov	rcx, r13
	mov	r8, r14
	mov	r9, r15
	movsd	xmm0, qword ptr [rbp + -56]
	movsd	xmm1, qword ptr [rbp + -48]
	call	"c_after8"@PLT
	mov	rsi, rax
	# return v12
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_pass_after8_end:
	.cfi_endproc
	.size "k_pass_after8", .-"k_pass_after8"
	.section	".text","ax",@progbits
	.globl "k_fstack"
	.p2align	4
	.type	"k_fstack",@function
"k_fstack":
.Lk_fstack_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	lea	rsi, [rbp + 16]
	movss	xmm8, dword ptr [rbp + 32]
	lea	rdi, [rbp + 40]
.Lk_fstack_block0:
	# v11 = call @h_fstack(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10)
	movsd	qword ptr [rsp], xmm8
	movq	xmm14, rdi
	mov	rdi, rsi
	movq	rsi, xmm14
	call	"h_fstack"@PLT
	mov	rsi, rax
	# return v11
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_fstack_end:
	.cfi_endproc
	.size "k_fstack", .-"k_fstack"
	.section	".text","ax",@progbits
	.globl "k_pass_fstack"
	.p2align	4
	.type	"k_pass_fstack",@function
"k_pass_fstack":
.Lk_pass_fstack_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 48
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_pass_fstack_block0:
	# v2 = fconst.f64 1.0
	movabs	rax, 4607182418800017408
	movq	xmm0, rax
	# v3 = fconst.f64 2.0
	movabs	rax, 4611686018427387904
	movq	xmm1, rax
	# v4 = fconst.f64 3.0
	movabs	rax, 4613937818241073152
	movq	xmm2, rax
	# v5 = fconst.f64 4.0
	movabs	rax, 4616189618054758400
	movq	xmm3, rax
	# v6 = fconst.f64 5.0
	movabs	rax, 4617315517961601024
	movq	xmm4, rax
	# v7 = fconst.f64 6.0
	movabs	rax, 4618441417868443648
	movq	xmm5, rax
	# v8 = fconst.f64 7.0
	movabs	rax, 4619567317775286272
	movq	xmm6, rax
	# v9 = fconst.f64 8.0
	movabs	rax, 4620693217682128896
	movq	xmm7, rax
	# v10 = fconst.f32 0.75
	movabs	rax, 1061158912
	movq	xmm8, rax
	# v11 = call @c_fstack(v2, v3, v4, v5, v6, v7, v8, v9, v0, v10, v1)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rsp], rax
	mov	eax, dword ptr [rsi + 8]
	mov	dword ptr [rsp + 8], eax
	movsd	qword ptr [rsp + 16], xmm8
	mov	rax, qword ptr [rdi]
	mov	qword ptr [rsp + 24], rax
	mov	eax, dword ptr [rdi + 8]
	mov	dword ptr [rsp + 32], eax
	call	"c_fstack"@PLT
	mov	rsi, rax
	# return v11
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_pass_fstack_end:
	.cfi_endproc
	.size "k_pass_fstack", .-"k_pass_fstack"
	.section	".text","ax",@progbits
	.globl "k_scribble_big"
	.p2align	4
	.type	"k_scribble_big",@function
"k_scribble_big":
.Lk_scribble_big_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	lea	rbx, [rbp + 16]
.Lk_scribble_big_block0:
	# v1 = iconst.i64 40
	mov	esi, 40
	# call @scribble(v0, v1)
	mov	rdi, rbx
	call	"scribble"@PLT
	# v2 = call @h_big(v0)
	mov	rdi, rbx
	call	"h_big"@PLT
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_scribble_big_end:
	.cfi_endproc
	.size "k_scribble_big", .-"k_scribble_big"
	.section	".text","ax",@progbits
	.globl "k_scribble_s16"
	.p2align	4
	.type	"k_scribble_s16",@function
"k_scribble_s16":
.Lk_scribble_s16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 24
	mov	qword ptr [rbp + -32], rdi
	mov	qword ptr [rbp + -24], rsi
	lea	rbx, [rbp + -32]
.Lk_scribble_s16_block0:
	# v1 = iconst.i64 16
	mov	esi, 16
	# call @scribble(v0, v1)
	mov	rdi, rbx
	call	"scribble"@PLT
	# v2 = call @h_s16(v0)
	mov	rdi, rbx
	call	"h_s16"@PLT
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_scribble_s16_end:
	.cfi_endproc
	.size "k_scribble_s16", .-"k_scribble_s16"
	.section	".text","ax",@progbits
	.globl "k_k_s16"
	.p2align	4
	.type	"k_k_s16",@function
"k_k_s16":
.Lk_k_s16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	rsi, rdi
.Lk_k_s16_block0:
	# v1 = call @k_s16(v0)
	mov	rax, qword ptr [rsi]
	mov	qword ptr [rbp + -16], rax
	mov	rax, qword ptr [rsi + 8]
	mov	qword ptr [rbp + -8], rax
	mov	rdi, qword ptr [rbp + -16]
	mov	rsi, qword ptr [rbp + -8]
	call	"k_s16"
	mov	rsi, rax
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_k_s16_end:
	.cfi_endproc
	.size "k_k_s16", .-"k_k_s16"
	.section	".text","ax",@progbits
	.globl "k_k_big"
	.p2align	4
	.type	"k_k_big",@function
"k_k_big":
.Lk_k_big_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 56
	mov	rbx, rdi
.Lk_k_big_block0:
	# v1 = call @k_scribble_big(v0)
	mov	rax, qword ptr [rbx]
	mov	qword ptr [rsp], rax
	mov	rax, qword ptr [rbx + 8]
	mov	qword ptr [rsp + 8], rax
	mov	rax, qword ptr [rbx + 16]
	mov	qword ptr [rsp + 16], rax
	mov	rax, qword ptr [rbx + 24]
	mov	qword ptr [rsp + 24], rax
	mov	rax, qword ptr [rbx + 32]
	mov	qword ptr [rsp + 32], rax
	call	"k_scribble_big"
	mov	rsi, rax
	# v2 = call @h_big(v0)
	mov	rdi, rbx
	call	"h_big"@PLT
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_k_big_end:
	.cfi_endproc
	.size "k_k_big", .-"k_k_big"
	.section	".text","ax",@progbits
	.globl "k_make_s16"
	.p2align	4
	.type	"k_make_s16",@function
"k_make_s16":
.Lk_make_s16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	lea	rsi, [rbp + -16]
.Lk_make_s16_block0:
	# call @fill_s16(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"fill_s16"@PLT
	# return
	mov	rax, qword ptr [rbp + -16]
	mov	rdx, qword ptr [rbp + -8]
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_make_s16_end:
	.cfi_endproc
	.size "k_make_s16", .-"k_make_s16"
	.section	".text","ax",@progbits
	.globl "k_make_s24"
	.p2align	4
	.type	"k_make_s24",@function
"k_make_s24":
.Lk_make_s24_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	qword ptr [rbp + -16], rdi
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_make_s24_block0:
	# call @fill_s24(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"fill_s24"@PLT
	# return
	mov	rax, qword ptr [rbp + -16]
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_make_s24_end:
	.cfi_endproc
	.size "k_make_s24", .-"k_make_s24"
	.section	".text","ax",@progbits
	.globl "k_make_d2"
	.p2align	4
	.type	"k_make_d2",@function
"k_make_d2":
.Lk_make_d2_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	lea	rsi, [rbp + -16]
.Lk_make_d2_block0:
	# call @fill_d2(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"fill_d2"@PLT
	# return
	movsd	xmm0, qword ptr [rbp + -16]
	movsd	xmm1, qword ptr [rbp + -8]
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_make_d2_end:
	.cfi_endproc
	.size "k_make_d2", .-"k_make_d2"
	.section	".text","ax",@progbits
	.globl "k_make_f3"
	.p2align	4
	.type	"k_make_f3",@function
"k_make_f3":
.Lk_make_f3_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	lea	rsi, [rbp + -16]
.Lk_make_f3_block0:
	# call @fill_f3(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"fill_f3"@PLT
	# return
	movsd	xmm0, qword ptr [rbp + -16]
	movsd	xmm1, qword ptr [rbp + -8]
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_make_f3_end:
	.cfi_endproc
	.size "k_make_f3", .-"k_make_f3"
	.section	".text","ax",@progbits
	.globl "k_make_c3"
	.p2align	4
	.type	"k_make_c3",@function
"k_make_c3":
.Lk_make_c3_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	lea	rsi, [rbp + -16]
.Lk_make_c3_block0:
	# call @fill_c3(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"fill_c3"@PLT
	# return
	mov	rax, qword ptr [rbp + -16]
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_make_c3_end:
	.cfi_endproc
	.size "k_make_c3", .-"k_make_c3"
	.section	".text","ax",@progbits
	.globl "k_make_big"
	.p2align	4
	.type	"k_make_big",@function
"k_make_big":
.Lk_make_big_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	qword ptr [rbp + -16], rdi
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_make_big_block0:
	# call @fill_big(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"fill_big"@PLT
	# return
	mov	rax, qword ptr [rbp + -16]
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_make_big_end:
	.cfi_endproc
	.size "k_make_big", .-"k_make_big"
	.section	".text","ax",@progbits
	.globl "k_get_s16"
	.p2align	4
	.type	"k_get_s16",@function
"k_get_s16":
.Lk_get_s16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_get_s16_block0:
	# call @c_make_s16(v0, v1)
	mov	qword ptr [rbp + -16], rsi
	call	"c_make_s16"@PLT
	mov	r11, qword ptr [rbp + -16]
	mov	qword ptr [r11], rax
	mov	qword ptr [r11 + 8], rdx
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_get_s16_end:
	.cfi_endproc
	.size "k_get_s16", .-"k_get_s16"
	.section	".text","ax",@progbits
	.globl "k_get_s24"
	.p2align	4
	.type	"k_get_s24",@function
"k_get_s24":
.Lk_get_s24_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_get_s24_block0:
	# call @c_make_s24(v0, v1)
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"c_make_s24"@PLT
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_get_s24_end:
	.cfi_endproc
	.size "k_get_s24", .-"k_get_s24"
	.section	".text","ax",@progbits
	.globl "k_get_d2"
	.p2align	4
	.type	"k_get_d2",@function
"k_get_d2":
.Lk_get_d2_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_get_d2_block0:
	# call @c_make_d2(v0, v1)
	mov	qword ptr [rbp + -16], rsi
	call	"c_make_d2"@PLT
	mov	r11, qword ptr [rbp + -16]
	movsd	qword ptr [r11], xmm0
	movsd	qword ptr [r11 + 8], xmm1
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_get_d2_end:
	.cfi_endproc
	.size "k_get_d2", .-"k_get_d2"
	.section	".text","ax",@progbits
	.globl "k_get_f3"
	.p2align	4
	.type	"k_get_f3",@function
"k_get_f3":
.Lk_get_f3_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_get_f3_block0:
	# call @c_make_f3(v0, v1)
	mov	qword ptr [rbp + -16], rsi
	call	"c_make_f3"@PLT
	mov	r11, qword ptr [rbp + -16]
	movsd	qword ptr [r11], xmm0
	movss	dword ptr [r11 + 8], xmm1
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_get_f3_end:
	.cfi_endproc
	.size "k_get_f3", .-"k_get_f3"
	.section	".text","ax",@progbits
	.globl "k_get_c3"
	.p2align	4
	.type	"k_get_c3",@function
"k_get_c3":
.Lk_get_c3_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	sub	rsp, 16
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_get_c3_block0:
	# call @c_make_c3(v0, v1)
	mov	qword ptr [rbp + -16], rsi
	call	"c_make_c3"@PLT
	mov	r11, qword ptr [rbp + -16]
	mov	word ptr [r11], ax
	shr	rax, 16
	mov	byte ptr [r11 + 2], al
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_get_c3_end:
	.cfi_endproc
	.size "k_get_c3", .-"k_get_c3"
	.section	".text","ax",@progbits
	.globl "k_roundtrip"
	.p2align	4
	.type	"k_roundtrip",@function
"k_roundtrip":
.Lk_roundtrip_begin:
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
	sub	rsp, 136
	mov	rbx, rdi
.Lk_roundtrip_block0:
	# v1 = stack_addr ss0
	lea	r12, [rbp + -88]
	# call @k_make_s16(v1, v0)
	mov	rdi, rbx
	call	"k_make_s16"
	lea	r11, [rbp + -88]
	mov	qword ptr [r11], rax
	mov	qword ptr [r11 + 8], rdx
	# v2 = stack_addr ss1
	lea	r13, [rbp + -72]
	# call @k_make_big(v2, v0)
	mov	rdi, r13
	mov	rsi, rbx
	call	"k_make_big"
	# v3 = call @k_s16(v1)
	mov	rax, qword ptr [r12]
	mov	qword ptr [rbp + -104], rax
	mov	rax, qword ptr [r12 + 8]
	mov	qword ptr [rbp + -96], rax
	mov	rdi, qword ptr [rbp + -104]
	mov	rsi, qword ptr [rbp + -96]
	call	"k_s16"
	mov	rbx, rax
	# v4 = call @k_big(v2)
	mov	rax, qword ptr [r13]
	mov	qword ptr [rsp], rax
	mov	rax, qword ptr [r13 + 8]
	mov	qword ptr [rsp + 8], rax
	mov	rax, qword ptr [r13 + 16]
	mov	qword ptr [rsp + 16], rax
	mov	rax, qword ptr [r13 + 24]
	mov	qword ptr [rsp + 24], rax
	mov	rax, qword ptr [r13 + 32]
	mov	qword ptr [rsp + 32], rax
	call	"k_big"
	mov	rsi, rax
	# v5 = iadd v3, v4
	add	rsi, rbx
	# return v5
	mov	rax, rsi
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_roundtrip_end:
	.cfi_endproc
	.size "k_roundtrip", .-"k_roundtrip"
	.section	".note.GNU-stack","",@progbits
