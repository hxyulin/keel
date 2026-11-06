	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_load_relaxed"
	.p2align	4
	.type	"k_load_relaxed",@function
"k_load_relaxed":
.Lk_load_relaxed_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rsi, rdi
.Lk_load_relaxed_block0:
	# v1 = atomic_load.i128 relaxed v0
	vmovdqa	xmm13, xmmword ptr [rsi]
	vmovq	rax, xmm13
	vpextrq	rdx, xmm13, 1
	mov	rsi, rax
	mov	rdi, rdx
	# return v1
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_load_relaxed_end:
	.cfi_endproc
	.size "k_load_relaxed", .-"k_load_relaxed"
	.section	".text","ax",@progbits
	.globl "k_load_acquire"
	.p2align	4
	.type	"k_load_acquire",@function
"k_load_acquire":
.Lk_load_acquire_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rsi, rdi
.Lk_load_acquire_block0:
	# v1 = atomic_load.i128 acquire v0
	vmovdqa	xmm13, xmmword ptr [rsi]
	vmovq	rax, xmm13
	vpextrq	rdx, xmm13, 1
	mov	rsi, rax
	mov	rdi, rdx
	# return v1
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_load_acquire_end:
	.cfi_endproc
	.size "k_load_acquire", .-"k_load_acquire"
	.section	".text","ax",@progbits
	.globl "k_load_seq_cst"
	.p2align	4
	.type	"k_load_seq_cst",@function
"k_load_seq_cst":
.Lk_load_seq_cst_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rsi, rdi
.Lk_load_seq_cst_block0:
	# v1 = atomic_load.i128 seq_cst v0
	vmovdqa	xmm13, xmmword ptr [rsi]
	vmovq	rax, xmm13
	vpextrq	rdx, xmm13, 1
	mov	rsi, rax
	mov	rdi, rdx
	# return v1
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_load_seq_cst_end:
	.cfi_endproc
	.size "k_load_seq_cst", .-"k_load_seq_cst"
	.section	".text","ax",@progbits
	.globl "k_store_relaxed"
	.p2align	4
	.type	"k_store_relaxed",@function
"k_store_relaxed":
.Lk_store_relaxed_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_store_relaxed_block0:
	# atomic_store relaxed v1, v0
	vmovq	xmm13, rdi
	vpinsrq	xmm13, xmm13, r8, 1
	vmovdqa	xmmword ptr [rsi], xmm13
	# return
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_store_relaxed_end:
	.cfi_endproc
	.size "k_store_relaxed", .-"k_store_relaxed"
	.section	".text","ax",@progbits
	.globl "k_store_release"
	.p2align	4
	.type	"k_store_release",@function
"k_store_release":
.Lk_store_release_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_store_release_block0:
	# atomic_store release v1, v0
	vmovq	xmm13, rdi
	vpinsrq	xmm13, xmm13, r8, 1
	vmovdqa	xmmword ptr [rsi], xmm13
	# return
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_store_release_end:
	.cfi_endproc
	.size "k_store_release", .-"k_store_release"
	.section	".text","ax",@progbits
	.globl "k_store_seq_cst"
	.p2align	4
	.type	"k_store_seq_cst",@function
"k_store_seq_cst":
.Lk_store_seq_cst_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_store_seq_cst_block0:
	# atomic_store seq_cst v1, v0
	vmovq	xmm13, rdi
	vpinsrq	xmm13, xmm13, r8, 1
	vmovdqa	xmmword ptr [rsi], xmm13
	mfence
	# return
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_store_seq_cst_end:
	.cfi_endproc
	.size "k_store_seq_cst", .-"k_store_seq_cst"
	.section	".text","ax",@progbits
	.globl "k_cas_strong"
	.p2align	4
	.type	"k_cas_strong",@function
