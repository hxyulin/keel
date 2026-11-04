	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "udiv32"
	.p2align	4
	.type	"udiv32",@function
"udiv32":
.Ludiv32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ludiv32_block0:
	# v2 = udiv v0, v1
	mov	eax, esi
	test	edi, edi
	je	".Ludiv32_trap"
	xor	edx, edx
	div	edi
.Ludiv32_inst0_div:
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ludiv32_trap:
	ud2
.Ludiv32_end:
	.cfi_endproc
	.size "udiv32", .-"udiv32"
	.section	".text","ax",@progbits
	.globl "sdiv64"
	.p2align	4
	.type	"sdiv64",@function
"sdiv64":
.Lsdiv64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsdiv64_block0:
	# v2 = sdiv v0, v1
	mov	rax, rsi
	test	rdi, rdi
	je	".Lsdiv64_trap"
	cmp	rdi, -1
	jne	".Lsdiv64_inst0_divide"
	movabs	rdx, 9223372036854775808
	cmp	rax, rdx
	je	".Lsdiv64_trap"
.Lsdiv64_inst0_divide:
	cqo
	idiv	rdi
.Lsdiv64_inst0_div:
	mov	rsi, rax
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsdiv64_trap:
	ud2
.Lsdiv64_end:
	.cfi_endproc
	.size "sdiv64", .-"sdiv64"
	.section	".text","ax",@progbits
	.globl "srem32"
	.p2align	4
	.type	"srem32",@function
"srem32":
.Lsrem32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsrem32_block0:
	# v2 = srem v0, v1
	mov	eax, esi
	test	edi, edi
	je	".Lsrem32_trap"
	cmp	edi, -1
	jne	".Lsrem32_inst0_divide"
	xor	edx, edx
	jmp	".Lsrem32_inst0_div"
.Lsrem32_inst0_divide:
	cdq
	idiv	edi
.Lsrem32_inst0_div:
	mov	rsi, rdx
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsrem32_trap:
	ud2
.Lsrem32_end:
	.cfi_endproc
	.size "srem32", .-"srem32"
	.section	".text","ax",@progbits
	.globl "urem64"
	.p2align	4
	.type	"urem64",@function
"urem64":
.Lurem64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lurem64_block0:
	# v2 = urem v0, v1
	mov	rax, rsi
	test	rdi, rdi
	je	".Lurem64_trap"
	xor	edx, edx
	div	rdi
.Lurem64_inst0_div:
	mov	rsi, rdx
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lurem64_trap:
	ud2
.Lurem64_end:
	.cfi_endproc
	.size "urem64", .-"urem64"
	.section	".text","ax",@progbits
	.globl "sdiv8"
	.p2align	4
	.type	"sdiv8",@function
"sdiv8":
.Lsdiv8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsdiv8_block0:
	# v2 = trunc.i8 v0
	# v3 = trunc.i8 v1
	# v4 = sdiv v2, v3
	movsx	ecx, dil
	movsx	eax, sil
	test	ecx, ecx
	je	".Lsdiv8_trap"
	cmp	ecx, -1
	jne	".Lsdiv8_inst2_divide"
	cmp	eax, -128
	je	".Lsdiv8_trap"
.Lsdiv8_inst2_divide:
	cdq
	idiv	ecx
.Lsdiv8_inst2_div:
	mov	rsi, rax
	# v5 = sext.i32 v4
	movsx	esi, sil
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsdiv8_trap:
	ud2
.Lsdiv8_end:
	.cfi_endproc
	.size "sdiv8", .-"sdiv8"
	.section	".text","ax",@progbits
	.globl "add8"
	.p2align	4
	.type	"add8",@function
"add8":
.Ladd8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ladd8_block0:
	# v2 = trunc.i8 v0
	# v3 = trunc.i8 v1
	# v4 = iadd v2, v3
	add	esi, edi
	# v5 = sext.i32 v4
	movsx	esi, sil
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ladd8_end:
	.cfi_endproc
	.size "add8", .-"add8"
	.section	".text","ax",@progbits
	.globl "mul32"
	.p2align	4
	.type	"mul32",@function
