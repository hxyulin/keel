	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_memory_forward
	.p2align	2
_memory_forward:
Lmemory_forward_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x12 v5=x10 v7=x9 v8=x10 v9=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
	mov	x10, x1
	mov	x11, x2
Lmemory_forward_block0:
	; v3 = load.i64 v0
	ldr	x12, [x9]
	; store v2, v1
	str	x11, [x10]
	; v5 = load.i64 v0
	ldr	x10, [x9]
	; store v5, v0
	str	x10, [x9]
	; v7 = iadd v3, v3
	add	x9, x12, x12
	; v8 = iadd v5, v5
	add	x10, x10, x10
	; v9 = iadd v7, v8
	add	x9, x9, x10
	; return v9
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmemory_forward_end:
	.cfi_endproc
	.subsections_via_symbols
