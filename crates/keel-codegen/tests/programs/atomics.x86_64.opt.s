	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_add_i8"
	.p2align	4
	.type	"k_add_i8",@function
"k_add_i8":
.Lk_add_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_add_i8_block0:
	# v2 = atomic_rmw.add relaxed v0, v1
	mov	rax, rdi
	lock xadd	byte ptr [rsi], al
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_add_i8_end:
	.cfi_endproc
	.size "k_add_i8", .-"k_add_i8"
	.section	".text","ax",@progbits
	.globl "k_sub_i8"
	.p2align	4
	.type	"k_sub_i8",@function
"k_sub_i8":
.Lk_sub_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sub_i8_block0:
	# v2 = atomic_rmw.sub acquire v0, v1
	mov	rax, rdi
	neg	al
	lock xadd	byte ptr [rsi], al
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sub_i8_end:
	.cfi_endproc
	.size "k_sub_i8", .-"k_sub_i8"
	.section	".text","ax",@progbits
	.globl "k_and_i8"
	.p2align	4
	.type	"k_and_i8",@function
"k_and_i8":
.Lk_and_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_and_i8_block0:
	# v2 = atomic_rmw.and release v0, v1
	movzx	eax, byte ptr [rsi]
.Lk_and_i8_inst0_retry:
	mov	rcx, rax
	and	ecx, edi
	lock cmpxchg	byte ptr [rsi], cl
	jne	".Lk_and_i8_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_and_i8_end:
	.cfi_endproc
	.size "k_and_i8", .-"k_and_i8"
	.section	".text","ax",@progbits
	.globl "k_or_i8"
	.p2align	4
	.type	"k_or_i8",@function
"k_or_i8":
.Lk_or_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_or_i8_block0:
	# v2 = atomic_rmw.or acq_rel v0, v1
	movzx	eax, byte ptr [rsi]
.Lk_or_i8_inst0_retry:
	mov	rcx, rax
	or	ecx, edi
	lock cmpxchg	byte ptr [rsi], cl
	jne	".Lk_or_i8_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_or_i8_end:
	.cfi_endproc
	.size "k_or_i8", .-"k_or_i8"
	.section	".text","ax",@progbits
	.globl "k_xor_i8"
	.p2align	4
	.type	"k_xor_i8",@function
"k_xor_i8":
.Lk_xor_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xor_i8_block0:
	# v2 = atomic_rmw.xor seq_cst v0, v1
	movzx	eax, byte ptr [rsi]
.Lk_xor_i8_inst0_retry:
	mov	rcx, rax
	xor	ecx, edi
	lock cmpxchg	byte ptr [rsi], cl
	jne	".Lk_xor_i8_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xor_i8_end:
	.cfi_endproc
	.size "k_xor_i8", .-"k_xor_i8"
	.section	".text","ax",@progbits
	.globl "k_xchg_i8"
	.p2align	4
	.type	"k_xchg_i8",@function
"k_xchg_i8":
.Lk_xchg_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xchg_i8_block0:
	# v2 = atomic_rmw.xchg relaxed v0, v1
	mov	rax, rdi
	xchg	byte ptr [rsi], al
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xchg_i8_end:
	.cfi_endproc
	.size "k_xchg_i8", .-"k_xchg_i8"
	.section	".text","ax",@progbits
	.globl "k_umin_i8"
	.p2align	4
	.type	"k_umin_i8",@function
"k_umin_i8":
.Lk_umin_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umin_i8_block0:
	# v2 = atomic_rmw.umin acquire v0, v1
	movzx	eax, byte ptr [rsi]
.Lk_umin_i8_inst0_retry:
	mov	rcx, rax
	cmp	cl, dil
	cmova	ecx, edi
	lock cmpxchg	byte ptr [rsi], cl
	jne	".Lk_umin_i8_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umin_i8_end:
	.cfi_endproc
	.size "k_umin_i8", .-"k_umin_i8"
	.section	".text","ax",@progbits
	.globl "k_umax_i8"
	.p2align	4
	.type	"k_umax_i8",@function
