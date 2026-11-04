	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_iadd"
	.p2align	4
	.type	"k_iadd",@function
"k_iadd":
.Lk_iadd_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_iadd_block0:
	# v2 = iadd v0, v1
	mov	rax, rsi
	add	rax, r8
	mov	rdx, rdi
	adc	rdx, r9
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_iadd_end:
	.cfi_endproc
	.size "k_iadd", .-"k_iadd"
	.section	".text","ax",@progbits
	.globl "k_isub"
	.p2align	4
	.type	"k_isub",@function
"k_isub":
.Lk_isub_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_isub_block0:
	# v2 = isub v0, v1
	mov	rax, rsi
	sub	rax, r8
	mov	rdx, rdi
	sbb	rdx, r9
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_isub_end:
	.cfi_endproc
	.size "k_isub", .-"k_isub"
	.section	".text","ax",@progbits
	.globl "k_imul"
	.p2align	4
	.type	"k_imul",@function
"k_imul":
.Lk_imul_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_imul_block0:
	# v2 = imul v0, v1
	mov	rax, rsi
	mul	r8
	mov	rcx, rsi
	imul	rcx, r9
	add	rdx, rcx
	mov	rcx, rdi
	imul	rcx, r8
	add	rdx, rcx
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_imul_end:
	.cfi_endproc
	.size "k_imul", .-"k_imul"
	.section	".text","ax",@progbits
	.globl "k_udiv"
	.p2align	4
	.type	"k_udiv",@function
"k_udiv":
.Lk_udiv_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_udiv_block0:
	# v2 = udiv v0, v1
	mov	rax, r8
	or	rax, r9
	je	".Lk_udiv_trap"
	mov	rdx, r8
	mov	rcx, r9
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"__udivti3"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_udiv_trap:
	ud2
.Lk_udiv_end:
	.cfi_endproc
	.size "k_udiv", .-"k_udiv"
	.section	".text","ax",@progbits
	.globl "k_sdiv"
	.p2align	4
	.type	"k_sdiv",@function
"k_sdiv":
.Lk_sdiv_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sdiv_block0:
	# v2 = sdiv v0, v1
	mov	rax, r8
	or	rax, r9
	je	".Lk_sdiv_trap"
	mov	rax, r8
	and	rax, r9
	cmp	rax, -1
	jne	".Lk_sdiv_inst0_divok"
	mov	rax, rsi
	test	rax, rax
	jne	".Lk_sdiv_inst0_divok"
	movabs	rax, 9223372036854775808
	cmp	rax, rdi
	je	".Lk_sdiv_trap"
.Lk_sdiv_inst0_divok:
	mov	rdx, r8
	mov	rcx, r9
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"__divti3"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sdiv_trap:
	ud2
.Lk_sdiv_end:
	.cfi_endproc
	.size "k_sdiv", .-"k_sdiv"
	.section	".text","ax",@progbits
	.globl "k_urem"
	.p2align	4
	.type	"k_urem",@function
"k_urem":
.Lk_urem_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_urem_block0:
	# v2 = urem v0, v1
	mov	rax, r8
	or	rax, r9
	je	".Lk_urem_trap"
	mov	rdx, r8
	mov	rcx, r9
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"__umodti3"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_urem_trap:
	ud2
.Lk_urem_end:
	.cfi_endproc
	.size "k_urem", .-"k_urem"
	.section	".text","ax",@progbits
	.globl "k_srem"
	.p2align	4
	.type	"k_srem",@function
"k_srem":
.Lk_srem_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_srem_block0:
	# v2 = srem v0, v1
	mov	rax, r8
	or	rax, r9
	je	".Lk_srem_trap"
	mov	rdx, r8
	mov	rcx, r9
	mov	r11, rdi
	mov	rdi, rsi
	mov	rsi, r11
	call	"__modti3"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_srem_trap:
	ud2
.Lk_srem_end:
	.cfi_endproc
	.size "k_srem", .-"k_srem"
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_and_block0:
	# v2 = and v0, v1
	mov	rax, rsi
	and	rax, r8
	mov	rdx, rdi
	and	rdx, r9
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_or_block0:
	# v2 = or v0, v1
	mov	rax, rsi
	or	rax, r8
	mov	rdx, rdi
	or	rdx, r9
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
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
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_xor_block0:
	# v2 = xor v0, v1
	mov	rax, rsi
	xor	rax, r8
	mov	rdx, rdi
	xor	rdx, r9
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_xor_end:
	.cfi_endproc
	.size "k_xor", .-"k_xor"
	.section	".text","ax",@progbits
	.globl "k_shl"
	.p2align	4
	.type	"k_shl",@function
"k_shl":
.Lk_shl_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_shl_block0:
	# v2 = shl v0, v1
	mov	rcx, r8
	mov	rax, rsi
	mov	rdx, rdi
	shld	rdx, rax, cl
	shl	rax, cl
	test	cl, 64
	je	".Lk_shl_inst0_shifted"
	mov	rdx, rax
	xor	eax, eax