"k_cas_strong":
.Lk_cas_strong_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_cas_strong_block0:
	# v4, v5 = atomic_cas acq_rel acquire v0, v1, v2
	mov	r11, rsi
	mov	qword ptr [rbp + -32], rbx
	mov	rax, rdi
	mov	rdx, r8
	mov	rcx, rbx
	mov	rbx, r9
	lock cmpxchg16b	xmmword ptr [r11]
	sete	cl
	mov	rbx, qword ptr [rbp + -32]
	mov	rsi, rax
	mov	rdi, rdx
	mov	r8, rcx
	# store v4, v3
	mov	rax, rsi
	mov	qword ptr [r12], rax
	mov	rax, rdi
	mov	qword ptr [r12 + 8], rax
	# return v5
	mov	rax, r8
	movzx	eax, al
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_cas_strong_end:
	.cfi_endproc
	.size "k_cas_strong", .-"k_cas_strong"
	.section	".text","ax",@progbits
	.globl "k_cas_weak"
	.p2align	4
	.type	"k_cas_weak",@function
"k_cas_weak":
.Lk_cas_weak_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_cas_weak_block0:
	# v4, v5 = atomic_cas weak relaxed seq_cst v0, v1, v2
	mov	r11, rsi
	mov	qword ptr [rbp + -32], rbx
	mov	rax, rdi
	mov	rdx, r8
	mov	rcx, rbx
	mov	rbx, r9
	lock cmpxchg16b	xmmword ptr [r11]
	sete	cl
	mov	rbx, qword ptr [rbp + -32]
	mov	rsi, rax
	mov	rdi, rdx
	mov	r8, rcx
	# store v4, v3
	mov	rax, rsi
	mov	qword ptr [r12], rax
	mov	rax, rdi
	mov	qword ptr [r12 + 8], rax
	# return v5
	mov	rax, r8
	movzx	eax, al
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_cas_weak_end:
	.cfi_endproc
	.size "k_cas_weak", .-"k_cas_weak"
	.section	".text","ax",@progbits
	.globl "k_cas_release"
	.p2align	4
	.type	"k_cas_release",@function
"k_cas_release":
.Lk_cas_release_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_cas_release_block0:
	# v4, v5 = atomic_cas release relaxed v0, v1, v2
	mov	r11, rsi
	mov	qword ptr [rbp + -32], rbx
	mov	rax, rdi
	mov	rdx, r8
	mov	rcx, rbx
	mov	rbx, r9
	lock cmpxchg16b	xmmword ptr [r11]
	sete	cl
	mov	rbx, qword ptr [rbp + -32]
	mov	rsi, rax
	mov	rdi, rdx
	mov	r8, rcx
	# store v4, v3
	mov	rax, rsi
	mov	qword ptr [r12], rax
	mov	rax, rdi
	mov	qword ptr [r12 + 8], rax
	# return v5
	mov	rax, r8
	movzx	eax, al
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_cas_release_end:
	.cfi_endproc
	.size "k_cas_release", .-"k_cas_release"
	.section	".text","ax",@progbits
	.globl "k_cas_acquire"
	.p2align	4
	.type	"k_cas_acquire",@function
"k_cas_acquire":
.Lk_cas_acquire_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_cas_acquire_block0:
	# v4, v5 = atomic_cas acquire acquire v0, v1, v2
	mov	r11, rsi
	mov	qword ptr [rbp + -32], rbx
	mov	rax, rdi
	mov	rdx, r8
	mov	rcx, rbx
	mov	rbx, r9
	lock cmpxchg16b	xmmword ptr [r11]
	sete	cl
	mov	rbx, qword ptr [rbp + -32]
	mov	rsi, rax
	mov	rdi, rdx
	mov	r8, rcx
	# store v4, v3
	mov	rax, rsi
	mov	qword ptr [r12], rax
	mov	rax, rdi
	mov	qword ptr [r12 + 8], rax
	# return v5
	mov	rax, r8
	movzx	eax, al
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_cas_acquire_end:
	.cfi_endproc
	.size "k_cas_acquire", .-"k_cas_acquire"
	.section	".text","ax",@progbits
	.globl "k_add"
	.p2align	4
	.type	"k_add",@function