"k_umax_i8":
.Lk_umax_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umax_i8_block0:
	# v2 = atomic_rmw.umax release v0, v1
	movzx	eax, byte ptr [rsi]
.Lk_umax_i8_inst0_retry:
	mov	rcx, rax
	cmp	cl, dil
	cmovb	ecx, edi
	lock cmpxchg	byte ptr [rsi], cl
	jne	".Lk_umax_i8_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umax_i8_end:
	.cfi_endproc
	.size "k_umax_i8", .-"k_umax_i8"
	.section	".text","ax",@progbits
	.globl "k_smin_i8"
	.p2align	4
	.type	"k_smin_i8",@function
"k_smin_i8":
.Lk_smin_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smin_i8_block0:
	# v2 = atomic_rmw.smin acq_rel v0, v1
	movzx	eax, byte ptr [rsi]
.Lk_smin_i8_inst0_retry:
	mov	rcx, rax
	cmp	cl, dil
	cmovg	ecx, edi
	lock cmpxchg	byte ptr [rsi], cl
	jne	".Lk_smin_i8_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movsx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smin_i8_end:
	.cfi_endproc
	.size "k_smin_i8", .-"k_smin_i8"
	.section	".text","ax",@progbits
	.globl "k_smax_i8"
	.p2align	4
	.type	"k_smax_i8",@function
"k_smax_i8":
.Lk_smax_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smax_i8_block0:
	# v2 = atomic_rmw.smax seq_cst v0, v1
	movzx	eax, byte ptr [rsi]
.Lk_smax_i8_inst0_retry:
	mov	rcx, rax
	cmp	cl, dil
	cmovl	ecx, edi
	lock cmpxchg	byte ptr [rsi], cl
	jne	".Lk_smax_i8_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movsx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smax_i8_end:
	.cfi_endproc
	.size "k_smax_i8", .-"k_smax_i8"
	.section	".text","ax",@progbits
	.globl "k_cas_i8"
	.p2align	4
	.type	"k_cas_i8",@function
"k_cas_i8":
.Lk_cas_i8_begin:
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
.Lk_cas_i8_block0:
	# v3 = atomic_cas relaxed v0, v1, v2
	mov	rax, rdi
	lock cmpxchg	byte ptr [rsi], r8b
	mov	rsi, rax
	# return v3
	mov	rax, rsi
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_cas_i8_end:
	.cfi_endproc
	.size "k_cas_i8", .-"k_cas_i8"
	.section	".text","ax",@progbits
	.globl "k_swap_i8"
	.p2align	4
	.type	"k_swap_i8",@function
"k_swap_i8":
.Lk_swap_i8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_swap_i8_block0:
	# v2 = atomic_load.i8 seq_cst v0
	movzx	r8d, byte ptr [rsi]
	# fence acquire
	# atomic_store seq_cst v1, v0
	mov	rax, rdi
	xchg	byte ptr [rsi], al
	# fence seq_cst
	mfence
	# return v2
	mov	rax, r8
	movzx	eax, al
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_swap_i8_end:
	.cfi_endproc
	.size "k_swap_i8", .-"k_swap_i8"
	.section	".text","ax",@progbits
	.globl "k_add_i16"
	.p2align	4
	.type	"k_add_i16",@function
"k_add_i16":
.Lk_add_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_add_i16_block0:
	# v2 = atomic_rmw.add release v0, v1
	mov	rax, rdi
	lock xadd	word ptr [rsi], ax
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_add_i16_end:
	.cfi_endproc
	.size "k_add_i16", .-"k_add_i16"
	.section	".text","ax",@progbits
	.globl "k_sub_i16"
	.p2align	4
	.type	"k_sub_i16",@function