.Lk_shl_inst0_shifted:
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_shl_end:
	.cfi_endproc
	.size "k_shl", .-"k_shl"
	.section	".text","ax",@progbits
	.globl "k_ushr"
	.p2align	4
	.type	"k_ushr",@function
"k_ushr":
.Lk_ushr_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_ushr_block0:
	# v2 = ushr v0, v1
	mov	rcx, r8
	mov	rax, rsi
	mov	rdx, rdi
	shrd	rax, rdx, cl
	shr	rdx, cl
	test	cl, 64
	je	".Lk_ushr_inst0_shifted"
	mov	rax, rdx
	xor	edx, edx
.Lk_ushr_inst0_shifted:
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ushr_end:
	.cfi_endproc
	.size "k_ushr", .-"k_ushr"
	.section	".text","ax",@progbits
	.globl "k_sshr"
	.p2align	4
	.type	"k_sshr",@function
"k_sshr":
.Lk_sshr_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sshr_block0:
	# v2 = sshr v0, v1
	mov	rcx, r8
	mov	rax, rsi
	mov	rdx, rdi
	shrd	rax, rdx, cl
	sar	rdx, cl
	test	cl, 64
	je	".Lk_sshr_inst0_shifted"
	mov	rax, rdx
	sar	rdx, 63
.Lk_sshr_inst0_shifted:
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sshr_end:
	.cfi_endproc
	.size "k_sshr", .-"k_sshr"
	.section	".text","ax",@progbits
	.globl "k_rotl"
	.p2align	4
	.type	"k_rotl",@function
"k_rotl":
.Lk_rotl_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_rotl_block0:
	# v2 = rotl v0, v1
	mov	rcx, r8
	mov	rax, rsi
	mov	rdx, rdi
	test	cl, 64
	je	".Lk_rotl_inst0_swapped"
	xchg	rax, rdx
.Lk_rotl_inst0_swapped:
	mov	r11, rax
	shld	rax, rdx, cl
	shld	rdx, r11, cl
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_rotl_end:
	.cfi_endproc
	.size "k_rotl", .-"k_rotl"
	.section	".text","ax",@progbits
	.globl "k_rotr"
	.p2align	4
	.type	"k_rotr",@function
"k_rotr":
.Lk_rotr_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_rotr_block0:
	# v2 = rotr v0, v1
	mov	rcx, r8
	mov	rax, rsi
	mov	rdx, rdi
	test	cl, 64
	je	".Lk_rotr_inst0_swapped"
	xchg	rax, rdx
.Lk_rotr_inst0_swapped:
	mov	r11, rax
	shrd	rax, rdx, cl
	shrd	rdx, r11, cl
	mov	rsi, rax
	mov	rdi, rdx
	# return v2
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_rotr_end:
	.cfi_endproc
	.size "k_rotr", .-"k_rotr"
	.section	".text","ax",@progbits
	.globl "k_ineg"
	.p2align	4
	.type	"k_ineg",@function
"k_ineg":
.Lk_ineg_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_ineg_block0:
	# v1 = ineg v0
	mov	rax, rsi
	mov	rdx, rdi
	neg	rax
	adc	rdx, 0
	neg	rdx
	mov	rsi, rax
	mov	rdi, rdx
	# return v1
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ineg_end:
	.cfi_endproc
	.size "k_ineg", .-"k_ineg"
	.section	".text","ax",@progbits
	.globl "k_bnot"
	.p2align	4
	.type	"k_bnot",@function
"k_bnot":
.Lk_bnot_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_bnot_block0:
	# v1 = bnot v0
	mov	rax, rsi
	mov	rdx, rdi
	not	rax
	not	rdx
	mov	rsi, rax
	mov	rdi, rdx
	# return v1
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_bnot_end:
	.cfi_endproc
	.size "k_bnot", .-"k_bnot"
	.section	".text","ax",@progbits
	.globl "k_clz"
	.p2align	4
	.type	"k_clz",@function
"k_clz":
.Lk_clz_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_clz_block0:
	# v1 = clz v0
	mov	rdx, rdi
	test	rdx, rdx
	je	".Lk_clz_inst0_low"
	bsr	rax, rdx
	xor	eax, 63
	jmp	".Lk_clz_inst0_counted"
.Lk_clz_inst0_low:
	mov	rdx, rsi
	bsr	rax, rdx
	mov	ecx, 127
	cmove	eax, ecx
	xor	eax, 63
	add	eax, 64
.Lk_clz_inst0_counted:
	xor	edx, edx
	mov	rsi, rax
	mov	rdi, rdx
	# return v1
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_clz_end:
	.cfi_endproc
	.size "k_clz", .-"k_clz"
	.section	".text","ax",@progbits
	.globl "k_ctz"
	.p2align	4
	.type	"k_ctz",@function
