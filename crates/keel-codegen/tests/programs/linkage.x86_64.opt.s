	.intel_syntax noprefix
	.file	1 "linkage.keel"
	.section	".text","ax",@progbits
	.globl "k_linkage"
	.p2align	4
	.type	"k_linkage",@function
"k_linkage":
.Lk_linkage_begin:
	.loc	1 22 0
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	push	rbx
	.cfi_offset 3, -24
	sub	rsp, 8
	mov	rsi, rdi
.Lk_linkage_block0:
	# v13 = iconst.i64 1000
	.loc	1 22 5 prologue_end
	# v14 = iadd v0, v13
.Lk_linkage_ds0:
	.loc	1 15 5
	add	rsi, 1000
	# v2 = symbol_addr @counter
	lea	rdi, [rip + "counter"]
	# v3 = load.i64 v2
	mov	r8, qword ptr [rdi]
	# v4 = iconst.i64 1
	# v5 = iadd v3, v4
	lea	r9, [r8 + 1]
	# store v5, v2
	mov	qword ptr [rdi], r9
	# v6 = symbol_addr @tls_count
	mov	rdi, qword ptr fs:[0]
	lea	rdi, [rdi + "tls_count"@TPOFF]
	# v7 = load.i64 v6
	mov	r9, qword ptr [rdi]
	# v8 = iadd v7, v4
	lea	rbx, [r9 + 1]
	# store v8, v6
	mov	qword ptr [rdi], rbx
	# v9 = iadd v3, v14
	add	r8, rsi
	# v10 = iadd v7, v9
	add	r9, r8
	# return v10
	mov	rax, r9
	lea	rsp, [rbp + -8]
	pop	rbx
	pop	rbp
	ret
.Lk_linkage_end:
	.cfi_endproc
	.size "k_linkage", .-"k_linkage"
	.section	".data","wa",@progbits
	.type	"counter",@object
	.size "counter", 8
	.p2align	3
"counter":
	.quad 100
	.section	".tdata","waT",@progbits
	.type	"tls_count",@object
	.size "tls_count", 8
	.p2align	3
"tls_count":
	.quad 7
	.section	".debug_abbrev","",@progbits