"k_sub_i16":
.Lk_sub_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sub_i16_block0:
	# v2 = atomic_rmw.sub acq_rel v0, v1
	mov	rax, rdi
	neg	ax
	lock xadd	word ptr [rsi], ax
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sub_i16_end:
	.cfi_endproc
	.size "k_sub_i16", .-"k_sub_i16"
	.section	".text","ax",@progbits
	.globl "k_and_i16"
	.p2align	4
	.type	"k_and_i16",@function
"k_and_i16":
.Lk_and_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_and_i16_block0:
	# v2 = atomic_rmw.and seq_cst v0, v1
	movzx	eax, word ptr [rsi]
.Lk_and_i16_inst0_retry:
	mov	rcx, rax
	and	ecx, edi
	lock cmpxchg	word ptr [rsi], cx
	jne	".Lk_and_i16_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_and_i16_end:
	.cfi_endproc
	.size "k_and_i16", .-"k_and_i16"
	.section	".text","ax",@progbits
	.globl "k_or_i16"
	.p2align	4
	.type	"k_or_i16",@function
"k_or_i16":
.Lk_or_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_or_i16_block0:
	# v2 = atomic_rmw.or relaxed v0, v1
	movzx	eax, word ptr [rsi]
.Lk_or_i16_inst0_retry:
	mov	rcx, rax
	or	ecx, edi
	lock cmpxchg	word ptr [rsi], cx
	jne	".Lk_or_i16_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_or_i16_end:
	.cfi_endproc
	.size "k_or_i16", .-"k_or_i16"
	.section	".text","ax",@progbits
	.globl "k_xor_i16"
	.p2align	4
	.type	"k_xor_i16",@function
"k_xor_i16":
.Lk_xor_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xor_i16_block0:
	# v2 = atomic_rmw.xor acquire v0, v1
	movzx	eax, word ptr [rsi]
.Lk_xor_i16_inst0_retry:
	mov	rcx, rax
	xor	ecx, edi
	lock cmpxchg	word ptr [rsi], cx
	jne	".Lk_xor_i16_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xor_i16_end:
	.cfi_endproc
	.size "k_xor_i16", .-"k_xor_i16"
	.section	".text","ax",@progbits
	.globl "k_xchg_i16"
	.p2align	4
	.type	"k_xchg_i16",@function
"k_xchg_i16":
.Lk_xchg_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xchg_i16_block0:
	# v2 = atomic_rmw.xchg release v0, v1
	mov	rax, rdi
	xchg	word ptr [rsi], ax
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xchg_i16_end:
	.cfi_endproc
	.size "k_xchg_i16", .-"k_xchg_i16"
	.section	".text","ax",@progbits
	.globl "k_umin_i16"
	.p2align	4
	.type	"k_umin_i16",@function
"k_umin_i16":
.Lk_umin_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umin_i16_block0:
	# v2 = atomic_rmw.umin acq_rel v0, v1
	movzx	eax, word ptr [rsi]
.Lk_umin_i16_inst0_retry:
	mov	rcx, rax
	cmp	cx, di
	cmova	ecx, edi
	lock cmpxchg	word ptr [rsi], cx
	jne	".Lk_umin_i16_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umin_i16_end:
	.cfi_endproc
	.size "k_umin_i16", .-"k_umin_i16"
	.section	".text","ax",@progbits
	.globl "k_umax_i16"
	.p2align	4
	.type	"k_umax_i16",@function
"k_umax_i16":
.Lk_umax_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umax_i16_block0:
	# v2 = atomic_rmw.umax seq_cst v0, v1
	movzx	eax, word ptr [rsi]
.Lk_umax_i16_inst0_retry:
	mov	rcx, rax
	cmp	cx, di
	cmovb	ecx, edi
	lock cmpxchg	word ptr [rsi], cx
	jne	".Lk_umax_i16_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umax_i16_end:
	.cfi_endproc
	.size "k_umax_i16", .-"k_umax_i16"
	.section	".text","ax",@progbits
	.globl "k_smin_i16"
	.p2align	4
	.type	"k_smin_i16",@function
