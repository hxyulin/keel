	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "k_setjmp"
	.p2align	4
	.type	"k_setjmp",@function
"k_setjmp":
.Lk_setjmp_begin:
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
	sub	rsp, 296
	mov	rbx, rdi
.Lk_setjmp_block0:
	# v29 = stack_addr ss0
	lea	rsi, [rbp + -240]
	# v30 = call @setjmp(v29)
	mov	rdi, rsi
	call	"setjmp"@PLT
	mov	rsi, rax
	# v31 = iconst.i32 0
	# v32 = icmp.ne v30, v31
	# brif v32, block2, block1
	cmp	esi, 0
	jne	".Lk_setjmp_block2"
.Lk_setjmp_block1:
	# v33 = iconst.i64 101
	# v34 = imul v0, v33
	imul	rax, rbx, 101
	mov	qword ptr [rbp + -336], rax
	# v35 = iconst.i64 103
	# v36 = imul v0, v35
	imul	rax, rbx, 103
	mov	qword ptr [rbp + -328], rax
	# v37 = iconst.i64 105
	# v38 = imul v0, v37
	imul	rax, rbx, 105
	mov	qword ptr [rbp + -320], rax
	# v39 = iconst.i64 107
	# v40 = imul v0, v39
	imul	rax, rbx, 107
	mov	qword ptr [rbp + -312], rax
	# v41 = iconst.i64 109
	# v42 = imul v0, v41
	imul	rax, rbx, 109
	mov	qword ptr [rbp + -304], rax
	# v43 = iconst.i64 111
	# v44 = imul v0, v43
	imul	rax, rbx, 111
	mov	qword ptr [rbp + -296], rax
	# v45 = iconst.i64 113
	# v46 = imul v0, v45
	imul	rax, rbx, 113
	mov	qword ptr [rbp + -288], rax
	# v47 = iconst.i64 115
	# v48 = imul v0, v47
	imul	rax, rbx, 115
	mov	qword ptr [rbp + -280], rax
	# v49 = iconst.i64 117
	# v50 = imul v0, v49
	imul	rax, rbx, 117
	mov	qword ptr [rbp + -272], rax
	# v51 = iconst.i64 119
	# v52 = imul v0, v51
	imul	rax, rbx, 119
	mov	qword ptr [rbp + -264], rax
	# v53 = iconst.i64 121
	# v54 = imul v0, v53
	imul	rax, rbx, 121
	mov	qword ptr [rbp + -256], rax
	# v55 = iconst.i64 123
	# v56 = imul v0, v55
	imul	r9, rbx, 123
	# v57 = iconst.i64 125
	# v58 = imul v0, v57
	imul	r12, rbx, 125
	# v59 = iconst.i64 127
	# v60 = imul v0, v59
	imul	r13, rbx, 127
	# v61 = iconst.i64 129
	# v62 = imul v0, v61
	imul	r14, rbx, 129
	# v63 = iconst.i64 131
	# v64 = imul v0, v63
	imul	r15, rbx, 131
	# v65 = iconst.i64 133
	# v66 = imul v0, v65
	imul	rsi, rbx, 133
	# v67 = iconst.i64 135
	# v68 = imul v0, v67
	imul	rdi, rbx, 135
	# v69 = iadd v34, v36
	mov	rax, qword ptr [rbp + -336]
	mov	rcx, qword ptr [rbp + -328]
	lea	r8, [rax + rcx * 1]
	# v70 = iadd v38, v69
	mov	rax, qword ptr [rbp + -320]
	add	r8, rax
	# v71 = iadd v40, v70
	mov	rax, qword ptr [rbp + -312]
	add	r8, rax
	# v72 = iadd v42, v71
	mov	rax, qword ptr [rbp + -304]
	add	r8, rax
	# v73 = iadd v44, v72
	mov	rax, qword ptr [rbp + -296]
	add	r8, rax
	# v74 = iadd v46, v73
	mov	rax, qword ptr [rbp + -288]
	add	r8, rax
	# v75 = iadd v48, v74
	mov	rax, qword ptr [rbp + -280]
	add	r8, rax
	# v76 = iadd v50, v75
	mov	rax, qword ptr [rbp + -272]
	add	r8, rax
	# v77 = iadd v52, v76
	mov	rax, qword ptr [rbp + -264]
	add	r8, rax
	# v78 = iadd v54, v77
	mov	rax, qword ptr [rbp + -256]
	add	r8, rax
	# v79 = iadd v56, v78
	add	r9, r8
	# v80 = iadd v58, v79
	add	r12, r9
	# v81 = iadd v60, v80
	add	r13, r12
	# v82 = iadd v62, v81
	add	r14, r13
	# v83 = iadd v64, v82
	add	r15, r14
	# v84 = iadd v66, v83
	add	rsi, r15
	# v85 = iadd v68, v84
	add	rdi, rsi
	# v86 = symbol_addr @sink
	lea	rsi, [rip + "sink"]
	# store v85, v86
	mov	qword ptr [rsi], rdi
	# v87 = iconst.i32 1
	mov	esi, 1
	# v103 = stack_addr ss0
	lea	rdi, [rbp + -240]
	# call @longjmp(v103, v87)
	call	"longjmp"@PLT
	# trap
	ud2
