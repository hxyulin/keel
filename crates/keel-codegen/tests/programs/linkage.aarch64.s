	.section	__TEXT,__text
	.file	1 "linkage.keel"
	.section	__TEXT,__text
	.p2align	2
_helper:
Lhelper_begin:
	; regalloc: v0=x9 v1=x10 v2=x9
	.loc	1 15 0
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lhelper_block0:
	; v1 = iconst.i64 1000
	; v2 = iadd v0, v1
	.loc	1 15 5 prologue_end
	add	x9, x9, #1000
	; return v2
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lhelper_end:
	.cfi_endproc
	.section	__TEXT,__text
	.globl	_k_linkage
	.p2align	2
_k_linkage:
Lk_linkage_begin:
	; regalloc: v0=x9 v1=x9 v2=x10 v3=x11 v4=x12 v5=x13 v6=x10 v7=x13 v8=x12 v9=x9 v10=x9
	.loc	1 22 0
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
	mov	x9, x0
Lk_linkage_block0:
	; v1 = call @helper(v0)
	.loc	1 22 5 prologue_end
	mov	x0, x9
	bl	_helper
	mov	x9, x0
	; v2 = symbol_addr @counter
	adrp	x10, _counter@PAGE
	add	x10, x10, _counter@PAGEOFF
	; v3 = load.i64 v2
	ldr	x11, [x10]
	; v4 = iconst.i64 1
	; v5 = iadd v3, v4
	add	x13, x11, #1
	; store v5, v2
	str	x13, [x10]
	; v6 = symbol_addr @tls_count
	adrp	x0, _tls_count@TLVPPAGE
	ldr	x0, [x0, _tls_count@TLVPPAGEOFF]
	ldr	x16, [x0]
	blr	x16
	mov	x10, x0
	; v7 = load.i64 v6
	ldr	x13, [x10]
	; v8 = iadd v7, v4
	add	x12, x13, #1
	; store v8, v6
	str	x12, [x10]
	; v9 = iadd v1, v3
	add	x9, x9, x11
	; v10 = iadd v9, v7
	add	x9, x9, x13
	; return v10
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lk_linkage_end:
	.cfi_endproc
	.section	__DATA,__data
	.p2align	3
_counter:
	.quad	100
	.section	__DATA,__thread_data,thread_local_regular
	.p2align	3
_tls_count$tlv$init:
	.quad	7
	.section	__DATA,__thread_vars,thread_local_variables
	.p2align	3