"k_smin_i16":
.Lk_smin_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smin_i16_block0:
	# v2 = atomic_rmw.smin relaxed v0, v1
	movzx	eax, word ptr [rsi]
.Lk_smin_i16_inst0_retry:
	mov	rcx, rax
	cmp	cx, di
	cmovg	ecx, edi
	lock cmpxchg	word ptr [rsi], cx
	jne	".Lk_smin_i16_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movsx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smin_i16_end:
	.cfi_endproc
	.size "k_smin_i16", .-"k_smin_i16"
	.section	".text","ax",@progbits
	.globl "k_smax_i16"
	.p2align	4
	.type	"k_smax_i16",@function
"k_smax_i16":
.Lk_smax_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smax_i16_block0:
	# v2 = atomic_rmw.smax acquire v0, v1
	movzx	eax, word ptr [rsi]
.Lk_smax_i16_inst0_retry:
	mov	rcx, rax
	cmp	cx, di
	cmovl	ecx, edi
	lock cmpxchg	word ptr [rsi], cx
	jne	".Lk_smax_i16_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	movsx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smax_i16_end:
	.cfi_endproc
	.size "k_smax_i16", .-"k_smax_i16"
	.section	".text","ax",@progbits
	.globl "k_cas_i16"
	.p2align	4
	.type	"k_cas_i16",@function
"k_cas_i16":
.Lk_cas_i16_begin:
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
.Lk_cas_i16_block0:
	# v3 = atomic_cas release v0, v1, v2
	mov	rax, rdi
	lock cmpxchg	word ptr [rsi], r8w
	mov	rsi, rax
	# return v3
	mov	rax, rsi
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_cas_i16_end:
	.cfi_endproc
	.size "k_cas_i16", .-"k_cas_i16"
	.section	".text","ax",@progbits
	.globl "k_swap_i16"
	.p2align	4
	.type	"k_swap_i16",@function
"k_swap_i16":
.Lk_swap_i16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_swap_i16_block0:
	# v2 = atomic_load.i16 seq_cst v0
	movzx	r8d, word ptr [rsi]
	# fence acquire
	# atomic_store seq_cst v1, v0
	mov	rax, rdi
	xchg	word ptr [rsi], ax
	# fence seq_cst
	mfence
	# return v2
	mov	rax, r8
	movzx	eax, ax
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_swap_i16_end:
	.cfi_endproc
	.size "k_swap_i16", .-"k_swap_i16"
	.section	".text","ax",@progbits
	.globl "k_add_i32"
	.p2align	4
	.type	"k_add_i32",@function
"k_add_i32":
.Lk_add_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_add_i32_block0:
	# v2 = atomic_rmw.add seq_cst v0, v1
	mov	rax, rdi
	lock xadd	dword ptr [rsi], eax
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_add_i32_end:
	.cfi_endproc
	.size "k_add_i32", .-"k_add_i32"
	.section	".text","ax",@progbits
	.globl "k_sub_i32"
	.p2align	4
	.type	"k_sub_i32",@function
"k_sub_i32":
.Lk_sub_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sub_i32_block0:
	# v2 = atomic_rmw.sub relaxed v0, v1
	mov	rax, rdi
	neg	eax
	lock xadd	dword ptr [rsi], eax
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sub_i32_end:
	.cfi_endproc
	.size "k_sub_i32", .-"k_sub_i32"
	.section	".text","ax",@progbits
	.globl "k_and_i32"
	.p2align	4
	.type	"k_and_i32",@function
"k_and_i32":
.Lk_and_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_and_i32_block0:
	# v2 = atomic_rmw.and acquire v0, v1
	mov	eax, dword ptr [rsi]
.Lk_and_i32_inst0_retry:
	mov	rcx, rax
	and	ecx, edi
	lock cmpxchg	dword ptr [rsi], ecx
	jne	".Lk_and_i32_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_and_i32_end:
	.cfi_endproc
	.size "k_and_i32", .-"k_and_i32"
	.section	".text","ax",@progbits
	.globl "k_or_i32"
	.p2align	4
	.type	"k_or_i32",@function
