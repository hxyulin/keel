	.intel_syntax noprefix
	.section	".text","ax",@progbits
	.globl "pressure"
	.p2align	4
	.type	"pressure",@function
"pressure":
.Lpressure_begin:
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
	sub	rsp, 136
	mov	rsi, rdi
.Lpressure_block0:
	# v1 = iconst.i64 1
	# v2 = iadd v0, v1
	lea	rax, [rsi + 1]
	mov	qword ptr [rbp + -176], rax
	# v3 = iconst.i64 2
	# v4 = iadd v0, v3
	lea	rax, [rsi + 2]
	mov	qword ptr [rbp + -168], rax
	# v5 = iconst.i64 5
	# v6 = iadd v0, v5
	lea	rax, [rsi + 5]
	mov	qword ptr [rbp + -160], rax
	# v7 = iconst.i64 10
	# v8 = iadd v0, v7
	lea	rax, [rsi + 10]
	mov	qword ptr [rbp + -152], rax
	# v9 = iconst.i64 17
	# v10 = iadd v0, v9
	lea	rax, [rsi + 17]
	mov	qword ptr [rbp + -144], rax
	# v11 = iconst.i64 26
	# v12 = iadd v0, v11
	lea	rax, [rsi + 26]
	mov	qword ptr [rbp + -136], rax
	# v13 = iconst.i64 37
	# v14 = iadd v0, v13
	lea	rax, [rsi + 37]
	mov	qword ptr [rbp + -128], rax
	# v15 = iconst.i64 50
	# v16 = iadd v0, v15
	lea	rax, [rsi + 50]
	mov	qword ptr [rbp + -120], rax
	# v17 = iconst.i64 65
	# v18 = iadd v0, v17
	lea	rax, [rsi + 65]
	mov	qword ptr [rbp + -112], rax
	# v19 = iconst.i64 82
	# v20 = iadd v0, v19
	lea	rax, [rsi + 82]
	mov	qword ptr [rbp + -104], rax
	# v21 = iconst.i64 101
	# v22 = iadd v0, v21
	lea	rax, [rsi + 101]
	mov	qword ptr [rbp + -96], rax
	# v23 = iconst.i64 122
	# v24 = iadd v0, v23
	lea	rax, [rsi + 122]
	mov	qword ptr [rbp + -88], rax
	# v25 = iconst.i64 145
	# v26 = iadd v0, v25
	lea	rax, [rsi + 145]
	mov	qword ptr [rbp + -80], rax
	# v27 = iconst.i64 170
	# v28 = iadd v0, v27
	lea	rax, [rsi + 170]
	mov	qword ptr [rbp + -72], rax
	# v29 = iconst.i64 197
	# v30 = iadd v0, v29
	lea	rax, [rsi + 197]
	mov	qword ptr [rbp + -64], rax
	# v31 = iconst.i64 226
	# v32 = iadd v0, v31
	lea	rax, [rsi + 226]
	mov	qword ptr [rbp + -56], rax
	# v33 = iconst.i64 257
	# v34 = iadd v0, v33
	lea	rax, [rsi + 257]
	mov	qword ptr [rbp + -48], rax
	# v35 = iconst.i64 290
	# v36 = iadd v0, v35
	lea	r8, [rsi + 290]
	# v37 = iconst.i64 325
	# v38 = iadd v0, v37
	lea	r9, [rsi + 325]
	# v39 = iconst.i64 362
	# v40 = iadd v0, v39
	lea	rbx, [rsi + 362]
	# v41 = iconst.i64 401
	# v42 = iadd v0, v41
	lea	r12, [rsi + 401]
	# v43 = iconst.i64 442
	# v44 = iadd v0, v43
	lea	r13, [rsi + 442]
	# v45 = iconst.i64 485
	# v46 = iadd v0, v45
	lea	r14, [rsi + 485]
	# v47 = iconst.i64 530
	# v48 = iadd v0, v47
	add	rsi, 530
	# v49 = iconst.i64 3
	# v50 = iconst.i64 0
	xor	edi, edi
	# v51 = imul v50, v49
	imul	rdi, rdi, 3
	# v52 = iadd v51, v48
	add	rsi, rdi
	# v53 = imul v52, v49
	imul	rsi, rsi, 3
	# v54 = iadd v53, v46
	add	rsi, r14
	# v55 = imul v54, v49
	imul	rsi, rsi, 3
	# v56 = iadd v55, v44
	add	rsi, r13
	# v57 = imul v56, v49
	imul	rsi, rsi, 3
	# v58 = iadd v57, v42
	add	rsi, r12
	# v59 = imul v58, v49
	imul	rsi, rsi, 3
	# v60 = iadd v59, v40
	add	rsi, rbx
	# v61 = imul v60, v49
	imul	rsi, rsi, 3
	# v62 = iadd v61, v38
	add	rsi, r9
	# v63 = imul v62, v49
	imul	rsi, rsi, 3
	# v64 = iadd v63, v36
	add	rsi, r8
	# v65 = imul v64, v49
	imul	rsi, rsi, 3
	# v66 = iadd v65, v34
	mov	rcx, qword ptr [rbp + -48]
	add	rsi, rcx
	# v67 = imul v66, v49
	imul	rsi, rsi, 3
	# v68 = iadd v67, v32
	mov	rcx, qword ptr [rbp + -56]
	add	rsi, rcx
	# v69 = imul v68, v49
	imul	rsi, rsi, 3
	# v70 = iadd v69, v30
	mov	rcx, qword ptr [rbp + -64]
	add	rsi, rcx
	# v71 = imul v70, v49
	imul	rsi, rsi, 3
	# v72 = iadd v71, v28
	mov	rcx, qword ptr [rbp + -72]
	add	rsi, rcx
	# v73 = imul v72, v49
	imul	rsi, rsi, 3
	# v74 = iadd v73, v26
	mov	rcx, qword ptr [rbp + -80]
	add	rsi, rcx
	# v75 = imul v74, v49
	imul	rsi, rsi, 3
	# v76 = iadd v75, v24
	mov	rcx, qword ptr [rbp + -88]
	add	rsi, rcx
	# v77 = imul v76, v49
	imul	rsi, rsi, 3
	# v78 = iadd v77, v22
	mov	rcx, qword ptr [rbp + -96]
	add	rsi, rcx
	# v79 = imul v78, v49
	imul	rsi, rsi, 3
	# v80 = iadd v79, v20
	mov	rcx, qword ptr [rbp + -104]
	add	rsi, rcx
	# v81 = imul v80, v49
	imul	rsi, rsi, 3
	# v82 = iadd v81, v18
	mov	rcx, qword ptr [rbp + -112]
	add	rsi, rcx
	# v83 = imul v82, v49
	imul	rsi, rsi, 3
	# v84 = iadd v83, v16
	mov	rcx, qword ptr [rbp + -120]
	add	rsi, rcx
	# v85 = imul v84, v49
	imul	rsi, rsi, 3
	# v86 = iadd v85, v14
	mov	rcx, qword ptr [rbp + -128]
	add	rsi, rcx
	# v87 = imul v86, v49
	imul	rsi, rsi, 3
	# v88 = iadd v87, v12
	mov	rcx, qword ptr [rbp + -136]
	add	rsi, rcx
	# v89 = imul v88, v49
	imul	rsi, rsi, 3
	# v90 = iadd v89, v10
	mov	rcx, qword ptr [rbp + -144]
	add	rsi, rcx
	# v91 = imul v90, v49
	imul	rsi, rsi, 3
	# v92 = iadd v91, v8
	mov	rcx, qword ptr [rbp + -152]
	add	rsi, rcx
	# v93 = imul v92, v49
	imul	rsi, rsi, 3
	# v94 = iadd v93, v6
	mov	rcx, qword ptr [rbp + -160]
	add	rsi, rcx
	# v95 = imul v94, v49
	imul	rsi, rsi, 3
	# v96 = iadd v95, v4
	mov	rcx, qword ptr [rbp + -168]
	add	rsi, rcx
	# v97 = imul v96, v49
	imul	rsi, rsi, 3
	# v98 = iadd v97, v2
	mov	rcx, qword ptr [rbp + -176]
	add	rsi, rcx
	# return v98
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lpressure_end:
	.cfi_endproc
	.size "pressure", .-"pressure"
	.section	".text","ax",@progbits
	.globl "pressure_call"
	.p2align	4
	.type	"pressure_call",@function
