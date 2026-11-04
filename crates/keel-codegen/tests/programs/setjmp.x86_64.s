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
	sub	rsp, 408
	mov	rbx, rdi
.Lk_setjmp_block0:
	# v1 = iconst.i64 3
	# v2 = imul v0, v1
	imul	rax, rbx, 3
	mov	qword ptr [rbp + -448], rax
	# v3 = iconst.i64 5
	# v4 = imul v0, v3
	imul	rax, rbx, 5
	mov	qword ptr [rbp + -440], rax
	# v5 = iconst.i64 7
	# v6 = imul v0, v5
	imul	rax, rbx, 7
	mov	qword ptr [rbp + -432], rax
	# v7 = iconst.i64 11
	# v8 = imul v0, v7
	imul	rax, rbx, 11
	mov	qword ptr [rbp + -424], rax
	# v9 = iconst.i64 13
	# v10 = imul v0, v9
	imul	rax, rbx, 13
	mov	qword ptr [rbp + -416], rax
	# v11 = iconst.i64 17
	# v12 = imul v0, v11
	imul	rax, rbx, 17
	mov	qword ptr [rbp + -408], rax
	# v13 = iconst.i64 19
	# v14 = imul v0, v13
	imul	rax, rbx, 19
	mov	qword ptr [rbp + -400], rax
	# v15 = iconst.i64 23
	# v16 = imul v0, v15
	imul	rax, rbx, 23
	mov	qword ptr [rbp + -392], rax
	# v17 = iconst.i64 29
	# v18 = imul v0, v17
	imul	rax, rbx, 29
	mov	qword ptr [rbp + -384], rax
	# v19 = iconst.i64 31
	# v20 = imul v0, v19
	imul	rax, rbx, 31
	mov	qword ptr [rbp + -376], rax
	# v21 = iconst.i64 37
	# v22 = imul v0, v21
	imul	rax, rbx, 37
	mov	qword ptr [rbp + -368], rax
	# v23 = iconst.i64 41
	# v24 = imul v0, v23
	imul	rax, rbx, 41
	mov	qword ptr [rbp + -360], rax
	# v25 = iconst.i64 43
	# v26 = imul v0, v25
	imul	rax, rbx, 43
	mov	qword ptr [rbp + -352], rax
	# v27 = iconst.i64 47
	# v28 = imul v0, v27
	imul	rax, rbx, 47
	mov	qword ptr [rbp + -344], rax
	# v29 = stack_addr ss0
	lea	rax, [rbp + -240]
	mov	qword ptr [rbp + -336], rax
	# v30 = call @setjmp(v29)
	mov	rdi, qword ptr [rbp + -336]
	call	"setjmp"@PLT
	mov	rsi, rax
	# v31 = iconst.i32 0
	# v32 = icmp.ne v30, v31
	cmp	esi, 0
	setne	sil
	# brif v32, block2, block1
	test	sil, sil
	jne	".Lk_setjmp_block2"
