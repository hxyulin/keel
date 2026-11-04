	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_memory_flags
	.p2align	2
_memory_flags:
Lmemory_flags_begin:
	; regalloc: v0=x9 v1=x10 v2=x11 v3=x10 v4=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lmemory_flags_block0:
	; v1 = load.i64 volatile align 8 v0
	ldr	x10, [x9]
	; v2 = iconst.i64 1
	; v3 = iadd v1, v2
	add	x10, x10, #1
	; store volatile align 8 v3, v0
	str	x10, [x9]
	; v4 = load.i64 align 8 notrap v0
	ldr	x9, [x9]
	; return v4
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmemory_flags_end:
	.cfi_endproc
	.subsections_via_symbols
