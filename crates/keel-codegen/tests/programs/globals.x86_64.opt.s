	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "main"
	.p2align	4
	.type	"main",@function
"main":
.Lmain_begin:
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
.Lmain_block0:
	# v0 = iconst.i32 0
	xor	ebx, ebx
	# v1 = symbol_addr @greeting
	lea	r12, [rip + "greeting"]
	# v2 = call @puts(v1)
	mov	rdi, r12
	call	"puts"@PLT
	mov	rsi, rax
	# v3 = symbol_addr @tail
	lea	rsi, [rip + "tail"]
	# v4 = load.ptr v3
	mov	rsi, qword ptr [rsi]
	# v5 = call @puts(v4)
	mov	rdi, rsi
	call	"puts"@PLT
	mov	rsi, rax
	# v6 = symbol_addr @puts
	mov	rsi, qword ptr [rip + "puts"@GOTPCREL]
	# v7 = call_indirect v6(v1) : (ptr) -> i32 cc(c)
	mov	rdi, r12
	mov	r10, rsi
	call	r10
	mov	rsi, rax
	# v8 = symbol_addr @counter
	lea	rsi, [rip + "counter"]
	# v9 = load.i64 v8
	mov	rdi, qword ptr [rsi]
	# v10 = iconst.i64 2
	mov	eax, 2
	mov	qword ptr [rbp + -64], rax
	# v11 = iadd v9, v10
	add	rdi, 2
	# store v11, v8
	mov	qword ptr [rsi], rdi
	# v12 = call @c_read_counter()
	call	"c_read_counter"@PLT
	mov	rsi, rax
	# v13 = iconst.i64 42
	# v14 = icmp.ne v12, v13
	cmp	rsi, 42
	setne	sil
	# v15 = iconst.i32 3
	mov	edi, 3
	# v16 = select v14, v15, v0
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rbx, rax
	# v17 = symbol_addr @ops
	lea	r13, [rip + "ops"]
	# v18 = iconst.i64 8
	mov	eax, 8
	mov	qword ptr [rbp + -56], rax
	# v19 = ptradd v17, v18
	# v20 = load.ptr v19
	mov	rsi, qword ptr [r13 + 8]
	# v21 = iconst.i64 10
	mov	r15d, 10
	# v22 = iconst.i64 3
	mov	r14d, 3
	# v23 = call_indirect v20(v21, v22) : (i64, i64) -> i64 cc(c)
	mov	rdi, r15
	mov	r10, rsi
	mov	rsi, r14
	call	r10
	mov	rsi, rax
	# v24 = iconst.i64 7
	mov	r12d, 7
	# v25 = icmp.ne v23, v24
	cmp	rsi, 7
	setne	sil
	# v26 = iconst.i32 4
	mov	edi, 4
	# v27 = select v25, v26, v16
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rbx, rax
	# v28 = load.ptr v17
	mov	rsi, qword ptr [r13]
	# v29 = call_indirect v28(v21, v22) : (i64, i64) -> i64 cc(c)
	mov	rdi, r15
	mov	r10, rsi
	mov	rsi, r14
	call	r10
	mov	rsi, rax
	# v30 = iconst.i64 13
	# v31 = icmp.ne v29, v30
	cmp	rsi, 13
	setne	sil
	# v32 = iconst.i32 5
	mov	edi, 5
	# v33 = select v31, v32, v27
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rsi, rax
	# v34 = symbol_addr @c_counter
	mov	rdi, qword ptr [rip + "c_counter"@GOTPCREL]
	# v35 = load.i64 v34
	mov	r8, qword ptr [rdi]
	# v36 = icmp.ne v35, v24
	cmp	r8, 7
	setne	r8b
	# v37 = iconst.i32 6
	mov	r9d, 6
	# v38 = select v36, v37, v33
	test	r8b, r8b
	mov	rax, rsi
	cmovne	rax, r9
	mov	rbx, rax
	# store v18, v34
	mov	qword ptr [rdi], 8
	# v39 = call @c_read_c_counter()
	call	"c_read_c_counter"@PLT
	mov	rsi, rax
	# v40 = icmp.ne v39, v18
	cmp	rsi, 8
	setne	sil
	# v41 = iconst.i32 7
	mov	edi, 7
	# v42 = select v40, v41, v38
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rbx, rax
	# v43 = symbol_addr @c_double
	mov	rsi, qword ptr [rip + "c_double"@GOTPCREL]
	# v44 = iconst.i64 21
	mov	edi, 21
	# v45 = call_indirect v43(v44) : (i64) -> i64 cc(c)
	mov	r10, rsi
	call	r10
	mov	rsi, rax
	# v46 = icmp.ne v45, v13
	cmp	rsi, 42
	setne	sil
	# v47 = iconst.i32 8
	mov	edi, 8
	# v48 = select v46, v47, v42
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rbx, rax
	# v49 = symbol_addr @add
	lea	r14, [rip + "add"]
	# v50 = iconst.i64 40
	mov	r15d, 40
	# v51 = call @c_apply(v49, v50, v10)
	mov	rdi, r14
	mov	rsi, r15
	mov	rdx, qword ptr [rbp + -64]
	call	"c_apply"@PLT
	mov	rsi, rax
	# v52 = icmp.ne v51, v13
	cmp	rsi, 42
	setne	sil
	# v53 = iconst.i32 9
	mov	edi, 9
	# v54 = select v52, v53, v48
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rbx, rax
	# v55 = call @c_get_mul()
	call	"c_get_mul"@PLT
	mov	rsi, rax
	# v56 = iconst.i64 6
	mov	edi, 6
	# v57 = call_indirect v55(v56, v24) : (i64, i64) -> i64 cc(c)
	mov	r10, rsi
	mov	rsi, r12
	call	r10
	mov	rsi, rax
	# v58 = icmp.ne v57, v13
	cmp	rsi, 42
	setne	sil
	# v59 = iconst.i32 10
	mov	edi, 10
	# v60 = select v58, v59, v54
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rsi, rax
	# v61 = symbol_addr @scratch
	lea	rdi, [rip + "scratch"]
	# v62 = iconst.i64 56
	# v63 = ptradd v61, v62
	add	rdi, 56
	# v64 = load.i64 v63
	mov	r8, qword ptr [rdi]
	# v65 = iconst.i64 99
	# store v65, v63
	mov	qword ptr [rdi], 99
	# v69 = iconst.i32 11
	mov	edi, 11
	# v70 = select v64, v69, v60
	test	r8, r8
	mov	rax, rsi
	cmovne	rax, rdi
	mov	rbx, rax
	# v71 = call @c_check_mixed()
	call	"c_check_mixed"@PLT
	mov	rsi, rax
	# v72 = iconst.i32 1
	# v73 = icmp.ne v71, v72
	cmp	esi, 1
	setne	sil
	# v74 = iconst.i32 12
	mov	edi, 12
	# v75 = select v73, v74, v70
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rsi, rax
	# v76 = symbol_addr @mixed
	lea	rdi, [rip + "mixed"]
	# v77 = ptradd v76, v10
	# v78 = load.i16 v77
	movzx	r8d, word ptr [rdi + 2]
	# v79 = sext.i64 v78
	movsx	r8, r8w
	# v80 = iconst.i64 772
	# v81 = icmp.ne v79, v80
	cmp	r8, 772
	setne	r8b
	# v82 = iconst.i32 13
	mov	r9d, 13
	# v83 = select v81, v82, v75
	test	r8b, r8b
	mov	rax, rsi
	cmovne	rax, r9
	mov	rsi, rax
	# v84 = iconst.i64 4
	# v85 = ptradd v76, v84
	# v86 = load.i32 v85
	mov	edi, dword ptr [rdi + 4]
	# v87 = sext.i64 v86
	movsxd	rdi, edi
	# v88 = iconst.i64 -2
	# v89 = icmp.ne v87, v88
	cmp	rdi, -2
	setne	dil
	# v90 = iconst.i32 14
	mov	r8d, 14
	# v91 = select v89, v90, v83
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rbx, rax
	# v92 = symbol_addr @fp
	lea	r12, [rip + "fp"]
	# v93 = load.ptr v92
	mov	rsi, qword ptr [r12]
	# v94 = iconst.i64 50
	mov	edi, 50
	# v95 = call_indirect v93(v94, v18) : (i64, i64) -> i64 cc(c)
	mov	r10, rsi
	mov	rsi, qword ptr [rbp + -56]
	call	r10
	mov	rsi, rax
	# v96 = icmp.ne v95, v13
	cmp	rsi, 42
	setne	sil
	# v97 = iconst.i32 15
	mov	edi, 15
	# v98 = select v96, v97, v91
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rbx, rax
	# store v49, v92
	mov	qword ptr [r12], r14
	# v99 = load.ptr v92
	mov	rsi, qword ptr [r12]
	# v100 = call_indirect v99(v50, v10) : (i64, i64) -> i64 cc(c)
	mov	rdi, r15
	mov	r10, rsi
	mov	rsi, qword ptr [rbp + -64]
	call	r10
	mov	rsi, rax
	# v101 = icmp.ne v100, v13
	cmp	rsi, 42
	setne	sil
	# v102 = iconst.i32 16
	mov	edi, 16
	# v103 = select v101, v102, v98
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rbx, rax
	# v104 = symbol_addr @bump
	lea	rsi, [rip + "bump"]
	# call_indirect v104() : () cc(c)
	mov	r10, rsi
	call	r10
	# v105 = call @c_read_counter()
	call	"c_read_counter"@PLT
	mov	rsi, rax
	# v106 = iconst.i64 43
	# v107 = icmp.ne v105, v106
	cmp	rsi, 43
	setne	sil
	# v108 = iconst.i32 17
	mov	edi, 17
	# v109 = select v107, v108, v103
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rbx, rax
	# v110 = call @chain(v17)
	mov	rdi, r13
	call	"chain"
	mov	rsi, rax
	# v111 = iconst.i64 18
	# v112 = icmp.ne v110, v111
	cmp	rsi, 18
	setne	sil
	# v113 = iconst.i32 18
	mov	edi, 18
	# v114 = select v112, v113, v109
	test	sil, sil
	mov	rax, rbx
	cmovne	rax, rdi
	mov	rsi, rax
	# return v114
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".text","ax",@progbits
	.globl "chain"
	.p2align	4
	.type	"chain",@function