"pressure_call":
.Lpressure_call_begin:
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
	sub	rsp, 152
	mov	rbx, rdi
.Lpressure_call_block0:
	# v1 = iconst.i64 1
	# v2 = iadd v0, v1
	lea	rax, [rbx + 1]
	mov	qword ptr [rbp + -192], rax
	# v3 = iconst.i64 2
	# v4 = iadd v0, v3
	lea	rax, [rbx + 2]
	mov	qword ptr [rbp + -184], rax
	# v5 = iconst.i64 5
	# v6 = iadd v0, v5
	lea	rax, [rbx + 5]
	mov	qword ptr [rbp + -176], rax
	# v7 = iconst.i64 10
	# v8 = iadd v0, v7
	lea	rax, [rbx + 10]
	mov	qword ptr [rbp + -168], rax
	# v9 = iconst.i64 17
	# v10 = iadd v0, v9
	lea	rax, [rbx + 17]
	mov	qword ptr [rbp + -160], rax
	# v11 = iconst.i64 26
	# v12 = iadd v0, v11
	lea	rax, [rbx + 26]
	mov	qword ptr [rbp + -152], rax
	# v13 = iconst.i64 37
	# v14 = iadd v0, v13
	lea	rax, [rbx + 37]
	mov	qword ptr [rbp + -144], rax
	# v15 = iconst.i64 50
	# v16 = iadd v0, v15
	lea	rax, [rbx + 50]
	mov	qword ptr [rbp + -136], rax
	# v17 = iconst.i64 65
	# v18 = iadd v0, v17
	lea	rax, [rbx + 65]
	mov	qword ptr [rbp + -128], rax
	# v19 = iconst.i64 82
	# v20 = iadd v0, v19
	lea	rax, [rbx + 82]
	mov	qword ptr [rbp + -120], rax
	# v21 = iconst.i64 101
	# v22 = iadd v0, v21
	lea	rax, [rbx + 101]
	mov	qword ptr [rbp + -112], rax
	# v23 = iconst.i64 122
	# v24 = iadd v0, v23
	lea	rax, [rbx + 122]
	mov	qword ptr [rbp + -104], rax
	# call @clobber()
	call	"clobber"@PLT
	# v25 = iconst.i64 145
	# v26 = iadd v0, v25
	lea	rax, [rbx + 145]
	mov	qword ptr [rbp + -96], rax
	# v27 = iconst.i64 170
	# v28 = iadd v0, v27
	lea	rax, [rbx + 170]
	mov	qword ptr [rbp + -88], rax
	# v29 = iconst.i64 197
	# v30 = iadd v0, v29
	lea	rax, [rbx + 197]
	mov	qword ptr [rbp + -80], rax
	# v31 = iconst.i64 226
	# v32 = iadd v0, v31
	lea	rax, [rbx + 226]
	mov	qword ptr [rbp + -72], rax
	# v33 = iconst.i64 257
	# v34 = iadd v0, v33
	lea	rax, [rbx + 257]
	mov	qword ptr [rbp + -64], rax
	# v35 = iconst.i64 290
	# v36 = iadd v0, v35
	lea	rax, [rbx + 290]
	mov	qword ptr [rbp + -56], rax
	# v37 = iconst.i64 325
	# v38 = iadd v0, v37
	lea	rax, [rbx + 325]
	mov	qword ptr [rbp + -48], rax
	# v39 = iconst.i64 362
	# v40 = iadd v0, v39
	lea	r15, [rbx + 362]
	# v41 = iconst.i64 401
	# v42 = iadd v0, v41
	lea	r12, [rbx + 401]
	# v43 = iconst.i64 442
	# v44 = iadd v0, v43
	lea	r13, [rbx + 442]
	# v45 = iconst.i64 485
	# v46 = iadd v0, v45
	lea	r14, [rbx + 485]
	# v47 = iconst.i64 530
	# v48 = iadd v0, v47
	add	rbx, 530
	# call @clobber()
	call	"clobber"@PLT
	# v49 = iconst.i64 3
	# v50 = iconst.i64 0
	xor	edi, edi
	# v51 = imul v50, v49
	imul	rdi, rdi, 3
	# v52 = iadd v51, v48
	add	rdi, rbx
	# v53 = imul v52, v49
	imul	rdi, rdi, 3
	# v54 = iadd v53, v46
	add	rdi, r14
	# v55 = imul v54, v49
	imul	rdi, rdi, 3
	# v56 = iadd v55, v44
	add	rdi, r13
	# v57 = imul v56, v49
	imul	rdi, rdi, 3
	# v58 = iadd v57, v42
	add	rdi, r12
	# v59 = imul v58, v49
	imul	rdi, rdi, 3
	# v60 = iadd v59, v40
	add	rdi, r15
	# v61 = imul v60, v49
	imul	rdi, rdi, 3
	# v62 = iadd v61, v38
	mov	rcx, qword ptr [rbp + -48]
	add	rdi, rcx
	# v63 = imul v62, v49
	imul	rdi, rdi, 3
	# v64 = iadd v63, v36
	mov	rcx, qword ptr [rbp + -56]
	add	rdi, rcx
	# v65 = imul v64, v49
	imul	rdi, rdi, 3
	# v66 = iadd v65, v34
	mov	rcx, qword ptr [rbp + -64]
	add	rdi, rcx
	# v67 = imul v66, v49
	imul	rdi, rdi, 3
	# v68 = iadd v67, v32
	mov	rcx, qword ptr [rbp + -72]
	add	rdi, rcx
	# v69 = imul v68, v49
	imul	rdi, rdi, 3
	# v70 = iadd v69, v30
	mov	rcx, qword ptr [rbp + -80]
	add	rdi, rcx
	# v71 = imul v70, v49
	imul	rdi, rdi, 3
	# v72 = iadd v71, v28
	mov	rcx, qword ptr [rbp + -88]
	add	rdi, rcx
	# v73 = imul v72, v49
	imul	rdi, rdi, 3
	# v74 = iadd v73, v26
	mov	rcx, qword ptr [rbp + -96]
	add	rdi, rcx
	# v75 = imul v74, v49
	imul	rdi, rdi, 3
	# v76 = iadd v75, v24
	mov	rcx, qword ptr [rbp + -104]
	add	rdi, rcx
	# v77 = imul v76, v49
	imul	rdi, rdi, 3
	# v78 = iadd v77, v22
	mov	rcx, qword ptr [rbp + -112]
	add	rdi, rcx
	# v79 = imul v78, v49
	imul	rdi, rdi, 3
	# v80 = iadd v79, v20
	mov	rcx, qword ptr [rbp + -120]
	add	rdi, rcx
	# v81 = imul v80, v49
	imul	rdi, rdi, 3
	# v82 = iadd v81, v18
	mov	rcx, qword ptr [rbp + -128]
	add	rdi, rcx
	# v83 = imul v82, v49
	imul	rdi, rdi, 3
	# v84 = iadd v83, v16
	mov	rcx, qword ptr [rbp + -136]
	add	rdi, rcx
	# v85 = imul v84, v49
	imul	rdi, rdi, 3
	# v86 = iadd v85, v14
	mov	rcx, qword ptr [rbp + -144]
	add	rdi, rcx
	# v87 = imul v86, v49
	imul	rdi, rdi, 3
	# v88 = iadd v87, v12
	mov	rcx, qword ptr [rbp + -152]
	add	rdi, rcx
	# v89 = imul v88, v49
	imul	rdi, rdi, 3
	# v90 = iadd v89, v10
	mov	rcx, qword ptr [rbp + -160]
	add	rdi, rcx
	# v91 = imul v90, v49
	imul	rdi, rdi, 3
	# v92 = iadd v91, v8
	mov	rcx, qword ptr [rbp + -168]
	add	rdi, rcx
	# v93 = imul v92, v49
	imul	rdi, rdi, 3
	# v94 = iadd v93, v6
	mov	rcx, qword ptr [rbp + -176]
	add	rdi, rcx
	# v95 = imul v94, v49
	imul	rdi, rdi, 3
	# v96 = iadd v95, v4
	mov	rcx, qword ptr [rbp + -184]
	add	rdi, rcx
	# v97 = imul v96, v49
	imul	rsi, rdi, 3
	# v98 = iadd v97, v2
	mov	rcx, qword ptr [rbp + -192]
	add	rsi, rcx
	# return v98
	mov	rax, rsi
	lea	rsp, [rbp + -40]
	pop	r15
	pop	r14
	pop	r13
	pop	r12
	pop	rbx
	pop	rbp
	ret
.Lpressure_call_end:
	.cfi_endproc
	.size "pressure_call", .-"pressure_call"
	.section	".note.GNU-stack","",@progbits
