	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_catch
	.p2align	2
_k_catch:
Lk_catch_begin:
	; regalloc: v0=x19 v2=x10 v3=x9 v5=x9 v6=x9 v7=x20 v8=x9 v9=x10 v11=x20 v13=x9 v14=x9 v15=x9
	.cfi_startproc
	.cfi_personality 155, ___gxx_personality_v0
	.cfi_lsda 16, Lk_catch_lsda
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	mov	x19, x0
Lk_catch_block0:
	; v3 = invoke @c_may_throw(v0), block1, block2
	mov	x0, x19
Lk_catch_inst2_begin:
	bl	_c_may_throw
Lk_catch_inst2_end:
	mov	x9, x0
	b	Lk_catch_block1
Lk_catch_block2:
	; v6, v7 = landingpad catch @_ZTIi, catch null
	mov	x9, x0
	mov	x20, x1
	; v8 = call @__cxa_begin_catch(v6)
	mov	x0, x9
	bl	___cxa_begin_catch
	mov	x9, x0
	; v9 = eh_typeid @_ZTIi
	movz	w10, #0x1
	; v10 = icmp.eq v7, v9
	; brif v10, block3, block4
	cmp	w20, w10
	b.eq	Lk_catch_block3
Lk_catch_block4:
	; call @__cxa_end_catch()
	bl	___cxa_end_catch
	; v15 = iconst.i32 -1
	movn	x9, #0x0
	; return v15
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_catch_block3:
	; v11 = load.i32 v8
	ldr	w20, [x9]
	; call @__cxa_end_catch()
	bl	___cxa_end_catch
	; v12 = iconst.i32 1000
	; v13 = iadd v11, v12
	add	w9, w20, #1000
	; v1 = iconst.i32 7
	; v2 = iadd v0, v1
	add	w10, w19, #7
	; v14 = iadd v2, v13
	add	w9, w10, w9
	; return v14
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_catch_block1:
	; v4 = iconst.i32 1
	; v5 = iadd v3, v4
	add	w9, w9, #1
	; return v5
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_catch_end:
	.cfi_endproc
	.section	__TEXT,__gcc_except_tab
	.p2align	2
GCC_except_table_k_catch:
Lk_catch_lsda:
	.byte	255
	.byte	155
	.uleb128	Lk_catch_ttbase-Lk_catch_ttref
Lk_catch_ttref:
	.byte	1
	.uleb128	Lk_catch_cst_end-Lk_catch_cst_begin
Lk_catch_cst_begin:
	.uleb128	Lk_catch_begin-Lk_catch_begin
	.uleb128	Lk_catch_inst2_begin-Lk_catch_begin
	.byte	0, 0
	.uleb128	Lk_catch_inst2_begin-Lk_catch_begin
	.uleb128	Lk_catch_inst2_end-Lk_catch_inst2_begin
	.uleb128	Lk_catch_block2-Lk_catch_begin
	.uleb128	1
	.uleb128	Lk_catch_inst2_end-Lk_catch_begin
	.uleb128	Lk_catch_end-Lk_catch_inst2_end
	.byte	0, 0
Lk_catch_cst_end:
	.byte	1, 1, 2, 0
	.p2align	2
	.long	0
Lk_catch_tt0:
	.long	__ZTIi@GOT-Lk_catch_tt0
Lk_catch_ttbase:
	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_cleanup
	.p2align	2