"k_ctz":
.Lk_ctz_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_ctz_block0:
	# v1 = ctz v0
	mov	rdx, rsi
	test	rdx, rdx
	je	".Lk_ctz_inst0_high"
	bsf	rax, rdx
	jmp	".Lk_ctz_inst0_counted"
.Lk_ctz_inst0_high:
	mov	rdx, rdi
	bsf	rax, rdx
	mov	ecx, 64
	cmove	eax, ecx
	add	eax, 64
.Lk_ctz_inst0_counted:
	xor	edx, edx
	mov	rsi, rax
	mov	rdi, rdx
	# return v1
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ctz_end:
	.cfi_endproc
	.size "k_ctz", .-"k_ctz"
	.section	".text","ax",@progbits
	.globl "k_popcnt"
	.p2align	4
	.type	"k_popcnt",@function
"k_popcnt":
.Lk_popcnt_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_popcnt_block0:
	# v1 = popcnt v0
	movq	xmm14, rdi
	mov	rax, rsi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	movq	xmm15, rax
	movq	rax, xmm14
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	movq	rcx, xmm15
	add	rax, rcx
	xor	edx, edx
	mov	rsi, rax
	mov	rdi, rdx
	# return v1
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_popcnt_end:
	.cfi_endproc
	.size "k_popcnt", .-"k_popcnt"
	.section	".text","ax",@progbits
	.globl "k_bswap"
	.p2align	4
	.type	"k_bswap",@function
"k_bswap":
.Lk_bswap_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_bswap_block0:
	# v1 = bswap v0
	mov	rax, rdi
	mov	rdx, rsi
	bswap	rax
	bswap	rdx
	mov	rsi, rax
	mov	rdi, rdx
	# return v1
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_bswap_end:
	.cfi_endproc
	.size "k_bswap", .-"k_bswap"
	.section	".text","ax",@progbits
	.globl "k_eq"
	.p2align	4
	.type	"k_eq",@function
"k_eq":
.Lk_eq_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_eq_block0:
	# v2 = icmp.eq v0, v1
	mov	rax, rsi
	xor	rax, r8
	mov	rdx, rdi
	xor	rdx, r9
	or	rax, rdx
	sete	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_eq_end:
	.cfi_endproc
	.size "k_eq", .-"k_eq"
	.section	".text","ax",@progbits
	.globl "k_ne"
	.p2align	4
	.type	"k_ne",@function
"k_ne":
.Lk_ne_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_ne_block0:
	# v2 = icmp.ne v0, v1
	mov	rax, rsi
	xor	rax, r8
	mov	rdx, rdi
	xor	rdx, r9
	or	rax, rdx
	setne	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ne_end:
	.cfi_endproc
	.size "k_ne", .-"k_ne"
	.section	".text","ax",@progbits
	.globl "k_slt"
	.p2align	4
	.type	"k_slt",@function
"k_slt":
.Lk_slt_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_slt_block0:
	# v2 = icmp.slt v0, v1
	mov	rax, rsi
	cmp	rax, r8
	mov	rdx, rdi
	sbb	rdx, r9
	setl	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_slt_end:
	.cfi_endproc
	.size "k_slt", .-"k_slt"
	.section	".text","ax",@progbits
	.globl "k_sle"
	.p2align	4
	.type	"k_sle",@function
"k_sle":
.Lk_sle_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sle_block0:
	# v2 = icmp.sle v0, v1
	mov	rax, r8
	cmp	rax, rsi
	mov	rdx, r9
	sbb	rdx, rdi
	setge	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sle_end:
	.cfi_endproc
	.size "k_sle", .-"k_sle"
	.section	".text","ax",@progbits
	.globl "k_sgt"
	.p2align	4
	.type	"k_sgt",@function
"k_sgt":
.Lk_sgt_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sgt_block0:
	# v2 = icmp.sgt v0, v1
	mov	rax, r8
	cmp	rax, rsi
	mov	rdx, r9
	sbb	rdx, rdi
	setl	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sgt_end:
	.cfi_endproc
	.size "k_sgt", .-"k_sgt"
	.section	".text","ax",@progbits
	.globl "k_sge"
	.p2align	4
	.type	"k_sge",@function
"k_sge":
.Lk_sge_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_sge_block0:
	# v2 = icmp.sge v0, v1
	mov	rax, rsi
	cmp	rax, r8
	mov	rdx, rdi
	sbb	rdx, r9
	setge	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_sge_end:
	.cfi_endproc
	.size "k_sge", .-"k_sge"
	.section	".text","ax",@progbits
	.globl "k_ult"
	.p2align	4
	.type	"k_ult",@function