.Lk_setjmp_block2:
	# v3 = iconst.i64 5
	# v4 = imul v0, v3
	imul	rsi, rbx, 5
	# v1 = iconst.i64 3
	# v2 = imul v0, v1
	imul	rdi, rbx, 3
	# v88 = iadd v2, v4
	add	rdi, rsi
	# v5 = iconst.i64 7
	# v6 = imul v0, v5
	imul	rsi, rbx, 7
	# v89 = iadd v6, v88
	add	rsi, rdi
	# v7 = iconst.i64 11
	# v8 = imul v0, v7
	imul	rdi, rbx, 11
	# v90 = iadd v8, v89
	add	rdi, rsi
	# v9 = iconst.i64 13
	# v10 = imul v0, v9
	imul	rsi, rbx, 13
	# v91 = iadd v10, v90
	add	rsi, rdi
	# v11 = iconst.i64 17
	# v12 = imul v0, v11
	imul	rdi, rbx, 17
	# v92 = iadd v12, v91
	add	rdi, rsi
	# v13 = iconst.i64 19
	# v14 = imul v0, v13
	imul	rsi, rbx, 19
	# v93 = iadd v14, v92
	add	rsi, rdi
	# v15 = iconst.i64 23
	# v16 = imul v0, v15
	imul	rdi, rbx, 23
	# v94 = iadd v16, v93
	add	rdi, rsi
	# v17 = iconst.i64 29
	# v18 = imul v0, v17
	imul	rsi, rbx, 29
	# v95 = iadd v18, v94
	add	rsi, rdi
	# v19 = iconst.i64 31
	# v20 = imul v0, v19
	imul	rdi, rbx, 31
	# v96 = iadd v20, v95
	add	rdi, rsi
	# v21 = iconst.i64 37
	# v22 = imul v0, v21
	imul	rsi, rbx, 37
	# v97 = iadd v22, v96
	add	rsi, rdi
	# v23 = iconst.i64 41
	# v24 = imul v0, v23
	imul	rdi, rbx, 41
	# v98 = iadd v24, v97
	add	rdi, rsi
	# v25 = iconst.i64 43
	# v26 = imul v0, v25
	imul	rsi, rbx, 43
	# v99 = iadd v26, v98
	add	rsi, rdi
	# v27 = iconst.i64 47
	# v28 = imul v0, v27
	imul	rdi, rbx, 47
	# v100 = iadd v28, v99
	add	rdi, rsi
	# return v100
	mov	rax, rdi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_setjmp_end:
	.cfi_endproc
	.size "k_setjmp", .-"k_setjmp"
	.section	".text","ax",@progbits
	.globl "k_setjmp_swapped"
	.p2align	4
	.type	"k_setjmp_swapped",@function