"chain":
.Lchain_begin:
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
	mov	rsi, rdi
.Lchain_block0:
	# v1 = iconst.i64 8
	# v2 = ptradd v0, v1
	# v3 = load.ptr v0
	mov	rbx, qword ptr [rsi]
	# v4 = load.ptr v2
	mov	r12, qword ptr [rsi + 8]
	# v6 = iconst.i64 1
	mov	esi, 1
	# v7 = iconst.i64 10
	mov	r13d, 10
	# v8 = iconst.i64 3
	mov	r14d, 3
	# v9 = call_indirect v3(v6, v7) : (i64, i64) -> i64 cc(c)
	mov	rdi, rsi
	mov	rsi, r13
	mov	r10, rbx
	call	r10
	mov	rsi, rax
	# v10 = call_indirect v4(v9, v8) : (i64, i64) -> i64 cc(c)
	mov	rdi, rsi
	mov	rsi, r14
	mov	r10, r12
	call	r10
	mov	rsi, rax
	# v11 = call_indirect v3(v10, v7) : (i64, i64) -> i64 cc(c)
	mov	rdi, rsi
	mov	rsi, r13
	mov	r10, rbx
	call	r10
	mov	rsi, rax
	# return v11
	mov	rax, rsi
	lea	rsp, [rbp + -32]
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lchain_end:
	.cfi_endproc
	.size "chain", .-"chain"
	.section	".text","ax",@progbits
	.globl "add"
	.p2align	4
	.type	"add",@function