"k_ult":
.Lk_ult_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_ult_block0:
	# v2 = icmp.ult v0, v1
	mov	rax, rsi
	cmp	rax, r8
	mov	rdx, rdi
	sbb	rdx, r9
	setb	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ult_end:
	.cfi_endproc
	.size "k_ult", .-"k_ult"
	.section	".text","ax",@progbits
	.globl "k_ule"
	.p2align	4
	.type	"k_ule",@function
"k_ule":
.Lk_ule_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_ule_block0:
	# v2 = icmp.ule v0, v1
	mov	rax, r8
	cmp	rax, rsi
	mov	rdx, r9
	sbb	rdx, rdi
	setae	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ule_end:
	.cfi_endproc
	.size "k_ule", .-"k_ule"
	.section	".text","ax",@progbits
	.globl "k_ugt"
	.p2align	4
	.type	"k_ugt",@function
"k_ugt":
.Lk_ugt_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_ugt_block0:
	# v2 = icmp.ugt v0, v1
	mov	rax, r8
	cmp	rax, rsi
	mov	rdx, r9
	sbb	rdx, rdi
	setb	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_ugt_end:
	.cfi_endproc
	.size "k_ugt", .-"k_ugt"
	.section	".text","ax",@progbits
	.globl "k_uge"
	.p2align	4
	.type	"k_uge",@function
"k_uge":
.Lk_uge_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r8, rdx
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_uge_block0:
	# v2 = icmp.uge v0, v1
	mov	rax, rsi
	cmp	rax, r8
	mov	rdx, rdi
	sbb	rdx, r9
	setae	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_uge_end:
	.cfi_endproc
	.size "k_uge", .-"k_uge"
	.section	".text","ax",@progbits
	.globl "k_select"
	.p2align	4
	.type	"k_select",@function
"k_select":
.Lk_select_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_select_block0:
	# v3 = select v0, v1, v2
	mov	rax, rsi
	or	rax, rdi
	mov	rax, rbx
	cmovne	rax, r8
	mov	rdx, r12
	cmovne	rdx, r9
	mov	rsi, rax
	mov	rdi, rdx
	# return v3
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -16]
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_select_end:
	.cfi_endproc
	.size "k_select", .-"k_select"
	.section	".text","ax",@progbits
	.globl "k_select32"
	.p2align	4
	.type	"k_select32",@function
"k_select32":
.Lk_select32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	r9, rcx
	mov	rbx, r8
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_select32_block0:
	# v3 = select v0, v1, v2
	test	esi, esi
	mov	rax, r9
	cmovne	rax, rdi
	mov	rdx, rbx
	cmovne	rdx, r8
	mov	rsi, rax
	mov	rdi, rdx
	# return v3
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_select32_end:
	.cfi_endproc
	.size "k_select32", .-"k_select32"
	.section	".text","ax",@progbits
	.globl "k_truth"
	.p2align	4
	.type	"k_truth",@function
"k_truth":
.Lk_truth_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_truth_block0:
	# v1 = iconst.i64 1
	mov	r8d, 1
	# v2 = iconst.i64 0
	xor	r9d, r9d
	# v3 = select v0, v1, v2
	mov	rax, rsi
	or	rax, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	r8, rax
	# brif v0, block1, block2
	mov	rax, rsi
	or	rax, rdi
	jne	".Lk_truth_block1"
.Lk_truth_block2:
	# v6 = iconst.i64 4
	# v7 = iadd v3, v6
	add	r8, 4
	# return v7
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_truth_block1:
	# v4 = iconst.i64 2
	# v5 = iadd v3, v4
	add	r8, 2
	# return v5
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_truth_end:
	.cfi_endproc
	.size "k_truth", .-"k_truth"
	.section	".text","ax",@progbits
	.globl "k_ext"
	.p2align	4
	.type	"k_ext",@function
"k_ext":
.Lk_ext_begin:
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
	sub	rsp, 104
	mov	rsi, rdi