"k_or_i32":
.Lk_or_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_or_i32_block0:
	# v2 = atomic_rmw.or release v0, v1
	mov	eax, dword ptr [rsi]
.Lk_or_i32_inst0_retry:
	mov	rcx, rax
	or	ecx, edi
	lock cmpxchg	dword ptr [rsi], ecx
	jne	".Lk_or_i32_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_or_i32_end:
	.cfi_endproc
	.size "k_or_i32", .-"k_or_i32"
	.section	".text","ax",@progbits
	.globl "k_xor_i32"
	.p2align	4
	.type	"k_xor_i32",@function
"k_xor_i32":
.Lk_xor_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xor_i32_block0:
	# v2 = atomic_rmw.xor acq_rel v0, v1
	mov	eax, dword ptr [rsi]
.Lk_xor_i32_inst0_retry:
	mov	rcx, rax
	xor	ecx, edi
	lock cmpxchg	dword ptr [rsi], ecx
	jne	".Lk_xor_i32_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xor_i32_end:
	.cfi_endproc
	.size "k_xor_i32", .-"k_xor_i32"
	.section	".text","ax",@progbits
	.globl "k_xchg_i32"
	.p2align	4
	.type	"k_xchg_i32",@function
"k_xchg_i32":
.Lk_xchg_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xchg_i32_block0:
	# v2 = atomic_rmw.xchg seq_cst v0, v1
	mov	rax, rdi
	xchg	dword ptr [rsi], eax
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xchg_i32_end:
	.cfi_endproc
	.size "k_xchg_i32", .-"k_xchg_i32"
	.section	".text","ax",@progbits
	.globl "k_umin_i32"
	.p2align	4
	.type	"k_umin_i32",@function
"k_umin_i32":
.Lk_umin_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umin_i32_block0:
	# v2 = atomic_rmw.umin relaxed v0, v1
	mov	eax, dword ptr [rsi]
.Lk_umin_i32_inst0_retry:
	mov	rcx, rax
	cmp	ecx, edi
	cmova	ecx, edi
	lock cmpxchg	dword ptr [rsi], ecx
	jne	".Lk_umin_i32_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umin_i32_end:
	.cfi_endproc
	.size "k_umin_i32", .-"k_umin_i32"
	.section	".text","ax",@progbits
	.globl "k_umax_i32"
	.p2align	4
	.type	"k_umax_i32",@function
"k_umax_i32":
.Lk_umax_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umax_i32_block0:
	# v2 = atomic_rmw.umax acquire v0, v1
	mov	eax, dword ptr [rsi]
.Lk_umax_i32_inst0_retry:
	mov	rcx, rax
	cmp	ecx, edi
	cmovb	ecx, edi
	lock cmpxchg	dword ptr [rsi], ecx
	jne	".Lk_umax_i32_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umax_i32_end:
	.cfi_endproc
	.size "k_umax_i32", .-"k_umax_i32"
	.section	".text","ax",@progbits
	.globl "k_smin_i32"
	.p2align	4
	.type	"k_smin_i32",@function
"k_smin_i32":
.Lk_smin_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smin_i32_block0:
	# v2 = atomic_rmw.smin release v0, v1
	mov	eax, dword ptr [rsi]
.Lk_smin_i32_inst0_retry:
	mov	rcx, rax
	cmp	ecx, edi
	cmovg	ecx, edi
	lock cmpxchg	dword ptr [rsi], ecx
	jne	".Lk_smin_i32_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smin_i32_end:
	.cfi_endproc
	.size "k_smin_i32", .-"k_smin_i32"
	.section	".text","ax",@progbits
	.globl "k_smax_i32"
	.p2align	4
	.type	"k_smax_i32",@function