"k_add":
.Lk_add_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_add_block0:
	# v2 = atomic_rmw.add relaxed v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
.Lk_add_inst0_retry:
	mov	rbx, rax
	mov	rcx, rdx
	add	rbx, rdi
	adc	rcx, r8
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_add_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_add_end:
	.cfi_endproc
	.size "k_add", .-"k_add"
	.section	".text","ax",@progbits
	.globl "k_sub"
	.p2align	4
	.type	"k_sub",@function
"k_sub":
.Lk_sub_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sub_block0:
	# v2 = atomic_rmw.sub acquire v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
.Lk_sub_inst0_retry:
	mov	rbx, rax
	mov	rcx, rdx
	sub	rbx, rdi
	sbb	rcx, r8
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_sub_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_sub_end:
	.cfi_endproc
	.size "k_sub", .-"k_sub"
	.section	".text","ax",@progbits
	.globl "k_and"
	.p2align	4
	.type	"k_and",@function
"k_and":
.Lk_and_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_and_block0:
	# v2 = atomic_rmw.and release v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
.Lk_and_inst0_retry:
	mov	rbx, rax
	mov	rcx, rdx
	and	rbx, rdi
	and	rcx, r8
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_and_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_and_end:
	.cfi_endproc
	.size "k_and", .-"k_and"
	.section	".text","ax",@progbits
	.globl "k_or"
	.p2align	4
	.type	"k_or",@function
"k_or":
.Lk_or_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_or_block0:
	# v2 = atomic_rmw.or acq_rel v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
.Lk_or_inst0_retry:
	mov	rbx, rax
	mov	rcx, rdx
	or	rbx, rdi
	or	rcx, r8
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_or_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_or_end:
	.cfi_endproc
	.size "k_or", .-"k_or"
	.section	".text","ax",@progbits
	.globl "k_xor"
	.p2align	4
	.type	"k_xor",@function
"k_xor":
.Lk_xor_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xor_block0:
	# v2 = atomic_rmw.xor seq_cst v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
.Lk_xor_inst0_retry:
	mov	rbx, rax
	mov	rcx, rdx
	xor	rbx, rdi
	xor	rcx, r8
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_xor_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_xor_end:
	.cfi_endproc
	.size "k_xor", .-"k_xor"
	.section	".text","ax",@progbits
	.globl "k_xchg"
	.p2align	4
	.type	"k_xchg",@function
"k_xchg":
.Lk_xchg_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xchg_block0:
	# v2 = atomic_rmw.xchg relaxed v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
	mov	rcx, r8
	mov	rbx, rdi
.Lk_xchg_inst0_retry:
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_xchg_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_xchg_end:
	.cfi_endproc
	.size "k_xchg", .-"k_xchg"
	.section	".text","ax",@progbits
	.globl "k_umin"
	.p2align	4
	.type	"k_umin",@function
"k_umin":
.Lk_umin_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umin_block0:
	# v2 = atomic_rmw.umin acquire v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
.Lk_umin_inst0_retry:
	cmp	rax, rdi
	mov	rcx, rdx
	sbb	rcx, r8
	mov	rbx, rdi
	mov	rcx, r8
	cmovb	rbx, rax
	cmovb	rcx, rdx
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_umin_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_umin_end:
	.cfi_endproc
	.size "k_umin", .-"k_umin"
	.section	".text","ax",@progbits
	.globl "k_umax"
	.p2align	4
	.type	"k_umax",@function
"k_umax":
.Lk_umax_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umax_block0:
	# v2 = atomic_rmw.umax release v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
.Lk_umax_inst0_retry:
	cmp	rax, rdi
	mov	rcx, rdx
	sbb	rcx, r8
	mov	rbx, rdi
	mov	rcx, r8
	cmovae	rbx, rax
	cmovae	rcx, rdx
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_umax_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_umax_end:
	.cfi_endproc
	.size "k_umax", .-"k_umax"
	.section	".text","ax",@progbits
	.globl "k_smin"
	.p2align	4
	.type	"k_smin",@function