.Lk_setjmp_block1:
	# v33 = iconst.i64 101
	# v34 = imul v0, v33
	imul	rax, rbx, 101
	mov	qword ptr [rbp + -328], rax
	# v35 = iconst.i64 103
	# v36 = imul v0, v35
	imul	rax, rbx, 103
	mov	qword ptr [rbp + -320], rax
	# v37 = iconst.i64 105
	# v38 = imul v0, v37
	imul	rax, rbx, 105
	mov	qword ptr [rbp + -312], rax
	# v39 = iconst.i64 107
	# v40 = imul v0, v39
	imul	rax, rbx, 107
	mov	qword ptr [rbp + -304], rax
	# v41 = iconst.i64 109
	# v42 = imul v0, v41
	imul	rax, rbx, 109
	mov	qword ptr [rbp + -296], rax
	# v43 = iconst.i64 111
	# v44 = imul v0, v43
	imul	rax, rbx, 111
	mov	qword ptr [rbp + -288], rax
	# v45 = iconst.i64 113
	# v46 = imul v0, v45
	imul	rax, rbx, 113
	mov	qword ptr [rbp + -280], rax
	# v47 = iconst.i64 115
	# v48 = imul v0, v47
	imul	rax, rbx, 115
	mov	qword ptr [rbp + -272], rax
	# v49 = iconst.i64 117
	# v50 = imul v0, v49
	imul	rax, rbx, 117
	mov	qword ptr [rbp + -264], rax
	# v51 = iconst.i64 119
	# v52 = imul v0, v51
	imul	rax, rbx, 119
	mov	qword ptr [rbp + -256], rax
	# v53 = iconst.i64 121
	# v54 = imul v0, v53
	imul	rax, rbx, 121
	mov	qword ptr [rbp + -248], rax
	# v55 = iconst.i64 123
	# v56 = imul v0, v55
	imul	r9, rbx, 123
	# v57 = iconst.i64 125
	# v58 = imul v0, v57
	imul	r14, rbx, 125
	# v59 = iconst.i64 127
	# v60 = imul v0, v59
	imul	r13, rbx, 127
	# v61 = iconst.i64 129
	# v62 = imul v0, v61
	imul	r12, rbx, 129
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
	mov	rax, qword ptr [rbp + -328]
	mov	rcx, qword ptr [rbp + -320]
	lea	r8, [rax + rcx * 1]
	# v70 = iadd v69, v38
	mov	rcx, qword ptr [rbp + -312]
	add	r8, rcx
	# v71 = iadd v70, v40
	mov	rcx, qword ptr [rbp + -304]
	add	r8, rcx
	# v72 = iadd v71, v42
	mov	rcx, qword ptr [rbp + -296]
	add	r8, rcx
	# v73 = iadd v72, v44
	mov	rcx, qword ptr [rbp + -288]
	add	r8, rcx
	# v74 = iadd v73, v46
	mov	rcx, qword ptr [rbp + -280]
	add	r8, rcx
	# v75 = iadd v74, v48
	mov	rcx, qword ptr [rbp + -272]
	add	r8, rcx
	# v76 = iadd v75, v50
	mov	rcx, qword ptr [rbp + -264]
	add	r8, rcx
	# v77 = iadd v76, v52
	mov	rcx, qword ptr [rbp + -256]
	add	r8, rcx
	# v78 = iadd v77, v54
	mov	rcx, qword ptr [rbp + -248]
	add	r8, rcx
	# v79 = iadd v78, v56
	add	r8, r9
	# v80 = iadd v79, v58
	add	r8, r14
	# v81 = iadd v80, v60
	add	r8, r13
	# v82 = iadd v81, v62
	add	r8, r12
	# v83 = iadd v82, v64
	add	r8, r15
	# v84 = iadd v83, v66
	add	rsi, r8
	# v85 = iadd v84, v68
	add	rsi, rdi
	# v86 = symbol_addr @sink
	lea	rdi, [rip + "sink"]
	# store v85, v86
	mov	qword ptr [rdi], rsi
	# v87 = iconst.i32 1
	mov	esi, 1
	# call @longjmp(v29, v87)
	mov	rdi, qword ptr [rbp + -336]
	call	"longjmp"@PLT
	# trap
	ud2
.Lk_setjmp_block2:
	# v88 = iadd v2, v4
	mov	rax, qword ptr [rbp + -448]
	mov	rcx, qword ptr [rbp + -440]
	lea	rsi, [rax + rcx * 1]
	# v89 = iadd v88, v6
	mov	rcx, qword ptr [rbp + -432]
	add	rsi, rcx
	# v90 = iadd v89, v8
	mov	rcx, qword ptr [rbp + -424]
	add	rsi, rcx
	# v91 = iadd v90, v10
	mov	rcx, qword ptr [rbp + -416]
	add	rsi, rcx
	# v92 = iadd v91, v12
	mov	rcx, qword ptr [rbp + -408]
	add	rsi, rcx
	# v93 = iadd v92, v14
	mov	rcx, qword ptr [rbp + -400]
	add	rsi, rcx
	# v94 = iadd v93, v16
	mov	rcx, qword ptr [rbp + -392]
	add	rsi, rcx
	# v95 = iadd v94, v18
	mov	rcx, qword ptr [rbp + -384]
	add	rsi, rcx
	# v96 = iadd v95, v20
	mov	rcx, qword ptr [rbp + -376]
	add	rsi, rcx
	# v97 = iadd v96, v22
	mov	rcx, qword ptr [rbp + -368]
	add	rsi, rcx
	# v98 = iadd v97, v24
	mov	rcx, qword ptr [rbp + -360]
	add	rsi, rcx
	# v99 = iadd v98, v26
	mov	rcx, qword ptr [rbp + -352]
	add	rsi, rcx
	# v100 = iadd v99, v28
	mov	rcx, qword ptr [rbp + -344]
	add	rsi, rcx
	# return v100
	mov	rax, rsi
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
	sub	rsp, 408
	mov	rbx, rdi
