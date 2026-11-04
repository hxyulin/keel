	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_tree_iadd32
	.p2align	2
_tree_iadd32:
Ltree_iadd32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_iadd32_block0:
	; v2 = iconst.i32 34
	; v3 = shl v1, v2
	lsl	w10, w10, #2
	; v4 = iadd v0, v3
	add	w9, w9, w10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_iadd32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_isub32
	.p2align	2
_tree_isub32:
Ltree_isub32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_isub32_block0:
	; v2 = iconst.i32 34
	; v3 = shl v1, v2
	lsl	w10, w10, #2
	; v4 = isub v0, v3
	sub	w9, w9, w10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_isub32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_and32
	.p2align	2
_tree_and32:
Ltree_and32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_and32_block0:
	; v2 = iconst.i32 34
	; v3 = shl v1, v2
	lsl	w10, w10, #2
	; v4 = and v0, v3
	and	w9, w9, w10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_and32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_or32
	.p2align	2
_tree_or32:
Ltree_or32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_or32_block0:
	; v2 = iconst.i32 34
	; v3 = shl v1, v2
	lsl	w10, w10, #2
	; v4 = or v0, v3
	orr	w9, w9, w10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_or32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_xor32
	.p2align	2
_tree_xor32:
Ltree_xor32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_xor32_block0:
	; v2 = iconst.i32 34
	; v3 = shl v1, v2
	lsl	w10, w10, #2
	; v4 = xor v0, v3
	eor	w9, w9, w10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_xor32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_muladd32
	.p2align	2
_tree_muladd32:
Ltree_muladd32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Ltree_muladd32_block0:
	; v3 = imul v0, v1
	mul	w9, w9, w10
	; v4 = iadd v3, v2
	add	w9, w9, w11
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_muladd32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_mulsub32
	.p2align	2
_tree_mulsub32:
Ltree_mulsub32_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Ltree_mulsub32_block0:
	; v3 = imul v0, v1
	mul	w9, w9, w10
	; v4 = isub v2, v3
	sub	w9, w11, w9
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_mulsub32_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_iadd64
	.p2align	2
_tree_iadd64:
Ltree_iadd64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_iadd64_block0:
	; v2 = iconst.i64 66
	; v3 = shl v1, v2
	lsl	x10, x10, #2
	; v4 = iadd v0, v3
	add	x9, x9, x10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_iadd64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_isub64
	.p2align	2
_tree_isub64:
Ltree_isub64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_isub64_block0:
	; v2 = iconst.i64 66
	; v3 = shl v1, v2
	lsl	x10, x10, #2
	; v4 = isub v0, v3
	sub	x9, x9, x10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_isub64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_and64
	.p2align	2
_tree_and64:
Ltree_and64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_and64_block0:
	; v2 = iconst.i64 66
	; v3 = shl v1, v2
	lsl	x10, x10, #2
	; v4 = and v0, v3
	and	x9, x9, x10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_and64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_or64
	.p2align	2
_tree_or64:
Ltree_or64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_or64_block0:
	; v2 = iconst.i64 66
	; v3 = shl v1, v2
	lsl	x10, x10, #2
	; v4 = or v0, v3
	orr	x9, x9, x10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_or64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_xor64
	.p2align	2
_tree_xor64:
Ltree_xor64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_xor64_block0:
	; v2 = iconst.i64 66
	; v3 = shl v1, v2
	lsl	x10, x10, #2
	; v4 = xor v0, v3
	eor	x9, x9, x10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_xor64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_muladd64
	.p2align	2
_tree_muladd64:
Ltree_muladd64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Ltree_muladd64_block0:
	; v3 = imul v0, v1
	mul	x9, x9, x10
	; v4 = iadd v3, v2
	add	x9, x9, x11
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_muladd64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_mulsub64
	.p2align	2
_tree_mulsub64:
Ltree_mulsub64_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Ltree_mulsub64_block0:
	; v3 = imul v0, v1
	mul	x9, x9, x10
	; v4 = isub v2, v3
	sub	x9, x11, x9
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_mulsub64_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_across_call
	.p2align	2
_tree_across_call:
Ltree_across_call_begin:
	; regalloc: v0=x9 v1=x10 v2=x19 v3=x20 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	stp	x19, x20, [sp, #-16]!
	.cfi_offset x19, -32
	.cfi_offset x20, -24
	mov	x9, x0
	mov	x10, x1
	mov	x19, x2
Ltree_across_call_block0:
	; v3 = imul v0, v1
	mul	x20, x9, x10
	; call @selection_clobber()
	bl	_selection_clobber
	; v4 = iadd v3, v2
	add	x9, x20, x19
	; return v4
	mov	x0, x9
	sub	sp, x29, #16
	ldp	x19, x20, [sp], #16
	ldp	x29, x30, [sp], #16
	ret
Ltree_across_call_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_shared
	.p2align	2
_tree_shared:
Ltree_shared_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x10 v5=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Ltree_shared_block0:
	; v3 = imul v0, v1
	mul	x9, x9, x10
	; v4 = iadd v3, v2
	add	x10, x9, x11
	; v5 = xor v4, v3
	eor	x9, x10, x9
	; return v5
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_shared_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_tree_constant
	.p2align	2
_tree_constant:
Ltree_constant_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x9 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
Ltree_constant_block0:
	; v2 = iconst.i64 7
	movz	x11, #0x7
	; v3 = imul v0, v2
	mul	x9, x9, x11
	; v4 = iadd v3, v1
	add	x9, x9, x10
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Ltree_constant_end:
	.cfi_endproc
	.subsections_via_symbols