"k_setjmp_swapped":
.Lk_setjmp_swapped_begin:
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
	sub	rsp, 296
	mov	rbx, rdi
.Lk_setjmp_swapped_block0:
	# v29 = stack_addr ss0
	lea	rsi, [rbp + -240]
	# v30 = call @setjmp(v29)
	mov	rdi, rsi
	call	"setjmp"@PLT
	mov	rsi, rax
	# v31 = iconst.i32 0
	# v32 = icmp.eq v30, v31
	# brif v32, block1, block2
	cmp	esi, 0
	je	".Lk_setjmp_swapped_block1"
.Lk_setjmp_swapped_block2:
	# v3 = iconst.i64 5
	# v4 = imul v0, v3
	imul	rsi, rbx, 5
	# v1 = iconst.i64 3
	# v2 = imul v0, v1
	imul	rdi, rbx, 3
	# v88 = iadd v2, v4
	add	rdi, rsi
	# v5 = iconst.i64 7
	# v6 = imul v0, v5
	imul	rsi, rbx, 7
	# v89 = iadd v6, v88
	add	rsi, rdi
	# v7 = iconst.i64 11
	# v8 = imul v0, v7
	imul	rdi, rbx, 11
	# v90 = iadd v8, v89
	add	rdi, rsi
	# v9 = iconst.i64 13
	# v10 = imul v0, v9
	imul	rsi, rbx, 13
	# v91 = iadd v10, v90
	add	rsi, rdi
	# v11 = iconst.i64 17
	# v12 = imul v0, v11
	imul	rdi, rbx, 17
	# v92 = iadd v12, v91
	add	rdi, rsi
	# v13 = iconst.i64 19
	# v14 = imul v0, v13
	imul	rsi, rbx, 19
	# v93 = iadd v14, v92
	add	rsi, rdi
	# v15 = iconst.i64 23
	# v16 = imul v0, v15
	imul	rdi, rbx, 23
	# v94 = iadd v16, v93
	add	rdi, rsi
	# v17 = iconst.i64 29
	# v18 = imul v0, v17
	imul	rsi, rbx, 29
	# v95 = iadd v18, v94
	add	rsi, rdi
	# v19 = iconst.i64 31
	# v20 = imul v0, v19
	imul	rdi, rbx, 31
	# v96 = iadd v20, v95
	add	rdi, rsi
	# v21 = iconst.i64 37
	# v22 = imul v0, v21
	imul	rsi, rbx, 37
	# v97 = iadd v22, v96
	add	rsi, rdi
	# v23 = iconst.i64 41
	# v24 = imul v0, v23
	imul	rdi, rbx, 41
	# v98 = iadd v24, v97
	add	rdi, rsi
	# v25 = iconst.i64 43
	# v26 = imul v0, v25
	imul	rsi, rbx, 43
	# v99 = iadd v26, v98
	add	rsi, rdi
	# v27 = iconst.i64 47
	# v28 = imul v0, v27
	imul	rdi, rbx, 47
	# v100 = iadd v28, v99
	add	rdi, rsi
	# return v100
	mov	rax, rdi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lk_setjmp_swapped_block1:
	# v33 = iconst.i64 101
	# v34 = imul v0, v33
	imul	rax, rbx, 101
	mov	qword ptr [rbp + -336], rax
	# v35 = iconst.i64 103
	# v36 = imul v0, v35
	imul	rax, rbx, 103
	mov	qword ptr [rbp + -328], rax
	# v37 = iconst.i64 105
	# v38 = imul v0, v37
	imul	rax, rbx, 105
	mov	qword ptr [rbp + -320], rax
	# v39 = iconst.i64 107
	# v40 = imul v0, v39
	imul	rax, rbx, 107
	mov	qword ptr [rbp + -312], rax
	# v41 = iconst.i64 109
	# v42 = imul v0, v41
	imul	rax, rbx, 109
	mov	qword ptr [rbp + -304], rax
	# v43 = iconst.i64 111
	# v44 = imul v0, v43
	imul	rax, rbx, 111
	mov	qword ptr [rbp + -296], rax
	# v45 = iconst.i64 113
	# v46 = imul v0, v45
	imul	rax, rbx, 113
	mov	qword ptr [rbp + -288], rax
	# v47 = iconst.i64 115
	# v48 = imul v0, v47
	imul	rax, rbx, 115
	mov	qword ptr [rbp + -280], rax
	# v49 = iconst.i64 117
	# v50 = imul v0, v49
	imul	rax, rbx, 117
	mov	qword ptr [rbp + -272], rax
	# v51 = iconst.i64 119
	# v52 = imul v0, v51
	imul	rax, rbx, 119
	mov	qword ptr [rbp + -264], rax
	# v53 = iconst.i64 121
	# v54 = imul v0, v53
	imul	rax, rbx, 121
	mov	qword ptr [rbp + -256], rax
	# v55 = iconst.i64 123
	# v56 = imul v0, v55
	imul	r9, rbx, 123
	# v57 = iconst.i64 125
	# v58 = imul v0, v57
	imul	r12, rbx, 125
	# v59 = iconst.i64 127
	# v60 = imul v0, v59
	imul	r13, rbx, 127
	# v61 = iconst.i64 129
	# v62 = imul v0, v61
	imul	r14, rbx, 129
	# v63 = iconst.i64 131
	# v64 = imul v0, v63
	imul	r15, rbx, 131
	# v65 = iconst.i64 133
	# v66 = imul v0, v65
	imul	rsi, rbx, 133
	# v67 = iconst.i64 135
	# v68 = imul v0, v67
	imul	rdi, rbx, 135
	# v69 = iadd v34, v36
	mov	rax, qword ptr [rbp + -336]
	mov	rcx, qword ptr [rbp + -328]
	lea	r8, [rax + rcx * 1]
	# v70 = iadd v38, v69
	mov	rax, qword ptr [rbp + -320]
	add	r8, rax
	# v71 = iadd v40, v70
	mov	rax, qword ptr [rbp + -312]
	add	r8, rax
	# v72 = iadd v42, v71
	mov	rax, qword ptr [rbp + -304]
	add	r8, rax
	# v73 = iadd v44, v72
	mov	rax, qword ptr [rbp + -296]
	add	r8, rax
	# v74 = iadd v46, v73
	mov	rax, qword ptr [rbp + -288]
	add	r8, rax
	# v75 = iadd v48, v74
	mov	rax, qword ptr [rbp + -280]
	add	r8, rax
	# v76 = iadd v50, v75
	mov	rax, qword ptr [rbp + -272]
	add	r8, rax
	# v77 = iadd v52, v76
	mov	rax, qword ptr [rbp + -264]
	add	r8, rax
	# v78 = iadd v54, v77
	mov	rax, qword ptr [rbp + -256]
	add	r8, rax
	# v79 = iadd v56, v78
	add	r9, r8
	# v80 = iadd v58, v79
	add	r12, r9
	# v81 = iadd v60, v80
	add	r13, r12
	# v82 = iadd v62, v81
	add	r14, r13
	# v83 = iadd v64, v82
	add	r15, r14
	# v84 = iadd v66, v83
	add	rsi, r15
	# v85 = iadd v68, v84
	add	rdi, rsi
	# v86 = symbol_addr @sink
	lea	rsi, [rip + "sink"]
	# store v85, v86
	mov	qword ptr [rsi], rdi
	# v87 = iconst.i32 1
	mov	esi, 1
	# v103 = stack_addr ss0
	lea	rdi, [rbp + -240]
	# call @longjmp(v103, v87)
	call	"longjmp"@PLT
	# trap
	ud2
.Lk_setjmp_swapped_end:
	.cfi_endproc
	.size "k_setjmp_swapped", .-"k_setjmp_swapped"
	.section	".bss","wa",@nobits
	.globl "sink"
	.type	"sink",@object
	.size "sink", 8
	.p2align	3
"sink":
	.space	8
	.section	".note.GNU-stack","",@progbits