"k_smax_i32":
.Lk_smax_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smax_i32_block0:
	# v2 = atomic_rmw.smax acq_rel v0, v1
	mov	eax, dword ptr [rsi]
.Lk_smax_i32_inst0_retry:
	mov	rcx, rax
	cmp	ecx, edi
	cmovl	ecx, edi
	lock cmpxchg	dword ptr [rsi], ecx
	jne	".Lk_smax_i32_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smax_i32_end:
	.cfi_endproc
	.size "k_smax_i32", .-"k_smax_i32"
	.section	".text","ax",@progbits
	.globl "k_cas_i32"
	.p2align	4
	.type	"k_cas_i32",@function
"k_cas_i32":
.Lk_cas_i32_begin:
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
.Lk_cas_i32_block0:
	# v3 = atomic_cas seq_cst v0, v1, v2
	mov	rax, rdi
	lock cmpxchg	dword ptr [rsi], r8d
	mov	rsi, rax
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_cas_i32_end:
	.cfi_endproc
	.size "k_cas_i32", .-"k_cas_i32"
	.section	".text","ax",@progbits
	.globl "k_swap_i32"
	.p2align	4
	.type	"k_swap_i32",@function
"k_swap_i32":
.Lk_swap_i32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_swap_i32_block0:
	# v2 = atomic_load.i32 seq_cst v0
	mov	r8d, dword ptr [rsi]
	# fence acquire
	# atomic_store seq_cst v1, v0
	mov	rax, rdi
	xchg	dword ptr [rsi], eax
	# fence seq_cst
	mfence
	# return v2
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_swap_i32_end:
	.cfi_endproc
	.size "k_swap_i32", .-"k_swap_i32"
	.section	".text","ax",@progbits
	.globl "k_add_i64"
	.p2align	4
	.type	"k_add_i64",@function
"k_add_i64":
.Lk_add_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_add_i64_block0:
	# v2 = atomic_rmw.add acquire v0, v1
	mov	rax, rdi
	lock xadd	qword ptr [rsi], rax
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_add_i64_end:
	.cfi_endproc
	.size "k_add_i64", .-"k_add_i64"
	.section	".text","ax",@progbits
	.globl "k_sub_i64"
	.p2align	4
	.type	"k_sub_i64",@function
"k_sub_i64":
.Lk_sub_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sub_i64_block0:
	# v2 = atomic_rmw.sub release v0, v1
	mov	rax, rdi
	neg	rax
	lock xadd	qword ptr [rsi], rax
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sub_i64_end:
	.cfi_endproc
	.size "k_sub_i64", .-"k_sub_i64"
	.section	".text","ax",@progbits
	.globl "k_and_i64"
	.p2align	4
	.type	"k_and_i64",@function
"k_and_i64":
.Lk_and_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_and_i64_block0:
	# v2 = atomic_rmw.and acq_rel v0, v1
	mov	rax, qword ptr [rsi]
.Lk_and_i64_inst0_retry:
	mov	rcx, rax
	and	rcx, rdi
	lock cmpxchg	qword ptr [rsi], rcx
	jne	".Lk_and_i64_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_and_i64_end:
	.cfi_endproc
	.size "k_and_i64", .-"k_and_i64"
	.section	".text","ax",@progbits
	.globl "k_or_i64"
	.p2align	4
	.type	"k_or_i64",@function
"k_or_i64":
.Lk_or_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_or_i64_block0:
	# v2 = atomic_rmw.or seq_cst v0, v1
	mov	rax, qword ptr [rsi]
.Lk_or_i64_inst0_retry:
	mov	rcx, rax
	or	rcx, rdi
	lock cmpxchg	qword ptr [rsi], rcx
	jne	".Lk_or_i64_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_or_i64_end:
	.cfi_endproc
	.size "k_or_i64", .-"k_or_i64"
	.section	".text","ax",@progbits
	.globl "k_xor_i64"
	.p2align	4
	.type	"k_xor_i64",@function
