	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_k_printf
	.p2align	2
_k_printf:
Lk_printf_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v3=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	sub	sp, sp, #16
	mov	x9, x0
Lk_printf_block0:
	; v1 = stack_addr ss0
	add	x10, sp, #0
	; intrinsic va_start(v1)
	add	x16, x29, #16
	str	x16, [x10]
	; v2 = load.ptr v1
	ldr	x10, [sp, #0]
	; v3 = call @vprintf(v0, v2)
	mov	x0, x9
	mov	x1, x10
	bl	_vprintf
	mov	x9, x0
	; return v3
	mov	x0, x9
	mov	sp, x29
	ldp	x29, x30, [sp], #16
	ret
Lk_printf_end:
	.cfi_endproc
	.subsections_via_symbols