.Lk_ext_block0:
	# v1 = trunc.i32 v0
	mov	rdi, rsi
	# v2 = trunc.i8 v0
	mov	r8, rsi
	# v3 = sext.i128 v0
	mov	rax, rsi
	mov	rdx, rax
	sar	rdx, 63
	mov	qword ptr [rbp + -144], rax
	mov	qword ptr [rbp + -136], rdx
	# v4 = uext.i128 v0
	mov	rax, rsi
	xor	edx, edx
	mov	qword ptr [rbp + -128], rax
	mov	qword ptr [rbp + -120], rdx
	# v5 = sext.i128 v1
	movsxd	rax, edi
	mov	rdx, rax
	sar	rdx, 63
	mov	qword ptr [rbp + -112], rax
	mov	qword ptr [rbp + -104], rdx
	# v6 = uext.i128 v1
	mov	eax, edi
	xor	edx, edx
	mov	qword ptr [rbp + -96], rax
	mov	qword ptr [rbp + -88], rdx
	# v7 = sext.i128 v2
	movsx	rax, r8b
	mov	rdx, rax
	sar	rdx, 63
	mov	qword ptr [rbp + -80], rax
	mov	qword ptr [rbp + -72], rdx
	# v8 = uext.i128 v2
	movzx	eax, r8b
	xor	edx, edx
	mov	qword ptr [rbp + -64], rax
	mov	qword ptr [rbp + -56], rdx
	# v9 = iconst.i128 3
	mov	eax, 3
	xor	edx, edx
	mov	r8, rax
	mov	r9, rdx
	# v10 = iconst.i128 5
	mov	eax, 5
	xor	edx, edx
	mov	rbx, rax
	mov	rdi, rdx
	# v11 = iconst.i128 7
	mov	eax, 7
	xor	edx, edx
	mov	r15, rax
	mov	r14, rdx
	# v12 = iconst.i128 11
	mov	eax, 11
	xor	edx, edx
	mov	r13, rax
	mov	r12, rdx
	# v13 = iconst.i128 13
	mov	eax, 13
	xor	edx, edx
	mov	rsi, rax
	mov	qword ptr [rbp + -48], rdx
	# v14 = imul v4, v9
	mov	rax, qword ptr [rbp + -128]
	mul	r8
	mov	rcx, qword ptr [rbp + -128]
	imul	rcx, r9
	add	rdx, rcx
	mov	rcx, qword ptr [rbp + -120]
	imul	rcx, r8
	add	rdx, rcx
	mov	r8, rax
	mov	r9, rdx
	# v15 = imul v5, v10
	mov	rax, qword ptr [rbp + -112]
	mul	rbx
	mov	rcx, qword ptr [rbp + -112]
	imul	rcx, rdi
	add	rdx, rcx
	mov	rcx, qword ptr [rbp + -104]
	imul	rcx, rbx
	add	rdx, rcx
	mov	rdi, rax
	mov	rbx, rdx
	# v16 = imul v6, v11
	mov	rax, qword ptr [rbp + -96]
	mul	r15
	mov	rcx, qword ptr [rbp + -96]
	imul	rcx, r14
	add	rdx, rcx
	mov	rcx, qword ptr [rbp + -88]
	imul	rcx, r15
	add	rdx, rcx
	mov	r14, rax
	mov	r15, rdx
	# v17 = imul v7, v12
	mov	rax, qword ptr [rbp + -80]
	mul	r13
	mov	rcx, qword ptr [rbp + -80]
	imul	rcx, r12
	add	rdx, rcx
	mov	rcx, qword ptr [rbp + -72]
	imul	rcx, r13
	add	rdx, rcx
	mov	r12, rax
	mov	r13, rdx
	# v18 = imul v8, v13
	mov	rax, qword ptr [rbp + -64]
	mul	rsi
	mov	rcx, qword ptr [rbp + -64]
	imul	rcx, qword ptr [rbp + -48]
	add	rdx, rcx
	mov	rcx, qword ptr [rbp + -56]
	imul	rcx, rsi
	add	rdx, rcx
	mov	rsi, rax
	mov	qword ptr [rbp + -128], rdx
	# v19 = iadd v3, v14
	mov	rax, qword ptr [rbp + -144]
	add	rax, r8
	mov	rdx, qword ptr [rbp + -136]
	adc	rdx, r9
	mov	r8, rax
	mov	r9, rdx
	# v20 = iadd v15, v19
	mov	rax, rdi
	add	rax, r8
	mov	rdx, rbx
	adc	rdx, r9
	mov	rdi, rax
	mov	rbx, rdx
	# v21 = iadd v16, v20
	mov	rax, r14
	add	rax, rdi
	mov	rdx, r15
	adc	rdx, rbx
	mov	r14, rax
	mov	r15, rdx
	# v22 = iadd v17, v21
	mov	rax, r12
	add	rax, r14
	mov	rdx, r13
	adc	rdx, r15
	mov	r12, rax
	mov	r13, rdx
	# v23 = iadd v18, v22
	mov	rax, rsi
	add	rax, r12
	mov	rdx, qword ptr [rbp + -128]
	adc	rdx, r13
	mov	rsi, rax
	mov	rdi, rdx
	# return v23
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_ext_end:
	.cfi_endproc
	.size "k_ext", .-"k_ext"
	.section	".text","ax",@progbits
	.globl "k_lo"
	.p2align	4
	.type	"k_lo",@function
"k_lo":
.Lk_lo_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_lo_block0:
	# v1 = trunc.i64 v0
	# return v1
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_lo_end:
	.cfi_endproc
	.size "k_lo", .-"k_lo"
	.section	".text","ax",@progbits
	.globl "k_hi"
	.p2align	4
	.type	"k_hi",@function