.Lk_setjmp_swapped_block0:
	# v1 = iconst.i64 3
	# v2 = imul v0, v1
	imul	rax, rbx, 3
	mov	qword ptr [rbp + -448], rax
	# v3 = iconst.i64 5
	# v4 = imul v0, v3
	imul	rax, rbx, 5
	mov	qword ptr [rbp + -440], rax
	# v5 = iconst.i64 7
	# v6 = imul v0, v5
	imul	rax, rbx, 7
	mov	qword ptr [rbp + -432], rax
	# v7 = iconst.i64 11
	# v8 = imul v0, v7
	imul	rax, rbx, 11
	mov	qword ptr [rbp + -424], rax
	# v9 = iconst.i64 13
	# v10 = imul v0, v9
	imul	rax, rbx, 13
	mov	qword ptr [rbp + -416], rax
	# v11 = iconst.i64 17
	# v12 = imul v0, v11
	imul	rax, rbx, 17
	mov	qword ptr [rbp + -408], rax
	# v13 = iconst.i64 19
	# v14 = imul v0, v13
	imul	rax, rbx, 19
	mov	qword ptr [rbp + -400], rax
	# v15 = iconst.i64 23
	# v16 = imul v0, v15
	imul	rax, rbx, 23
	mov	qword ptr [rbp + -392], rax
	# v17 = iconst.i64 29
	# v18 = imul v0, v17
	imul	rax, rbx, 29
	mov	qword ptr [rbp + -384], rax
	# v19 = iconst.i64 31
	# v20 = imul v0, v19
	imul	rax, rbx, 31
	mov	qword ptr [rbp + -376], rax
	# v21 = iconst.i64 37
	# v22 = imul v0, v21
	imul	rax, rbx, 37
	mov	qword ptr [rbp + -368], rax
	# v23 = iconst.i64 41
	# v24 = imul v0, v23
	imul	rax, rbx, 41
	mov	qword ptr [rbp + -360], rax
	# v25 = iconst.i64 43
	# v26 = imul v0, v25
	imul	rax, rbx, 43
	mov	qword ptr [rbp + -352], rax
	# v27 = iconst.i64 47
	# v28 = imul v0, v27
	imul	rax, rbx, 47
	mov	qword ptr [rbp + -344], rax
	# v29 = stack_addr ss0
	lea	rax, [rbp + -240]
	mov	qword ptr [rbp + -336], rax
	# v30 = call @setjmp(v29)
	mov	rdi, qword ptr [rbp + -336]
	call	"setjmp"@PLT
	mov	rsi, rax
	# v31 = iconst.i32 0
	# v32 = icmp.eq v30, v31
	cmp	esi, 0
	sete	sil
	# brif v32, block1, block2
	test	sil, sil
	jne	".Lk_setjmp_swapped_block1"