"add":
.Ladd_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Ladd_block0:
	# v2 = iadd v0, v1
	add	rsi, rdi
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Ladd_end:
	.cfi_endproc
	.size "add", .-"add"
	.section	".text","ax",@progbits
	.globl "sub"
	.p2align	4
	.type	"sub",@function
"sub":
.Lsub_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
	mov	r11, rsi
	mov	rsi, rdi
	mov	rdi, r11
.Lsub_block0:
	# v2 = isub v0, v1
	sub	rsi, rdi
	# return v2
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lsub_end:
	.cfi_endproc
	.size "sub", .-"sub"
	.section	".text","ax",@progbits
	.globl "bump"
	.p2align	4
	.type	"bump",@function
"bump":
.Lbump_begin:
	.cfi_startproc
	push	rbp
	.cfi_def_cfa_offset 16
	.cfi_offset 6, -16
	mov	rbp, rsp
	.cfi_def_cfa_register 6
.Lbump_block0:
	# v0 = symbol_addr @counter
	lea	rsi, [rip + "counter"]
	# v1 = load.i64 v0
	mov	rdi, qword ptr [rsi]
	# v2 = iconst.i64 1
	# v3 = iadd v1, v2
	add	rdi, 1
	# store v3, v0
	mov	qword ptr [rsi], rdi
	# return
	mov	rsp, rbp
	pop	rbp
	ret
.Lbump_end:
	.cfi_endproc
	.size "bump", .-"bump"
	.section	".rodata","a",@progbits
	.globl "greeting"
	.type	"greeting",@object
	.size "greeting", 14
	.p2align	0
"greeting":
	.byte	72, 105, 32, 102, 114, 111, 109, 32, 100, 97, 116, 97, 33, 0
	.section	".data.rel.ro","wa",@progbits
	.globl "tail"
	.type	"tail",@object
	.size "tail", 8
	.p2align	3
"tail":
	.quad "greeting"+3
	.section	".data","wa",@progbits
	.globl "counter"
	.type	"counter",@object
	.size "counter", 8
	.p2align	3
"counter":
	.quad 40
	.section	".data.rel.ro","wa",@progbits
	.globl "ops"
	.type	"ops",@object
	.size "ops", 16
	.p2align	3
"ops":
	.quad "add"
	.quad "sub"
	.section	".bss","wa",@nobits
	.globl "scratch"
	.type	"scratch",@object
	.size "scratch", 64
	.p2align	3
"scratch":
	.space	64
	.section	".rodata","a",@progbits
	.globl "mixed"
	.type	"mixed",@object
	.size "mixed", 12
	.p2align	2
"mixed":
	.byte 1
	.byte 255
	.short 772
	.long 4294967294
	.space	4
	.section	".data","wa",@progbits
	.globl "fp"
	.type	"fp",@object
	.size "fp", 8
	.p2align	3
"fp":
	.quad "sub"
	.section	".note.GNU-stack","",@progbits