"k_hi":
.Lk_hi_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_hi_block0:
	# v1 = iconst.i32 64
	mov	r8d, 64
	# v2 = ushr v0, v1
	mov	rax, rdi
	xor	edx, edx
	mov	rsi, rax
	mov	rdi, rdx
	# v3 = trunc.i64 v2
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_hi_end:
	.cfi_endproc
	.size "k_hi", .-"k_hi"
	.section	".text","ax",@progbits
	.globl "k_mem"
	.p2align	4
	.type	"k_mem",@function
"k_mem":
.Lk_mem_begin:
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
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_mem_block0:
	# v2 = load.i128 v0
	mov	rax, qword ptr [rsi]
	mov	rdx, qword ptr [rsi + 8]
	mov	r8, rax
	mov	r9, rdx
	# v3 = load.i128 v1
	mov	rax, qword ptr [rdi]
	mov	rdx, qword ptr [rdi + 8]
	mov	rdi, rax
	mov	rbx, rdx
	# v4 = iconst.i128 1
	mov	eax, 1
	xor	edx, edx
	mov	r12, rax
	mov	r13, rdx
	# v5 = iadd v3, v4
	mov	rax, rdi
	add	rax, r12
	mov	rdx, rbx
	adc	rdx, r13
	mov	rdi, rax
	mov	rbx, rdx
	# store v5, v0
	mov	rax, rdi
	mov	qword ptr [rsi], rax
	mov	rax, rbx
	mov	qword ptr [rsi + 8], rax
	# return v2
	mov	rax, r8
	mov	rdx, r9
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_mem_end:
	.cfi_endproc
	.size "k_mem", .-"k_mem"
	.section	".text","ax",@progbits
	.globl "k_minus_one"
	.p2align	4
	.type	"k_minus_one",@function
"k_minus_one":
.Lk_minus_one_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lk_minus_one_block0:
	# v0 = iconst.i128 -1
	mov	rax, -1
	mov	rdx, -1
	mov	rsi, rax
	mov	rdi, rdx
	# return v0
	mov	rax, rsi
	mov	rdx, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lk_minus_one_end:
	.cfi_endproc
	.size "k_minus_one", .-"k_minus_one"
	.section	".text","ax",@progbits
	.globl "k_args"
	.p2align	4
	.type	"k_args",@function
"k_args":
.Lk_args_begin:
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
.Lk_args_block0:
	# v3 = sext.i128 v0
	mov	rax, rsi
	mov	rdx, rax
	sar	rdx, 63
	mov	rsi, rax
	mov	rbx, rdx
	# v4 = iconst.i128 3
	mov	eax, 3
	xor	edx, edx
	mov	r12, rax
	mov	r13, rdx
	# v5 = imul v1, v4
	mov	rax, rdi
	mul	r12
	mov	rcx, rdi
	imul	rcx, r13
	add	rdx, rcx
	mov	rcx, r8
	imul	rcx, r12
	add	rdx, rcx
	mov	rdi, rax
	mov	r8, rdx
	# v6 = iadd v3, v5
	mov	rax, rsi
	add	rax, rdi
	mov	rdx, rbx
	adc	rdx, r8
	mov	rsi, rax
	mov	rbx, rdx
	# v7 = sext.i128 v2
	mov	rax, r9
	mov	rdx, rax
	sar	rdx, 63
	mov	rdi, rax
	mov	r8, rdx
	# v8 = isub v6, v7
	mov	rax, rsi
	sub	rax, rdi
	mov	rdx, rbx
	sbb	rdx, r8
	mov	rsi, rax
	mov	rbx, rdx
	# return v8
	mov	rax, rsi
	mov	rdx, rbx
	lea	rsp, [rbp + -24]
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_args_end:
	.cfi_endproc
	.size "k_args", .-"k_args"
	.section	".text","ax",@progbits
	.globl "k_tail"
	.p2align	4
	.type	"k_tail",@function
"k_tail":
.Lk_tail_begin:
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
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
	mov	r13, qword ptr [rbp + 16]
	mov	r14, qword ptr [rbp + 32]
	mov	r15, qword ptr [rbp + 40]
	mov	rax, qword ptr [rbp + 48]
	mov	qword ptr [rbp + -48], rax