"mul32":
.Lmul32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lmul32_block0:
	# v2 = imul v0, v1
	imul	esi, edi
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmul32_end:
	.cfi_endproc
	.size "mul32", .-"mul32"
	.section	".text","ax",@progbits
	.globl "shl8"
	.p2align	4
	.type	"shl8",@function
"shl8":
.Lshl8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lshl8_block0:
	# v2 = trunc.i8 v0
	# v3 = trunc.i8 v1
	# v4 = shl v2, v3
	mov	ecx, edi
	and	ecx, 7
	shl	esi, cl
	# v5 = uext.i32 v4
	movzx	esi, sil
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lshl8_end:
	.cfi_endproc
	.size "shl8", .-"shl8"
	.section	".text","ax",@progbits
	.globl "ushr8"
	.p2align	4
	.type	"ushr8",@function
"ushr8":
.Lushr8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lushr8_block0:
	# v2 = trunc.i8 v0
	# v3 = trunc.i8 v1
	# v4 = ushr v2, v3
	mov	ecx, edi
	and	ecx, 7
	movzx	eax, sil
	mov	rsi, rax
	shr	esi, cl
	# v5 = uext.i32 v4
	movzx	esi, sil
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lushr8_end:
	.cfi_endproc
	.size "ushr8", .-"ushr8"
	.section	".text","ax",@progbits
	.globl "sshr16"
	.p2align	4
	.type	"sshr16",@function
"sshr16":
.Lsshr16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsshr16_block0:
	# v2 = trunc.i16 v0
	# v3 = trunc.i16 v1
	# v4 = sshr v2, v3
	mov	ecx, edi
	and	ecx, 15
	movsx	eax, si
	mov	rsi, rax
	sar	esi, cl
	# v5 = sext.i32 v4
	movsx	esi, si
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsshr16_end:
	.cfi_endproc
	.size "sshr16", .-"sshr16"
	.section	".text","ax",@progbits
	.globl "shl64"
	.p2align	4
	.type	"shl64",@function
"shl64":
.Lshl64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lshl64_block0:
	# v2 = shl v0, v1
	mov	ecx, edi
	shl	rsi, cl
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lshl64_end:
	.cfi_endproc
	.size "shl64", .-"shl64"
	.section	".text","ax",@progbits
	.globl "bitops"
	.p2align	4
	.type	"bitops",@function
"bitops":
.Lbitops_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lbitops_block0:
	# v2 = and v0, v1
	mov	r8, rsi
	and	r8, rdi
	# v3 = or v0, v1
	or	rsi, rdi
	# v4 = xor v2, v3
	xor	r8, rsi
	# v5 = isub v4, v1
	sub	r8, rdi
	# return v5
	mov	rax, r8
	mov	rsp, rbp
	pop	rbp
	ret
.Lbitops_end:
	.cfi_endproc
	.size "bitops", .-"bitops"
	.section	".text","ax",@progbits
	.globl "slt8"
	.p2align	4
	.type	"slt8",@function
"slt8":
.Lslt8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lslt8_block0:
	# v2 = trunc.i8 v0
	# v3 = trunc.i8 v1
	# v4 = icmp.slt v2, v3
	cmp	sil, dil
	setl	sil
	# v5 = uext.i32 v4
	movzx	esi, sil
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lslt8_end:
	.cfi_endproc
	.size "slt8", .-"slt8"
	.section	".text","ax",@progbits
	.globl "ult16"
	.p2align	4
	.type	"ult16",@function
"ult16":
.Lult16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lult16_block0:
	# v2 = trunc.i16 v0
	# v3 = trunc.i16 v1
	# v4 = icmp.ult v2, v3
	cmp	si, di
	setb	sil
	# v5 = uext.i32 v4
	movzx	esi, sil
	# return v5
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lult16_end:
	.cfi_endproc
	.size "ult16", .-"ult16"
	.section	".text","ax",@progbits
	.globl "ugt64"
	.p2align	4
	.type	"ugt64",@function
