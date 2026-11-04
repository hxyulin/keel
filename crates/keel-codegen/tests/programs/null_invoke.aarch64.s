	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_null_invoke
	.p2align	2
_null_invoke:
Lnull_invoke_begin:
	; regalloc: v0=x9 v1=x10 v2=x19 v3=x9 v4=x9 v5=x10 v6=x9
	.cfi_startproc
	.cfi_personality 155, ___gxx_personality_v0
	.cfi_lsda 16, Lnull_invoke_lsda
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x9, x0
Lnull_invoke_block0:
	; v1 = iconst.i64 0
	movz	x10, #0x0
	; v2 = inttoptr.ptr v1
	mov	x19, x10
	; v3 = invoke @null_may_throw(v0), block1, block2
	mov	x0, x9
Lnull_invoke_inst2_begin:
	bl	_null_may_throw
Lnull_invoke_inst2_end:
	mov	x9, x0
	b	Lnull_invoke_block1
Lnull_invoke_block2:
	; v4, v5 = landingpad catch null
	mov	x9, x0
	mov	x10, x1
	; v6 = call @__cxa_begin_catch(v4)
	mov	x0, x9
	bl	___cxa_begin_catch
	mov	x9, x0
	; call @__cxa_end_catch()
	bl	___cxa_end_catch
	; return v2
	mov	x0, x19
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lnull_invoke_block1:
	; return v2
	mov	x0, x19
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lnull_invoke_end:
	.cfi_endproc
	.section	__TEXT,__gcc_except_tab
	.p2align	2
GCC_except_table_null_invoke:
Lnull_invoke_lsda:
	.byte	255
	.byte	155
	.uleb128	Lnull_invoke_ttbase-Lnull_invoke_ttref
Lnull_invoke_ttref:
	.byte	1
	.uleb128	Lnull_invoke_cst_end-Lnull_invoke_cst_begin
Lnull_invoke_cst_begin:
	.uleb128	Lnull_invoke_begin-Lnull_invoke_begin
	.uleb128	Lnull_invoke_inst2_begin-Lnull_invoke_begin
	.byte	0, 0
	.uleb128	Lnull_invoke_inst2_begin-Lnull_invoke_begin
	.uleb128	Lnull_invoke_inst2_end-Lnull_invoke_inst2_begin
	.uleb128	Lnull_invoke_block2-Lnull_invoke_begin
	.uleb128	1
	.uleb128	Lnull_invoke_inst2_end-Lnull_invoke_begin
	.uleb128	Lnull_invoke_end-Lnull_invoke_inst2_end
	.byte	0, 0
Lnull_invoke_cst_end:
	.byte	1, 0
	.p2align	2
	.long	0
Lnull_invoke_ttbase:
	.section	__TEXT,__text
	.subsections_via_symbols