.Lk_setjmp_swapped_block2:
	# v88 = iadd v2, v4
	mov	rax, qword ptr [rbp + -448]
	mov	rcx, qword ptr [rbp + -440]
	lea	rsi, [rax + rcx * 1]
	# v89 = iadd v88, v6
	mov	rcx, qword ptr [rbp + -432]
	add	rsi, rcx
	# v90 = iadd v89, v8
	mov	rcx, qword ptr [rbp + -424]
	add	rsi, rcx
	# v91 = iadd v90, v10
	mov	rcx, qword ptr [rbp + -416]
	add	rsi, rcx
	# v92 = iadd v91, v12
	mov	rcx, qword ptr [rbp + -408]
	add	rsi, rcx
	# v93 = iadd v92, v14
	mov	rcx, qword ptr [rbp + -400]
	add	rsi, rcx
	# v94 = iadd v93, v16
	mov	rcx, qword ptr [rbp + -392]
	add	rsi, rcx
	# v95 = iadd v94, v18
	mov	rcx, qword ptr [rbp + -384]
	add	rsi, rcx
	# v96 = iadd v95, v20
	mov	rcx, qword ptr [rbp + -376]
	add	rsi, rcx
	# v97 = iadd v96, v22
	mov	rcx, qword ptr [rbp + -368]
	add	rsi, rcx
	# v98 = iadd v97, v24
	mov	rcx, qword ptr [rbp + -360]
	add	rsi, rcx
	# v99 = iadd v98, v26
	mov	rcx, qword ptr [rbp + -352]
	add	rsi, rcx
	# v100 = iadd v99, v28
	mov	rcx, qword ptr [rbp + -344]
	add	rsi, rcx
	# return v100
	mov	rax, rsi
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
	mov	qword ptr [rbp + -328], rax
	# v35 = iconst.i64 103
	# v36 = imul v0, v35
	imul	rax, rbx, 103
	mov	qword ptr [rbp + -320], rax
	# v37 = iconst.i64 105
	# v38 = imul v0, v37
	imul	rax, rbx, 105
	mov	qword ptr [rbp + -312], rax
	# v39 = iconst.i64 107
	# v40 = imul v0, v39
	imul	rax, rbx, 107
	mov	qword ptr [rbp + -304], rax
	# v41 = iconst.i64 109
	# v42 = imul v0, v41
	imul	rax, rbx, 109
	mov	qword ptr [rbp + -296], rax
	# v43 = iconst.i64 111
	# v44 = imul v0, v43
	imul	rax, rbx, 111
	mov	qword ptr [rbp + -288], rax
	# v45 = iconst.i64 113
	# v46 = imul v0, v45
	imul	rax, rbx, 113
	mov	qword ptr [rbp + -280], rax
	# v47 = iconst.i64 115
	# v48 = imul v0, v47
	imul	rax, rbx, 115
	mov	qword ptr [rbp + -272], rax
	# v49 = iconst.i64 117
	# v50 = imul v0, v49
	imul	rax, rbx, 117
	mov	qword ptr [rbp + -264], rax
	# v51 = iconst.i64 119
	# v52 = imul v0, v51
	imul	rax, rbx, 119
	mov	qword ptr [rbp + -256], rax
	# v53 = iconst.i64 121
	# v54 = imul v0, v53
	imul	rax, rbx, 121
	mov	qword ptr [rbp + -248], rax
	# v55 = iconst.i64 123
	# v56 = imul v0, v55
	imul	r9, rbx, 123
	# v57 = iconst.i64 125
	# v58 = imul v0, v57
	imul	r14, rbx, 125
	# v59 = iconst.i64 127
	# v60 = imul v0, v59
	imul	r13, rbx, 127
	# v61 = iconst.i64 129
	# v62 = imul v0, v61
	imul	r12, rbx, 129
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
	mov	rax, qword ptr [rbp + -328]
	mov	rcx, qword ptr [rbp + -320]
	lea	r8, [rax + rcx * 1]
	# v70 = iadd v69, v38
	mov	rcx, qword ptr [rbp + -312]
	add	r8, rcx
	# v71 = iadd v70, v40
	mov	rcx, qword ptr [rbp + -304]
	add	r8, rcx
	# v72 = iadd v71, v42
	mov	rcx, qword ptr [rbp + -296]
	add	r8, rcx
	# v73 = iadd v72, v44
	mov	rcx, qword ptr [rbp + -288]
	add	r8, rcx
	# v74 = iadd v73, v46
	mov	rcx, qword ptr [rbp + -280]
	add	r8, rcx
	# v75 = iadd v74, v48
	mov	rcx, qword ptr [rbp + -272]
	add	r8, rcx
	# v76 = iadd v75, v50
	mov	rcx, qword ptr [rbp + -264]
	add	r8, rcx
	# v77 = iadd v76, v52
	mov	rcx, qword ptr [rbp + -256]
	add	r8, rcx
	# v78 = iadd v77, v54
	mov	rcx, qword ptr [rbp + -248]
	add	r8, rcx
	# v79 = iadd v78, v56
	add	r8, r9
	# v80 = iadd v79, v58
	add	r8, r14
	# v81 = iadd v80, v60
	add	r8, r13
	# v82 = iadd v81, v62
	add	r8, r12
	# v83 = iadd v82, v64
	add	r8, r15
	# v84 = iadd v83, v66
	add	rsi, r8
	# v85 = iadd v84, v68
	add	rsi, rdi
	# v86 = symbol_addr @sink
	lea	rdi, [rip + "sink"]
	# store v85, v86
	mov	qword ptr [rdi], rsi
	# v87 = iconst.i32 1
	mov	esi, 1
	# call @longjmp(v29, v87)
	mov	rdi, qword ptr [rbp + -336]
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