"ugt64":
.Lugt64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lugt64_block0:
	# v2 = icmp.ugt v0, v1
	cmp	rsi, rdi
	seta	sil
	# v3 = uext.i32 v2
	movzx	esi, sil
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lugt64_end:
	.cfi_endproc
	.size "ugt64", .-"ugt64"
	.section	".text","ax",@progbits
	.globl "sext_low_byte"
	.p2align	4
	.type	"sext_low_byte",@function
"sext_low_byte":
.Lsext_low_byte_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lsext_low_byte_block0:
	# v1 = trunc.i8 v0
	# v2 = sext.i64 v1
	movsx	rsi, sil
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsext_low_byte_end:
	.cfi_endproc
	.size "sext_low_byte", .-"sext_low_byte"
	.section	".text","ax",@progbits
	.globl "uext32"
	.p2align	4
	.type	"uext32",@function
"uext32":
.Luext32_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Luext32_block0:
	# v1 = trunc.i32 v0
	# v2 = uext.i64 v1
	mov	esi, esi
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Luext32_end:
	.cfi_endproc
	.size "uext32", .-"uext32"
	.section	".text","ax",@progbits
	.globl "big_const"
	.p2align	4
	.type	"big_const",@function
"big_const":
.Lbig_const_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lbig_const_block0:
	# v0 = iconst.i64 1311768467463790320
	movabs	rsi, 1311768467463790320
	# return v0
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lbig_const_end:
	.cfi_endproc
	.size "big_const", .-"big_const"
	.section	".text","ax",@progbits
	.globl "neg_const"
	.p2align	4
	.type	"neg_const",@function
"neg_const":
.Lneg_const_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lneg_const_block0:
	# v0 = iconst.i64 -2
	mov	rsi, -2
	# return v0
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lneg_const_end:
	.cfi_endproc
	.size "neg_const", .-"neg_const"
	.section	".text","ax",@progbits
	.globl "store16"
	.p2align	4
	.type	"store16",@function
"store16":
.Lstore16_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lstore16_block0:
	# v2 = trunc.i16 v1
	# store v2, v0
	mov	word ptr [rsi], di
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lstore16_end:
	.cfi_endproc
	.size "store16", .-"store16"
	.section	".text","ax",@progbits
	.globl "load_u8"
	.p2align	4
	.type	"load_u8",@function
"load_u8":
.Lload_u8_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	rsi, rdi
.Lload_u8_block0:
	# v1 = load.i8 v0
	movzx	esi, byte ptr [rsi]
	# v2 = uext.i32 v1
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lload_u8_end:
	.cfi_endproc
	.size "load_u8", .-"load_u8"
	.section	".text","ax",@progbits
	.globl "swap_loop"
	.p2align	4
	.type	"swap_loop",@function
"swap_loop":
.Lswap_loop_begin:
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
.Lswap_loop_block0:
	# jump block1(v0, v1, v2)
.Lswap_loop_block1:
	# v6 = iconst.i64 0
	# v7 = icmp.eq v5, v6
	# brif v7, block3, block2
	cmp	r8, 0
	je	".Lswap_loop_block3"
.Lswap_loop_block2:
	# v8 = iconst.i64 1
	# v9 = isub v5, v8
	sub	r8, 1
	# jump block1(v4, v3, v9)
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
	jmp	".Lswap_loop_block1"
.Lswap_loop_block3:
	# v10 = iconst.i64 10
	# v11 = imul v3, v10
	imul	rsi, rsi, 10
	# v12 = iadd v4, v11
	add	rdi, rsi
	# return v12
	mov	rax, rdi
	mov	rsp, rbp
	pop	rbp
	ret
.Lswap_loop_end:
	.cfi_endproc
	.size "swap_loop", .-"swap_loop"
	.section	".text","ax",@progbits
	.globl "max64"
	.p2align	4
	.type	"max64",@function
"max64":
.Lmax64_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lmax64_block0:
	# v2 = icmp.sgt v0, v1
	# brif v2, block1(v0), block1(v1)
	cmp	rsi, rdi
	jg	".Lmax64_block1"
	mov	rsi, rdi
.Lmax64_block1:
	# return v3
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmax64_end:
	.cfi_endproc
	.size "max64", .-"max64"
	.section	".note.GNU-stack","",@progbits