_k_cleanup:
Lk_cleanup_begin:
	; regalloc: v0=x19 v2=x12 v3=x9 v4=x9 v5=x10 v6=x10 v7=x11 v8=x11
	.cfi_startproc
	.cfi_personality 155, ___gxx_personality_v0
	.cfi_lsda 16, Lk_cleanup_lsda
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	str	x19, [sp, #-16]!
	.cfi_offset x19, -32
	mov	x19, x0
Lk_cleanup_block0:
	; v3 = invoke @c_may_throw(v0), block1, block2
	mov	x0, x19
Lk_cleanup_inst2_begin:
	bl	_c_may_throw
Lk_cleanup_inst2_end:
	mov	x9, x0
	b	Lk_cleanup_block1
Lk_cleanup_block2:
	; v4, v5 = landingpad cleanup
	mov	x9, x0
	mov	x10, x1
	; v6 = symbol_addr @cleanups
	adrp	x10, _cleanups@PAGE
	add	x10, x10, _cleanups@PAGEOFF
	; v7 = load.i32 v6
	ldr	w11, [x10]
	; v1 = iconst.i32 100
	; v2 = iadd v0, v1
	add	w12, w19, #100
	; v8 = iadd v2, v7
	add	w11, w12, w11
	; store v8, v6
	str	w11, [x10]
	; call @_Unwind_Resume(v4)
	mov	x0, x9
	bl	__Unwind_Resume
	; trap
	brk	#1
Lk_cleanup_block1:
	; return v3
	mov	x0, x9
	sub	sp, x29, #16
	ldr	x19, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_cleanup_end:
	.cfi_endproc
	.section	__TEXT,__gcc_except_tab
	.p2align	2
GCC_except_table_k_cleanup:
Lk_cleanup_lsda:
	.byte	255
	.byte	255
	.byte	1
	.uleb128	Lk_cleanup_cst_end-Lk_cleanup_cst_begin
Lk_cleanup_cst_begin:
	.uleb128	Lk_cleanup_begin-Lk_cleanup_begin
	.uleb128	Lk_cleanup_inst2_begin-Lk_cleanup_begin
	.byte	0, 0
	.uleb128	Lk_cleanup_inst2_begin-Lk_cleanup_begin
	.uleb128	Lk_cleanup_inst2_end-Lk_cleanup_inst2_begin
	.uleb128	Lk_cleanup_block2-Lk_cleanup_begin
	.uleb128	0
	.uleb128	Lk_cleanup_inst2_end-Lk_cleanup_begin
	.uleb128	Lk_cleanup_end-Lk_cleanup_inst2_end
	.byte	0, 0
Lk_cleanup_cst_end:
	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_two
	.p2align	2
_k_two:
Lk_two_begin:
	; regalloc: v0=x9 v1=x19 v2=x20 v3=x21 v4=x19 v5=x9 v6=x10 v7=x9 v8=x9 v9=x10 v10=x9 v11=x9 v12=x9 v13=x10 v14=x9 v15=x9
	.cfi_startproc
	.cfi_personality 155, ___gxx_personality_v0
	.cfi_lsda 16, Lk_two_lsda
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	str	x21, [sp, #-16]!
	.cfi_offset x21, -48
	mov	x9, x0
	mov	x19, x1
	mov	x20, x2
Lk_two_block0:
	; v3 = invoke @c_may_throw(v0), block1, block3
	mov	x0, x9
Lk_two_inst0_begin:
	bl	_c_may_throw
Lk_two_inst0_end:
	mov	x21, x0
	b	Lk_two_block1
Lk_two_block3:
	; v8, v9 = landingpad catch null
	mov	x9, x0
	mov	x10, x1
	; v10 = call @__cxa_begin_catch(v8)
	mov	x0, x9
	bl	___cxa_begin_catch
	mov	x9, x0
	; call @__cxa_end_catch()
	bl	___cxa_end_catch
	; v11 = iconst.i32 100
	movz	x9, #0x64
	; return v11
	mov	x0, x9
	sub	sp, x29, #32
	ldr	x21, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_two_block1:
	; v4 = call @c_may_throw(v1)
	mov	x0, x19
	bl	_c_may_throw
	mov	x19, x0
	; v5 = invoke @c_may_throw(v2), block2, block4
	mov	x0, x20
Lk_two_inst2_begin:
	bl	_c_may_throw
Lk_two_inst2_end:
	mov	x9, x0
	b	Lk_two_block2
Lk_two_block4:
	; v12, v13 = landingpad catch null
	mov	x9, x0
	mov	x10, x1
	; v14 = call @__cxa_begin_catch(v12)
	mov	x0, x9
	bl	___cxa_begin_catch
	mov	x9, x0
	; call @__cxa_end_catch()
	bl	___cxa_end_catch
	; v15 = iconst.i32 200
	movz	x9, #0xc8
	; return v15
	mov	x0, x9
	sub	sp, x29, #32
	ldr	x21, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_two_block2:
	; v6 = iadd v3, v4
	add	w10, w21, w19
	; v7 = iadd v5, v6
	add	w9, w9, w10
	; return v7
	mov	x0, x9
	sub	sp, x29, #32
	ldr	x21, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_two_end:
	.cfi_endproc
	.section	__TEXT,__gcc_except_tab
	.p2align	2
GCC_except_table_k_two:
Lk_two_lsda:
	.byte	255
	.byte	155
	.uleb128	Lk_two_ttbase-Lk_two_ttref
Lk_two_ttref:
	.byte	1
	.uleb128	Lk_two_cst_end-Lk_two_cst_begin
Lk_two_cst_begin:
	.uleb128	Lk_two_begin-Lk_two_begin
	.uleb128	Lk_two_inst0_begin-Lk_two_begin
	.byte	0, 0
	.uleb128	Lk_two_inst0_begin-Lk_two_begin
	.uleb128	Lk_two_inst0_end-Lk_two_inst0_begin
	.uleb128	Lk_two_block3-Lk_two_begin
	.uleb128	1
	.uleb128	Lk_two_inst0_end-Lk_two_begin
	.uleb128	Lk_two_inst2_begin-Lk_two_inst0_end
	.byte	0, 0
	.uleb128	Lk_two_inst2_begin-Lk_two_begin
	.uleb128	Lk_two_inst2_end-Lk_two_inst2_begin
	.uleb128	Lk_two_block4-Lk_two_begin
	.uleb128	3
	.uleb128	Lk_two_inst2_end-Lk_two_begin
	.uleb128	Lk_two_end-Lk_two_inst2_end
	.byte	0, 0
Lk_two_cst_end:
	.byte	1, 0, 1, 0
	.p2align	2
	.long	0
Lk_two_ttbase:
	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_invoke_storage
	.p2align	2
_k_invoke_storage:
Lk_invoke_storage_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v4=x13 v5=x13 v6=x14 v7=x15 v8=x19 v9=x20 v10=x21 v11=x22 v12=x23 v13=x9 v14=x9 v15=x10 v16=x9 v17=x9 v22=x9 v23=x9
	.cfi_startproc
	.cfi_personality 155, ___gxx_personality_v0
	.cfi_lsda 16, Lk_invoke_storage_lsda
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	stp	x21, x22, [sp, #-16]!
	.cfi_offset x21, -48
	.cfi_offset x22, -40
	str	x23, [sp, #-16]!
	.cfi_offset x23, -64
	sub	sp, sp, #80
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lk_invoke_storage_block0:
	; v3 = stack_addr ss0
	add	x12, sp, #64
	; v4 = iconst.i64 1234
	movz	x13, #0x4d2
	; store v4, v3
	str	x13, [sp, #64]
	; v5 = iconst.i64 1
	movz	x13, #0x1
	; v6 = iconst.i64 2
	movz	x14, #0x2
	; v7 = iconst.i64 3
	movz	x15, #0x3
	; v8 = iconst.i64 4
	movz	x19, #0x4
	; v9 = iconst.i64 5
	movz	x20, #0x5
	; v10 = iconst.i64 6
	movz	x21, #0x6
	; v11 = iconst.i64 7
	movz	x22, #0x7
	; v12 = iconst.i64 8
	movz	x23, #0x8
	; invoke @c_invoke_storage(v0, v5, v6, v7, v8, v9, v10, v11, v12, v1, v2, v3), block1, block2
	str	x9, [sp, #56]
	ldr	x17, [x10, #0]
	str	x17, [sp, #32]
	ldr	x17, [x10, #8]
	str	x17, [sp, #40]
	ldr	x17, [x10, #16]
	str	x17, [sp, #48]
	add	x16, sp, #32
	str	x16, [sp, #0]
	str	w11, [sp, #8]
	str	x12, [sp, #16]
	mov	x0, x13
	mov	x1, x14
	mov	x2, x15
	mov	x3, x19
	mov	x4, x20
	mov	x5, x21
	mov	x6, x22
	mov	x7, x23
Lk_invoke_storage_inst11_begin:
	bl	_c_invoke_storage
Lk_invoke_storage_inst11_end:
	ldr	x16, [sp, #56]
	str	x0, [x16, #0]
	str	x1, [x16, #8]
	b	Lk_invoke_storage_block1
Lk_invoke_storage_block2:
	; v14, v15 = landingpad catch null
	mov	x9, x0
	mov	x10, x1
	; v16 = call @__cxa_begin_catch(v14)
	mov	x0, x9
	bl	___cxa_begin_catch
	mov	x9, x0
	; call @__cxa_end_catch()
	bl	___cxa_end_catch
	; v22 = stack_addr ss0
	; v17 = load.i64 v22
	ldr	x9, [sp, #64]
	; return v17
	mov	x0, x9
	sub	sp, x29, #48
	ldr	x23, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_invoke_storage_block1:
	; v23 = stack_addr ss0
	; v13 = load.i64 v23
	ldr	x9, [sp, #64]
	; return v13
	mov	x0, x9
	sub	sp, x29, #48
	ldr	x23, [sp], #16
	ldp	x21, x22, [sp], #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Lk_invoke_storage_end:
	.cfi_endproc
	.section	__TEXT,__gcc_except_tab
	.p2align	2
GCC_except_table_k_invoke_storage:
Lk_invoke_storage_lsda:
	.byte	255
	.byte	155
	.uleb128	Lk_invoke_storage_ttbase-Lk_invoke_storage_ttref
Lk_invoke_storage_ttref:
	.byte	1
	.uleb128	Lk_invoke_storage_cst_end-Lk_invoke_storage_cst_begin
Lk_invoke_storage_cst_begin:
	.uleb128	Lk_invoke_storage_begin-Lk_invoke_storage_begin
	.uleb128	Lk_invoke_storage_inst11_begin-Lk_invoke_storage_begin
	.byte	0, 0
	.uleb128	Lk_invoke_storage_inst11_begin-Lk_invoke_storage_begin
	.uleb128	Lk_invoke_storage_inst11_end-Lk_invoke_storage_inst11_begin
	.uleb128	Lk_invoke_storage_block2-Lk_invoke_storage_begin
	.uleb128	1
	.uleb128	Lk_invoke_storage_inst11_end-Lk_invoke_storage_begin
	.uleb128	Lk_invoke_storage_end-Lk_invoke_storage_inst11_end
	.byte	0, 0
Lk_invoke_storage_cst_end:
	.byte	1, 0
	.p2align	2
	.long	0
Lk_invoke_storage_ttbase:
	.section	__TEXT,__text
	.globl	_cleanups
	.zerofill	__DATA,__bss,_cleanups,4,2
	.subsections_via_symbols