.Lk_tail_block0:
	# v9 = iadd v0, v1
	add	rsi, rdi
	# v10 = iadd v2, v9
	add	r8, rsi
	# v11 = iadd v3, v10
	add	r9, r8
	# v12 = iadd v4, v11
	add	rbx, r9
	# v13 = iadd v5, v12
	add	r12, rbx
	# v14 = iadd v6, v13
	add	r13, r12
	# v15 = sext.i128 v14
	mov	rax, r13
	mov	rdx, rax
	sar	rdx, 63
	mov	rsi, rax
	mov	rdi, rdx
	# v16 = iconst.i128 1000
	mov	eax, 1000
	xor	edx, edx
	mov	r8, rax
	mov	r9, rdx
	# v17 = imul v7, v16
	mov	rax, r14
	mul	r8
	mov	rcx, r14
	imul	rcx, r9
	add	rdx, rcx
	mov	rcx, r15
	imul	rcx, r8
	add	rdx, rcx
	mov	r14, rax
	mov	r15, rdx
	# v18 = iadd v15, v17
	mov	rax, rsi
	add	rax, r14
	mov	rdx, rdi
	adc	rdx, r15
	mov	rsi, rax
	mov	rdi, rdx
	# v19 = sext.i128 v8
	mov	rax, qword ptr [rbp + -48]
	mov	rax, rax
	mov	rdx, rax
	sar	rdx, 63
	mov	r8, rax
	mov	r9, rdx
	# v20 = iconst.i128 7
	mov	eax, 7
	xor	edx, edx
	mov	rbx, rax
	mov	r12, rdx
	# v21 = imul v19, v20
	mov	rax, r8
	mul	rbx
	mov	rcx, r8
	imul	rcx, r12
	add	rdx, rcx
	mov	rcx, r9
	imul	rcx, rbx
	add	rdx, rcx
	mov	r8, rax
	mov	r9, rdx
	# v22 = iadd v18, v21
	mov	rax, rsi
	add	rax, r8
	mov	rdx, rdi
	adc	rdx, r9
	mov	rsi, rax
	mov	rdi, rdx
	# return v22
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_tail_end:
	.cfi_endproc
	.size "k_tail", .-"k_tail"
	.section	".text","ax",@progbits
	.globl "k_call_c"
	.p2align	4
	.type	"k_call_c",@function
"k_call_c":
.Lk_call_c_begin:
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
	sub	rsp, 72
	mov	rbx, rdi
	mov	qword ptr [rbp + -64], rsi
.Lk_call_c_block0:
	# v1 = iconst.i64 1
	mov	r13d, 1
	# v2 = iconst.i64 2
	mov	r14d, 2
	# v3 = call @c_args(v1, v0, v2)
	mov	rdi, r13
	mov	rsi, rbx
	mov	rdx, qword ptr [rbp + -64]
	mov	rcx, r14
	call	"c_args"@PLT
	mov	qword ptr [rbp + -56], rax
	mov	qword ptr [rbp + -48], rdx
	# v4 = iconst.i64 3
	mov	esi, 3
	# v5 = iconst.i64 4
	mov	edi, 4
	# v6 = iconst.i64 5
	mov	r8d, 5
	# v7 = iconst.i64 6
	mov	r9d, 6
	# v8 = iconst.i64 7
	mov	r15d, 7
	# v9 = iconst.i64 -5
	mov	r12, -5
	# v10 = call @c_tail(v1, v2, v4, v5, v6, v7, v8, v0, v9)
	mov	qword ptr [rsp], r15
	mov	qword ptr [rsp + 16], rbx
	mov	rax, qword ptr [rbp + -64]
	mov	qword ptr [rsp + 24], rax
	mov	qword ptr [rsp + 32], r12
	mov	rdx, rsi
	mov	rsi, r14
	mov	rcx, rdi
	mov	rdi, r13
	call	"c_tail"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# v11 = iadd v3, v10
	mov	rax, qword ptr [rbp + -56]
	add	rax, rsi
	mov	rdx, qword ptr [rbp + -48]
	adc	rdx, rdi
	mov	rsi, rax
	mov	rdi, rdx
	# v12 = iadd v0, v11
	mov	rax, rbx
	add	rax, rsi
	mov	rdx, qword ptr [rbp + -64]
	adc	rdx, rdi
	mov	rbx, rax
	mov	rsi, rdx
	# return v12
	mov	rax, rbx
	mov	rdx, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_call_c_end:
	.cfi_endproc
	.size "k_call_c", .-"k_call_c"
	.section	".text","ax",@progbits
	.globl "k_pow"
	.p2align	4
	.type	"k_pow",@function
"k_pow":
.Lk_pow_begin:
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
	mov	r8, rdx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lk_pow_block0:
	# v2 = iconst.i128 1
	mov	eax, 1
	xor	edx, edx
	mov	r9, rax
	mov	rbx, rdx
	# jump block1(v0, v1, v2)
.Lk_pow_block1:
	# brif v4, block2, block3
	test	r8, r8
	jne	".Lk_pow_block2"
.Lk_pow_block3:
	# return v5
	mov	rax, r9
	mov	rdx, rbx
	lea	rsp, [rbp + -32]
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_pow_block2:
	# v6 = iconst.i64 1
	# v7 = and v4, v6
	mov	r12, r8
	and	r12, 1
	# v8 = imul v3, v5
	mov	rax, rsi
	mul	r9
	mov	rcx, rsi
	imul	rcx, rbx
	add	rdx, rcx
	mov	rcx, rdi
	imul	rcx, r9
	add	rdx, rcx
	mov	r13, rax
	mov	r14, rdx
	# v9 = select v7, v8, v5
	test	r12, r12
	mov	rax, r9
	cmovne	rax, r13
	mov	rdx, rbx
	cmovne	rdx, r14
	mov	r9, rax
	mov	rbx, rdx
	# v10 = imul v3, v3
	mov	rax, rsi
	mul	rsi
	mov	rcx, rsi
	imul	rcx, rdi
	add	rdx, rcx
	mov	rcx, rdi
	imul	rcx, rsi
	add	rdx, rcx
	mov	rsi, rax
	mov	rdi, rdx
	# v11 = ushr v4, v6
	shr	r8, 1
	# jump block1(v10, v11, v9)
	jmp	".Lk_pow_block1"