_tls_count:
	.quad	__tlv_bootstrap
	.quad	0
	.quad	_tls_count$tlv$init
	.section	__DWARF,__debug_abbrev,regular,debug
	.byte	1
	.byte	17
	.byte	1
	.byte	37
	.byte	8
	.byte	3
	.byte	8
	.byte	27
	.byte	8
	.byte	16
	.byte	23
	.byte	17
	.byte	1
	.byte	18
	.byte	6
	.byte	0
	.byte	0
	.byte	2
	.byte	46
	.byte	1
	.byte	3
	.byte	8
	.byte	17
	.byte	1
	.byte	18
	.byte	6
	.byte	64
	.byte	24
	.byte	39
	.byte	25
	.byte	63
	.byte	25
	.byte	0
	.byte	0
	.byte	3
	.byte	46
	.byte	1
	.byte	3
	.byte	8
	.byte	17
	.byte	1
	.byte	18
	.byte	6
	.byte	64
	.byte	24
	.byte	39
	.byte	25
	.byte	0
	.byte	0
	.byte	4
	.byte	46
	.byte	1
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	17
	.byte	1
	.byte	18
	.byte	6
	.byte	64
	.byte	24
	.byte	39
	.byte	25
	.byte	63
	.byte	25
	.byte	0
	.byte	0
	.byte	5
	.byte	46
	.byte	1
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	17
	.byte	1
	.byte	18
	.byte	6
	.byte	64
	.byte	24
	.byte	39
	.byte	25
	.byte	0
	.byte	0
	.byte	6
	.byte	5
	.byte	0
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	2
	.byte	24
	.byte	0
	.byte	0
	.byte	7
	.byte	52
	.byte	0
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	2
	.byte	24
	.byte	0
	.byte	0
	.byte	8
	.byte	52
	.byte	0
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	63
	.byte	25
	.byte	2
	.byte	24
	.byte	0
	.byte	0
	.byte	9
	.byte	52
	.byte	0
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	2
	.byte	24
	.byte	0
	.byte	0
	.byte	10
	.byte	36
	.byte	0
	.byte	3
	.byte	8
	.byte	62
	.byte	11
	.byte	11
	.byte	15
	.byte	0
	.byte	0
	.byte	11
	.byte	15
	.byte	0
	.byte	73
	.byte	19
	.byte	11
	.byte	11
	.byte	0
	.byte	0
	.byte	12
	.byte	15
	.byte	0
	.byte	11
	.byte	11
	.byte	0
	.byte	0
	.byte	13
	.byte	19
	.byte	1
	.byte	3
	.byte	8
	.byte	11
	.byte	15
	.byte	0
	.byte	0
	.byte	14
	.byte	19
	.byte	1
	.byte	11
	.byte	15
	.byte	0
	.byte	0
	.byte	15
	.byte	23
	.byte	1
	.byte	3
	.byte	8
	.byte	11
	.byte	15
	.byte	0
	.byte	0
	.byte	16
	.byte	23
	.byte	1
	.byte	11
	.byte	15
	.byte	0
	.byte	0
	.byte	17
	.byte	19
	.byte	0
	.byte	3
	.byte	8
	.byte	60
	.byte	25
	.byte	0
	.byte	0
	.byte	18
	.byte	23
	.byte	0
	.byte	3
	.byte	8
	.byte	60
	.byte	25
	.byte	0
	.byte	0
	.byte	19
	.byte	13
	.byte	0
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	56
	.byte	15
	.byte	0
	.byte	0
	.byte	20
	.byte	13
	.byte	0
	.byte	73
	.byte	19
	.byte	56
	.byte	15
	.byte	0
	.byte	0
	.byte	21
	.byte	13
	.byte	0
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	13
	.byte	15
	.byte	107
	.byte	15
	.byte	0
	.byte	0
	.byte	22
	.byte	1
	.byte	1
	.byte	73
	.byte	19
	.byte	0
	.byte	0
	.byte	23
	.byte	33
	.byte	0
	.byte	55
	.byte	15
	.byte	0
	.byte	0
	.byte	24
	.byte	33
	.byte	0
	.byte	0
	.byte	0
	.byte	25
	.byte	22
	.byte	0
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	0
	.byte	0
	.byte	26
	.byte	22
	.byte	0
	.byte	3
	.byte	8
	.byte	0
	.byte	0
	.byte	27
	.byte	21
	.byte	0
	.byte	0
	.byte	0
	.byte	28
	.byte	17
	.byte	1
	.byte	37
	.byte	8
	.byte	19
	.byte	5
	.byte	3
	.byte	8
	.byte	27
	.byte	8
	.byte	16
	.byte	23
	.byte	17
	.byte	1
	.byte	18
	.byte	6
	.byte	0
	.byte	0
	.byte	29
	.byte	11
	.byte	1
	.byte	17
	.byte	1
	.byte	18
	.byte	6
	.byte	0
	.byte	0
	.byte	30
	.byte	11
	.byte	1
	.byte	85
	.byte	23
	.byte	0
	.byte	0
	.byte	31
	.byte	38
	.byte	0
	.byte	73
	.byte	19
	.byte	0
	.byte	0
	.byte	32
	.byte	38
	.byte	0
	.byte	0
	.byte	0
	.byte	33
	.byte	53
	.byte	0
	.byte	73
	.byte	19
	.byte	0
	.byte	0
	.byte	34
	.byte	53
	.byte	0
	.byte	0
	.byte	0
	.byte	35
	.byte	4
	.byte	1
	.byte	3
	.byte	8
	.byte	73
	.byte	19
	.byte	11
	.byte	15
	.byte	0
	.byte	0
	.byte	36
	.byte	4
	.byte	1
	.byte	73
	.byte	19
	.byte	11
	.byte	15
	.byte	0
	.byte	0
	.byte	37
	.byte	40
	.byte	0
	.byte	3
	.byte	8
	.byte	28
	.byte	13
	.byte	0
	.byte	0
	.byte	38
	.byte	17
	.byte	1
	.byte	37
	.byte	8
	.byte	3
	.byte	8
	.byte	27
	.byte	8
	.byte	16
	.byte	23
	.byte	85
	.byte	23
	.byte	0
	.byte	0
	.byte	39
	.byte	17
	.byte	1
	.byte	37
	.byte	8
	.byte	19
	.byte	5
	.byte	3
	.byte	8
	.byte	27
	.byte	8
	.byte	16
	.byte	23
	.byte	85
	.byte	23
	.byte	0
	.byte	0
	.byte	0
	.section	__DWARF,__debug_info,regular,debug
Ldebug_cu:
	.long	Ldebug_info_end-Ldebug_info_start
Ldebug_info_start:
	.short	4
	.long	0
	.byte	8
	.byte	1
	.asciz	"keel"
	.asciz	"linkage.keel"
	.asciz	"<cwd>"
	.long	0
	Ldebug_unit_size = Lk_linkage_end-Lhelper_begin
	.quad	Lhelper_begin
	.long	Ldebug_unit_size
	.byte	3
	.asciz	"helper"
	.quad	Lhelper_begin
	.long	Lhelper_end-Lhelper_begin
	.byte	1
	.byte	109
	.byte	0
	.byte	2
	.asciz	"k_linkage"
	.quad	Lk_linkage_begin
	.long	Lk_linkage_end-Lk_linkage_begin
	.byte	1
	.byte	109
	.byte	0
	.byte	0
Ldebug_info_end:
	.subsections_via_symbols