"k_xor_i64":
.Lk_xor_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xor_i64_block0:
	# v2 = atomic_rmw.xor relaxed v0, v1
	mov	rax, qword ptr [rsi]
.Lk_xor_i64_inst0_retry:
	mov	rcx, rax
	xor	rcx, rdi
	lock cmpxchg	qword ptr [rsi], rcx
	jne	".Lk_xor_i64_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xor_i64_end:
	.cfi_endproc
	.size "k_xor_i64", .-"k_xor_i64"
	.section	".text","ax",@progbits
	.globl "k_xchg_i64"
	.p2align	4
	.type	"k_xchg_i64",@function
"k_xchg_i64":
.Lk_xchg_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xchg_i64_block0:
	# v2 = atomic_rmw.xchg acquire v0, v1
	mov	rax, rdi
	xchg	qword ptr [rsi], rax
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xchg_i64_end:
	.cfi_endproc
	.size "k_xchg_i64", .-"k_xchg_i64"
	.section	".text","ax",@progbits
	.globl "k_umin_i64"
	.p2align	4
	.type	"k_umin_i64",@function
"k_umin_i64":
.Lk_umin_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umin_i64_block0:
	# v2 = atomic_rmw.umin release v0, v1
	mov	rax, qword ptr [rsi]
.Lk_umin_i64_inst0_retry:
	mov	rcx, rax
	cmp	rcx, rdi
	cmova	rcx, rdi
	lock cmpxchg	qword ptr [rsi], rcx
	jne	".Lk_umin_i64_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umin_i64_end:
	.cfi_endproc
	.size "k_umin_i64", .-"k_umin_i64"
	.section	".text","ax",@progbits
	.globl "k_umax_i64"
	.p2align	4
	.type	"k_umax_i64",@function
"k_umax_i64":
.Lk_umax_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_umax_i64_block0:
	# v2 = atomic_rmw.umax acq_rel v0, v1
	mov	rax, qword ptr [rsi]
.Lk_umax_i64_inst0_retry:
	mov	rcx, rax
	cmp	rcx, rdi
	cmovb	rcx, rdi
	lock cmpxchg	qword ptr [rsi], rcx
	jne	".Lk_umax_i64_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_umax_i64_end:
	.cfi_endproc
	.size "k_umax_i64", .-"k_umax_i64"
	.section	".text","ax",@progbits
	.globl "k_smin_i64"
	.p2align	4
	.type	"k_smin_i64",@function
"k_smin_i64":
.Lk_smin_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smin_i64_block0:
	# v2 = atomic_rmw.smin seq_cst v0, v1
	mov	rax, qword ptr [rsi]
.Lk_smin_i64_inst0_retry:
	mov	rcx, rax
	cmp	rcx, rdi
	cmovg	rcx, rdi
	lock cmpxchg	qword ptr [rsi], rcx
	jne	".Lk_smin_i64_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smin_i64_end:
	.cfi_endproc
	.size "k_smin_i64", .-"k_smin_i64"
	.section	".text","ax",@progbits
	.globl "k_smax_i64"
	.p2align	4
	.type	"k_smax_i64",@function
"k_smax_i64":
.Lk_smax_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_smax_i64_block0:
	# v2 = atomic_rmw.smax relaxed v0, v1
	mov	rax, qword ptr [rsi]
.Lk_smax_i64_inst0_retry:
	mov	rcx, rax
	cmp	rcx, rdi
	cmovl	rcx, rdi
	lock cmpxchg	qword ptr [rsi], rcx
	jne	".Lk_smax_i64_inst0_retry"
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_smax_i64_end:
	.cfi_endproc
	.size "k_smax_i64", .-"k_smax_i64"
	.section	".text","ax",@progbits
	.globl "k_cas_i64"
	.p2align	4
	.type	"k_cas_i64",@function