.Lk_pow_end:
	.cfi_endproc
	.size "k_pow", .-"k_pow"
	.section	".text","ax",@progbits
	.globl "k_tail16"
	.p2align	4
	.type	"k_tail16",@function
"k_tail16":
.Lk_tail16_begin:
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
	sub	rsp, 24
	mov	rbx, r8
	mov	r8, rdx
	mov	r12, r9
	mov	r9, rcx
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
	mov	r13, qword ptr [rbp + 16]
	mov	r14, qword ptr [rbp + 24]
	mov	r15, qword ptr [rbp + 32]
	mov	rax, qword ptr [rbp + 48]
	mov	qword ptr [rbp + -64], rax
	mov	rax, qword ptr [rbp + 56]
	mov	qword ptr [rbp + -56], rax
.Lk_tail16_block0:
	# v10 = iadd v0, v1
	add	rsi, rdi
	# v11 = iadd v2, v10
	add	r8, rsi
	# v12 = iadd v3, v11
	add	r9, r8
	# v13 = iadd v4, v12
	add	rbx, r9
	# v14 = iadd v5, v13
	add	r12, rbx
	# v15 = iadd v6, v14
	add	r13, r12
	# v16 = iadd v7, v15
	add	r14, r13
	# v17 = iconst.i64 3
	# v18 = imul v8, v17
	imul	r15, r15, 3
	# v19 = iadd v16, v18
	add	r14, r15
	# v20 = sext.i128 v19
	mov	rax, r14
	mov	rdx, rax
	sar	rdx, 63
	mov	rsi, rax
	mov	rdi, rdx
	# v21 = iconst.i128 1000
	mov	eax, 1000
	xor	edx, edx
	mov	r8, rax
	mov	r9, rdx
	# v22 = imul v9, v21
	mov	rax, qword ptr [rbp + -64]
	mul	r8
	mov	rcx, qword ptr [rbp + -64]
	imul	rcx, r9
	add	rdx, rcx
	mov	rcx, qword ptr [rbp + -56]
	imul	rcx, r8
	add	rdx, rcx
	mov	r8, rax
	mov	r9, rdx
	# v23 = iadd v20, v22
	mov	rax, rsi
	add	rax, r8
	mov	rdx, rdi
	adc	rdx, r9
	mov	rsi, rax
	mov	rdi, rdx
	# return v23
	mov	rax, rsi
	mov	rdx, rdi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_tail16_end:
	.cfi_endproc
	.size "k_tail16", .-"k_tail16"
	.section	".text","ax",@progbits
	.globl "k_rem_live"
	.p2align	4
	.type	"k_rem_live",@function
"k_rem_live":
.Lk_rem_live_begin:
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
	mov	rbx, rdi
	mov	r12, rsi
	mov	r13, rdx
	mov	r14, rcx
	mov	r15, r8
.Lk_rem_live_block0:
	# v3 = urem v0, v1
	mov	rax, r13
	or	rax, r14
	je	".Lk_rem_live_trap"
	mov	rdi, rbx
	mov	rsi, r12
	mov	rdx, r13
	mov	rcx, r14
	call	"__umodti3"@PLT
	mov	rsi, rax
	mov	rdi, rdx
	# v4 = iadd v0, v3
	mov	rax, rbx
	add	rax, rsi
	mov	rdx, r12
	adc	rdx, rdi
	mov	rbx, rax
	mov	r12, rdx
	# v5 = iadd v1, v4
	mov	rax, r13
	add	rax, rbx
	mov	rdx, r14
	adc	rdx, r12
	mov	r13, rax
	mov	r14, rdx
	# v6 = sext.i128 v2
	mov	rax, r15
	mov	rdx, rax
	sar	rdx, 63
	mov	rsi, rax
	mov	rdi, rdx
	# v7 = iadd v5, v6
	mov	rax, r13
	add	rax, rsi
	mov	rdx, r14
	adc	rdx, rdi
	mov	r13, rax
	mov	r14, rdx
	# return v7
	mov	rax, r13
	mov	rdx, r14
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_rem_live_trap:
	ud2
.Lk_rem_live_end:
	.cfi_endproc
	.size "k_rem_live", .-"k_rem_live"
	.section	".note.GNU-stack","",@progbits