.Ldebug_abbrev:
	.byte 1
	.byte 17
	.byte 1
	.byte 37
	.byte 8
	.byte 3
	.byte 8
	.byte 27
	.byte 8
	.byte 16
	.byte 23
	.byte 17
	.byte 1
	.byte 18
	.byte 6
	.byte 0
	.byte 0
	.byte 2
	.byte 46
	.byte 1
	.byte 3
	.byte 8
	.byte 17
	.byte 1
	.byte 18
	.byte 6
	.byte 64
	.byte 24
	.byte 39
	.byte 25
	.byte 63
	.byte 25
	.byte 0
	.byte 0
	.byte 3
	.byte 46
	.byte 1
	.byte 3
	.byte 8
	.byte 17
	.byte 1
	.byte 18
	.byte 6
	.byte 64
	.byte 24
	.byte 39
	.byte 25
	.byte 0
	.byte 0
	.byte 4
	.byte 46
	.byte 1
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 17
	.byte 1
	.byte 18
	.byte 6
	.byte 64
	.byte 24
	.byte 39
	.byte 25
	.byte 63
	.byte 25
	.byte 0
	.byte 0
	.byte 5
	.byte 46
	.byte 1
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 17
	.byte 1
	.byte 18
	.byte 6
	.byte 64
	.byte 24
	.byte 39
	.byte 25
	.byte 0
	.byte 0
	.byte 6
	.byte 5
	.byte 0
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 2
	.byte 24
	.byte 0
	.byte 0
	.byte 7
	.byte 52
	.byte 0
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 2
	.byte 24
	.byte 0
	.byte 0
	.byte 8
	.byte 52
	.byte 0
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 63
	.byte 25
	.byte 2
	.byte 24
	.byte 0
	.byte 0
	.byte 9
	.byte 52
	.byte 0
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 2
	.byte 24
	.byte 0
	.byte 0
	.byte 10
	.byte 36
	.byte 0
	.byte 3
	.byte 8
	.byte 62
	.byte 11
	.byte 11
	.byte 15
	.byte 0
	.byte 0
	.byte 11
	.byte 15
	.byte 0
	.byte 73
	.byte 19
	.byte 11
	.byte 11
	.byte 0
	.byte 0
	.byte 12
	.byte 15
	.byte 0
	.byte 11
	.byte 11
	.byte 0
	.byte 0
	.byte 13
	.byte 19
	.byte 1
	.byte 3
	.byte 8
	.byte 11
	.byte 15
	.byte 0
	.byte 0
	.byte 14
	.byte 19
	.byte 1
	.byte 11
	.byte 15
	.byte 0
	.byte 0
	.byte 15
	.byte 23
	.byte 1
	.byte 3
	.byte 8
	.byte 11
	.byte 15
	.byte 0
	.byte 0
	.byte 16
	.byte 23
	.byte 1
	.byte 11
	.byte 15
	.byte 0
	.byte 0
	.byte 17
	.byte 19
	.byte 0
	.byte 3
	.byte 8
	.byte 60
	.byte 25
	.byte 0
	.byte 0
	.byte 18
	.byte 23
	.byte 0
	.byte 3
	.byte 8
	.byte 60
	.byte 25
	.byte 0
	.byte 0
	.byte 19
	.byte 13
	.byte 0
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 56
	.byte 15
	.byte 0
	.byte 0
	.byte 20
	.byte 13
	.byte 0
	.byte 73
	.byte 19
	.byte 56
	.byte 15
	.byte 0
	.byte 0
	.byte 21
	.byte 13
	.byte 0
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 13
	.byte 15
	.byte 107
	.byte 15
	.byte 0
	.byte 0
	.byte 22
	.byte 1
	.byte 1
	.byte 73
	.byte 19
	.byte 0
	.byte 0
	.byte 23
	.byte 33
	.byte 0
	.byte 55
	.byte 15
	.byte 0
	.byte 0
	.byte 24
	.byte 33
	.byte 0
	.byte 0
	.byte 0
	.byte 25
	.byte 22
	.byte 0
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 0
	.byte 0
	.byte 26
	.byte 22
	.byte 0
	.byte 3
	.byte 8
	.byte 0
	.byte 0
	.byte 27
	.byte 21
	.byte 0
	.byte 0
	.byte 0
	.byte 28
	.byte 17
	.byte 1
	.byte 37
	.byte 8
	.byte 19
	.byte 5
	.byte 3
	.byte 8
	.byte 27
	.byte 8
	.byte 16
	.byte 23
	.byte 17
	.byte 1
	.byte 18
	.byte 6
	.byte 0
	.byte 0
	.byte 29
	.byte 11
	.byte 1
	.byte 17
	.byte 1
	.byte 18
	.byte 6
	.byte 0
	.byte 0
	.byte 30
	.byte 11
	.byte 1
	.byte 85
	.byte 23
	.byte 0
	.byte 0
	.byte 31
	.byte 38
	.byte 0
	.byte 73
	.byte 19
	.byte 0
	.byte 0
	.byte 32
	.byte 38
	.byte 0
	.byte 0
	.byte 0
	.byte 33
	.byte 53
	.byte 0
	.byte 73
	.byte 19
	.byte 0
	.byte 0
	.byte 34
	.byte 53
	.byte 0
	.byte 0
	.byte 0
	.byte 35
	.byte 4
	.byte 1
	.byte 3
	.byte 8
	.byte 73
	.byte 19
	.byte 11
	.byte 15
	.byte 0
	.byte 0
	.byte 36
	.byte 4
	.byte 1
	.byte 73
	.byte 19
	.byte 11
	.byte 15
	.byte 0
	.byte 0
	.byte 37
	.byte 40
	.byte 0
	.byte 3
	.byte 8
	.byte 28
	.byte 13
	.byte 0
	.byte 0
	.byte 38
	.byte 17
	.byte 1
	.byte 37
	.byte 8
	.byte 3
	.byte 8
	.byte 27
	.byte 8
	.byte 16
	.byte 23
	.byte 85
	.byte 23
	.byte 0
	.byte 0
	.byte 39
	.byte 17
	.byte 1
	.byte 37
	.byte 8
	.byte 19
	.byte 5
	.byte 3
	.byte 8
	.byte 27
	.byte 8
	.byte 16
	.byte 23
	.byte 85
	.byte 23
	.byte 0
	.byte 0
	.byte 0
	.section	".debug_info","",@progbits
.Ldebug_cu:
	.long ".Ldebug_info_end"-".Ldebug_info_start"
.Ldebug_info_start:
	.short 4
	.long ".Ldebug_abbrev"
	.byte 8
	.byte 1
	.asciz	"keel"
	.asciz	"linkage.keel"
	.asciz	"<cwd>"
	.long ".Ldebug_line"
	.Ldebug_unit_size = .Lk_linkage_end-.Lk_linkage_begin
	.quad ".Lk_linkage_begin"
	.long ".Ldebug_unit_size"
	.byte 2
	.asciz	"k_linkage"
	.quad ".Lk_linkage_begin"
	.long ".Lk_linkage_end"-".Lk_linkage_begin"
	.byte 1
	.byte 86
	.byte 29
	.quad ".Lk_linkage_ds0"
	.long ".Lk_linkage_end"-".Lk_linkage_ds0"
	.byte 0
	.byte 0
	.byte 0
.Ldebug_info_end:
	.section	".debug_line","",@progbits
.Ldebug_line:
	.section	".note.GNU-stack","",@progbits