"k_cas_i64":
.Lk_cas_i64_begin:
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
.Lk_cas_i64_block0:
	# v3 = atomic_cas acquire v0, v1, v2
	mov	rax, rdi
	lock cmpxchg	qword ptr [rsi], r8
	mov	rsi, rax
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_cas_i64_end:
	.cfi_endproc
	.size "k_cas_i64", .-"k_cas_i64"
	.section	".text","ax",@progbits
	.globl "k_swap_i64"
	.p2align	4
	.type	"k_swap_i64",@function
"k_swap_i64":
.Lk_swap_i64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_swap_i64_block0:
	# v2 = atomic_load.i64 seq_cst v0
	mov	r8, qword ptr [rsi]
	# fence acquire
	# atomic_store seq_cst v1, v0
	mov	rax, rdi
	xchg	qword ptr [rsi], rax
	# fence seq_cst
	mfence
	# return v2
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_swap_i64_end:
	.cfi_endproc
	.size "k_swap_i64", .-"k_swap_i64"
	.section	".text","ax",@progbits
	.globl "k_xchg_ptr"
	.p2align	4
	.type	"k_xchg_ptr",@function
"k_xchg_ptr":
.Lk_xchg_ptr_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xchg_ptr_block0:
	# v2 = atomic_rmw.xchg seq_cst v0, v1
	mov	rax, rdi
	xchg	qword ptr [rsi], rax
	mov	rdi, rax
	# v3 = atomic_load.ptr acquire v0
	mov	r8, qword ptr [rsi]
	# v4 = atomic_cas acq_rel v0, v3, v2
	mov	rax, r8
	lock cmpxchg	qword ptr [rsi], rdi
	mov	rsi, rax
	# return v4
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xchg_ptr_end:
	.cfi_endproc
	.size "k_xchg_ptr", .-"k_xchg_ptr"
	.section	".text","ax",@progbits
	.globl "k_count"
	.p2align	4
	.type	"k_count",@function
"k_count":
.Lk_count_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_count_block0:
	# v2 = iconst.i64 0
	xor	r8d, r8d
	# v4 = icmp.ugt v1, v2
	# brif v4, block2(v2), block3
	cmp	rdi, 0
	jbe	".Lk_count_block3"
.Lk_count_block2:
	# v5 = iconst.i64 1
	mov	r9d, 1
	# v6 = atomic_rmw.add relaxed v0, v5
	mov	rax, r9
	lock xadd	qword ptr [rsi], rax
	mov	r9, rax
	# v7 = iadd v8, v5
	add	r8, 1
	# v10 = icmp.ugt v1, v7
	# brif v10, block2(v7), block3
	cmp	rdi, r8
	ja	".Lk_count_block2"
.Lk_count_block3:
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_count_end:
	.cfi_endproc
	.size "k_count", .-"k_count"
	.section	".text","ax",@progbits
	.globl "k_lock"
	.p2align	4
	.type	"k_lock",@function
"k_lock":
.Lk_lock_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_lock_block0:
	# jump block1
.Lk_lock_block1:
	# v2 = iconst.i32 1
	mov	edi, 1
	# v1 = iconst.i32 0
	xor	r8d, r8d
	# v3 = atomic_cas acquire v0, v1, v2
	mov	rax, r8
	lock cmpxchg	dword ptr [rsi], edi
	mov	rdi, rax
	# v4 = icmp.eq v3, v1
	# brif v4, block2, block1
	cmp	edi, 0
	jne	".Lk_lock_block1"
.Lk_lock_block2:
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_lock_end:
	.cfi_endproc
	.size "k_lock", .-"k_lock"
	.section	".text","ax",@progbits
	.globl "k_unlock"
	.p2align	4
	.type	"k_unlock",@function
"k_unlock":
.Lk_unlock_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lk_unlock_block0:
	# v1 = iconst.i32 0
	xor	edi, edi
	# atomic_store release v1, v0
	mov	dword ptr [rsi], edi
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_unlock_end:
	.cfi_endproc
	.size "k_unlock", .-"k_unlock"
	.section	".note.GNU-stack","",@progbits