"k_smin":
.Lk_smin_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smin_block0:
	# v2 = atomic_rmw.smin acq_rel v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
.Lk_smin_inst0_retry:
	cmp	rax, rdi
	mov	rcx, rdx
	sbb	rcx, r8
	mov	rbx, rdi
	mov	rcx, r8
	cmovl	rbx, rax
	cmovl	rcx, rdx
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_smin_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_smin_end:
	.cfi_endproc
	.size "k_smin", .-"k_smin"
	.section	".text","ax",@progbits
	.globl "k_smax"
	.p2align	4
	.type	"k_smax",@function
"k_smax":
.Lk_smax_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smax_block0:
	# v2 = atomic_rmw.smax seq_cst v0, v1
	mov	r11, rsi
	mov	qword ptr [rbp + -16], rbx
	mov	rax, qword ptr [r11]
	mov	rdx, qword ptr [r11 + 8]
.Lk_smax_inst0_retry:
	cmp	rax, rdi
	mov	rcx, rdx
	sbb	rcx, r8
	mov	rbx, rdi
	mov	rcx, r8
	cmovge	rbx, rax
	cmovge	rcx, rdx
	lock cmpxchg16b	xmmword ptr [r11]
	jne	".Lk_smax_inst0_retry"
	mov	rbx, qword ptr [rbp + -16]
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_smax_end:
	.cfi_endproc
	.size "k_smax", .-"k_smax"
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
	push	r13
	.cfi_offset 13, -40
	push	r14
	.cfi_offset 14, -48
	push	r15
	.cfi_offset 15, -56
	sub	rsp, 8
	mov	rsi, rdi
.Lk_bump_block0:
	# v1 = atomic_load.i128 relaxed v0
	vmovdqa	xmm13, xmmword ptr [rsi]
	vmovq	rax, xmm13
	vpextrq	rdx, xmm13, 1
	mov	rdi, rax
	mov	r8, rdx
	# jump block1(v1)
.Lk_bump_block1:
	# v3 = iconst.i128 1
	mov	eax, 1
	xor	edx, edx
	mov	r9, rax
	mov	rbx, rdx
	# v4 = iadd v2, v3
	mov	rax, rdi
	add	rax, r9
	mov	rdx, r8
	adc	rdx, rbx
	mov	r9, rax
	mov	rbx, rdx
	# v5 = iconst.i128 64
	mov	eax, 64
	xor	edx, edx
	mov	r12, rax
	mov	r13, rdx
	# v6 = shl v4, v5
	mov	rdx, r9
	xor	eax, eax
	mov	r9, rax
	mov	rbx, rdx
	# v7 = ushr v6, v5
	mov	rax, rbx
	xor	edx, edx
	mov	r9, rax
	mov	rbx, rdx
	# v8 = bnot v7
	mov	rax, r9
	mov	rdx, rbx
	not	rax
	not	rdx
	mov	r14, rax
	mov	r15, rdx
	# v9 = shl v8, v5
	mov	rdx, r14
	xor	eax, eax
	mov	r14, rax
	mov	r15, rdx
	# v10 = or v7, v9
	mov	rax, r9
	or	rax, r14
	mov	rdx, rbx
	or	rdx, r15
	mov	r9, rax
	mov	rbx, rdx
	# v11, v12 = atomic_cas weak acq_rel relaxed v0, v2, v10
	mov	r11, rsi
	mov	qword ptr [rbp + -48], rbx
	mov	rax, rdi
	mov	rdx, r8
	mov	rcx, rbx
	mov	rbx, r9
	lock cmpxchg16b	xmmword ptr [r11]
	mov	rbx, qword ptr [rbp + -48]
	mov	rdi, rax
	mov	r8, rdx
	# brif v12, block2, block1(v11)
	jne	".Lk_bump_block1"
.Lk_bump_block2:
	# return
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_bump_end:
	.cfi_endproc
	.size "k_bump", .-"k_bump"
	.section	".note.GNU-stack","",@progbits
