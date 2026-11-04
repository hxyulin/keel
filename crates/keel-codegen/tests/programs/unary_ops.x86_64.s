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
.Lmain_block0:
	# v0 = iconst.i32 0
	xor	esi, esi
	# v1 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v2 = trunc.i8 v1
	# v3 = ineg v2
	neg	edi
	# v4 = iconst.i8 0
	# v5 = icmp.ne v3, v4
	cmp	dil, 0
	setne	dil
	# v6 = iconst.i32 1
	mov	r8d, 1
	# v7 = select v5, v6, v0
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v8 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v9 = trunc.i8 v8
	# v10 = bnot v9
	not	edi
	# v11 = iconst.i8 -1
	# v12 = icmp.ne v10, v11
	cmp	dil, 255
	setne	dil
	# v13 = iconst.i32 2
	mov	r8d, 2
	# v14 = select v12, v13, v7
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v15 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v16 = trunc.i8 v15
	# v17 = clz v16
	movzx	eax, dil
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 24
	mov	edi, eax
	# v18 = iconst.i8 8
	# v19 = icmp.ne v17, v18
	cmp	dil, 8
	setne	dil
	# v20 = iconst.i32 3
	mov	r8d, 3
	# v21 = select v19, v20, v14
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v22 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v23 = trunc.i8 v22
	# v24 = ctz v23
	mov	eax, edi
	or	eax, 256
	bsf	eax, eax
	mov	edi, eax
	# v25 = iconst.i8 8
	# v26 = icmp.ne v24, v25
	cmp	dil, 8
	setne	dil
	# v27 = iconst.i32 4
	mov	r8d, 4
	# v28 = select v26, v27, v21
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v29 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v30 = trunc.i8 v29
	# v31 = popcnt v30
	movzx	eax, dil
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v32 = iconst.i8 0
	# v33 = icmp.ne v31, v32
	cmp	dil, 0
	setne	dil
	# v34 = iconst.i32 5
	mov	r8d, 5
	# v35 = select v33, v34, v28
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v36 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v37 = trunc.i8 v36
	# v38 = ineg v37
	neg	edi
	# v39 = iconst.i8 -1
	# v40 = icmp.ne v38, v39
	cmp	dil, 255
	setne	dil
	# v41 = iconst.i32 6
	mov	r8d, 6
	# v42 = select v40, v41, v35
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v43 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v44 = trunc.i8 v43
	# v45 = bnot v44
	not	edi
	# v46 = iconst.i8 -2
	# v47 = icmp.ne v45, v46
	cmp	dil, 254
	setne	dil
	# v48 = iconst.i32 7
	mov	r8d, 7
	# v49 = select v47, v48, v42
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v50 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v51 = trunc.i8 v50
	# v52 = clz v51
	movzx	eax, dil
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 24
	mov	edi, eax
	# v53 = iconst.i8 7
	# v54 = icmp.ne v52, v53
	cmp	dil, 7
	setne	dil
	# v55 = iconst.i32 8
	mov	r8d, 8
	# v56 = select v54, v55, v49
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v57 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v58 = trunc.i8 v57
	# v59 = ctz v58
	mov	eax, edi
	or	eax, 256
	bsf	eax, eax
	mov	edi, eax
	# v60 = iconst.i8 0
	# v61 = icmp.ne v59, v60
	cmp	dil, 0
	setne	dil
	# v62 = iconst.i32 9
	mov	r8d, 9
	# v63 = select v61, v62, v56
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v64 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v65 = trunc.i8 v64
	# v66 = popcnt v65
	movzx	eax, dil
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v67 = iconst.i8 1
	# v68 = icmp.ne v66, v67
	cmp	dil, 1
	setne	dil
	# v69 = iconst.i32 10
	mov	r8d, 10
	# v70 = select v68, v69, v63
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v71 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v72 = trunc.i8 v71
	# v73 = ineg v72
	neg	edi
	# v74 = iconst.i8 1
	# v75 = icmp.ne v73, v74
	cmp	dil, 1
	setne	dil
	# v76 = iconst.i32 11
	mov	r8d, 11
	# v77 = select v75, v76, v70
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v78 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v79 = trunc.i8 v78
	# v80 = bnot v79
	not	edi
	# v81 = iconst.i8 0
	# v82 = icmp.ne v80, v81
	cmp	dil, 0
	setne	dil
	# v83 = iconst.i32 12
	mov	r8d, 12
	# v84 = select v82, v83, v77
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v85 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v86 = trunc.i8 v85
	# v87 = clz v86
	movzx	eax, dil
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 24
	mov	edi, eax
	# v88 = iconst.i8 0
	# v89 = icmp.ne v87, v88
	cmp	dil, 0
	setne	dil
	# v90 = iconst.i32 13
	mov	r8d, 13
	# v91 = select v89, v90, v84
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v92 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v93 = trunc.i8 v92
	# v94 = ctz v93
	mov	eax, edi
	or	eax, 256
	bsf	eax, eax
	mov	edi, eax
	# v95 = iconst.i8 0
	# v96 = icmp.ne v94, v95
	cmp	dil, 0
	setne	dil
	# v97 = iconst.i32 14
	mov	r8d, 14
	# v98 = select v96, v97, v91
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v99 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v100 = trunc.i8 v99
	# v101 = popcnt v100
	movzx	eax, dil
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v102 = iconst.i8 8
	# v103 = icmp.ne v101, v102
	cmp	dil, 8
	setne	dil
	# v104 = iconst.i32 15
	mov	r8d, 15
	# v105 = select v103, v104, v98
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v106 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v107 = trunc.i8 v106
	# v108 = ineg v107
	neg	edi
	# v109 = iconst.i8 -128
	# v110 = icmp.ne v108, v109
	cmp	dil, 128
	setne	dil
	# v111 = iconst.i32 16
	mov	r8d, 16
	# v112 = select v110, v111, v105
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v113 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v114 = trunc.i8 v113
	# v115 = bnot v114
	not	edi
	# v116 = iconst.i8 127
	# v117 = icmp.ne v115, v116
	cmp	dil, 127
	setne	dil
	# v118 = iconst.i32 17
	mov	r8d, 17
	# v119 = select v117, v118, v112
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v120 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v121 = trunc.i8 v120
	# v122 = clz v121
	movzx	eax, dil
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 24
	mov	edi, eax
	# v123 = iconst.i8 0
	# v124 = icmp.ne v122, v123
	cmp	dil, 0
	setne	dil
	# v125 = iconst.i32 18
	mov	r8d, 18
	# v126 = select v124, v125, v119
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v127 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v128 = trunc.i8 v127
	# v129 = ctz v128
	mov	eax, edi
	or	eax, 256
	bsf	eax, eax
	mov	edi, eax
	# v130 = iconst.i8 7
	# v131 = icmp.ne v129, v130
	cmp	dil, 7
	setne	dil
	# v132 = iconst.i32 19
	mov	r8d, 19
	# v133 = select v131, v132, v126
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v134 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v135 = trunc.i8 v134
	# v136 = popcnt v135
	movzx	eax, dil
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v137 = iconst.i8 1
	# v138 = icmp.ne v136, v137
	cmp	dil, 1
	setne	dil
	# v139 = iconst.i32 20
	mov	r8d, 20
	# v140 = select v138, v139, v133
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v141 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v142 = trunc.i8 v141
	# v143 = ineg v142
	neg	edi
	# v144 = iconst.i8 -90
	# v145 = icmp.ne v143, v144
	cmp	dil, 166
	setne	dil
	# v146 = iconst.i32 21
	mov	r8d, 21
	# v147 = select v145, v146, v140
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v148 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v149 = trunc.i8 v148
	# v150 = bnot v149
	not	edi
	# v151 = iconst.i8 -91
	# v152 = icmp.ne v150, v151
	cmp	dil, 165
	setne	dil
	# v153 = iconst.i32 22
	mov	r8d, 22
	# v154 = select v152, v153, v147
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v155 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v156 = trunc.i8 v155
	# v157 = clz v156
	movzx	eax, dil
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 24
	mov	edi, eax
	# v158 = iconst.i8 1
	# v159 = icmp.ne v157, v158
	cmp	dil, 1
	setne	dil
	# v160 = iconst.i32 23
	mov	r8d, 23
	# v161 = select v159, v160, v154
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v162 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v163 = trunc.i8 v162
	# v164 = ctz v163
	mov	eax, edi
	or	eax, 256
	bsf	eax, eax
	mov	edi, eax
	# v165 = iconst.i8 1
	# v166 = icmp.ne v164, v165
	cmp	dil, 1
	setne	dil
	# v167 = iconst.i32 24
	mov	r8d, 24
	# v168 = select v166, v167, v161
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v169 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v170 = trunc.i8 v169
	# v171 = popcnt v170
	movzx	eax, dil
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v172 = iconst.i8 4
	# v173 = icmp.ne v171, v172
	cmp	dil, 4
	setne	dil
	# v174 = iconst.i32 25
	mov	r8d, 25
	# v175 = select v173, v174, v168
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v176 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v177 = trunc.i8 v176
	# v178 = ineg v177
	neg	edi
	# v179 = iconst.i8 17
	# v180 = icmp.ne v178, v179
	cmp	dil, 17
	setne	dil
	# v181 = iconst.i32 26
	mov	r8d, 26
	# v182 = select v180, v181, v175
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v183 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v184 = trunc.i8 v183
	# v185 = bnot v184
	not	edi
	# v186 = iconst.i8 16
	# v187 = icmp.ne v185, v186
	cmp	dil, 16
	setne	dil
	# v188 = iconst.i32 27
	mov	r8d, 27
	# v189 = select v187, v188, v182
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v190 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v191 = trunc.i8 v190
	# v192 = clz v191
	movzx	eax, dil
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 24
	mov	edi, eax
	# v193 = iconst.i8 0
	# v194 = icmp.ne v192, v193
	cmp	dil, 0
	setne	dil
	# v195 = iconst.i32 28
	mov	r8d, 28
	# v196 = select v194, v195, v189
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v197 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v198 = trunc.i8 v197
	# v199 = ctz v198
	mov	eax, edi
	or	eax, 256
	bsf	eax, eax
	mov	edi, eax
	# v200 = iconst.i8 0
	# v201 = icmp.ne v199, v200
	cmp	dil, 0
	setne	dil
	# v202 = iconst.i32 29
	mov	r8d, 29
	# v203 = select v201, v202, v196
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v204 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v205 = trunc.i8 v204
	# v206 = popcnt v205
	movzx	eax, dil
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v207 = iconst.i8 7
	# v208 = icmp.ne v206, v207
	cmp	dil, 7
	setne	dil
	# v209 = iconst.i32 30
	mov	r8d, 30
	# v210 = select v208, v209, v203
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v211 = iconst.i64 -6502179613573254128
	movabs	rdi, 11944564460136297488
	# v212 = trunc.i8 v211
	# v213 = ineg v212
	neg	edi
	# v214 = iconst.i8 -16
	# v215 = icmp.ne v213, v214
	cmp	dil, 240
	setne	dil
	# v216 = iconst.i32 31
	mov	r8d, 31
	# v217 = select v215, v216, v210
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v218 = iconst.i64 -6502179613573254128
	movabs	rdi, 11944564460136297488
	# v219 = trunc.i8 v218
	# v220 = bnot v219
	not	edi
	# v221 = iconst.i8 -17
	# v222 = icmp.ne v220, v221
	cmp	dil, 239
	setne	dil
	# v223 = iconst.i32 32
	mov	r8d, 32
	# v224 = select v222, v223, v217
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v225 = iconst.i64 -6502179613573254128
	movabs	rdi, 11944564460136297488
	# v226 = trunc.i8 v225
	# v227 = clz v226
	movzx	eax, dil
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 24
	mov	edi, eax
	# v228 = iconst.i8 3
	# v229 = icmp.ne v227, v228
	cmp	dil, 3
	setne	dil
	# v230 = iconst.i32 33
	mov	r8d, 33
	# v231 = select v229, v230, v224
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v232 = iconst.i64 -6502179613573254128
	movabs	rdi, 11944564460136297488
	# v233 = trunc.i8 v232
	# v234 = ctz v233
	mov	eax, edi
	or	eax, 256
	bsf	eax, eax
	mov	edi, eax
	# v235 = iconst.i8 4
	# v236 = icmp.ne v234, v235
	cmp	dil, 4
	setne	dil
	# v237 = iconst.i32 34
	mov	r8d, 34
	# v238 = select v236, v237, v231
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v239 = iconst.i64 -6502179613573254128
	movabs	rdi, 11944564460136297488
	# v240 = trunc.i8 v239
	# v241 = popcnt v240
	movzx	eax, dil
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v242 = iconst.i8 1
	# v243 = icmp.ne v241, v242
	cmp	dil, 1
	setne	dil
	# v244 = iconst.i32 35
	mov	r8d, 35
	# v245 = select v243, v244, v238
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v246 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v247 = trunc.i16 v246
	# v248 = ineg v247
	neg	edi
	# v249 = iconst.i16 0
	# v250 = icmp.ne v248, v249
	cmp	di, 0
	setne	dil
	# v251 = iconst.i32 36
	mov	r8d, 36
	# v252 = select v250, v251, v245
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v253 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v254 = trunc.i16 v253
	# v255 = bnot v254
	not	edi
	# v256 = iconst.i16 -1
	# v257 = icmp.ne v255, v256
	cmp	di, 65535
	setne	dil
	# v258 = iconst.i32 37
	mov	r8d, 37
	# v259 = select v257, v258, v252
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v260 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v261 = trunc.i16 v260
	# v262 = clz v261
	movzx	eax, di
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 16
	mov	edi, eax
	# v263 = iconst.i16 16
	# v264 = icmp.ne v262, v263
	cmp	di, 16
	setne	dil
	# v265 = iconst.i32 38
	mov	r8d, 38
	# v266 = select v264, v265, v259
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v267 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v268 = trunc.i16 v267
	# v269 = ctz v268
	mov	eax, edi
	or	eax, 65536
	bsf	eax, eax
	mov	edi, eax
	# v270 = iconst.i16 16
	# v271 = icmp.ne v269, v270
	cmp	di, 16
	setne	dil
	# v272 = iconst.i32 39
	mov	r8d, 39
	# v273 = select v271, v272, v266
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v274 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v275 = trunc.i16 v274
	# v276 = popcnt v275
	movzx	eax, di
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v277 = iconst.i16 0
	# v278 = icmp.ne v276, v277
	cmp	di, 0
	setne	dil
	# v279 = iconst.i32 40
	mov	r8d, 40
	# v280 = select v278, v279, v273
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v281 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v282 = trunc.i16 v281
	# v283 = bswap v282
	rol	di, 8
	# v284 = iconst.i16 0
	# v285 = icmp.ne v283, v284
	cmp	di, 0
	setne	dil
	# v286 = iconst.i32 41
	mov	r8d, 41
	# v287 = select v285, v286, v280
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v288 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v289 = trunc.i16 v288
	# v290 = ineg v289
	neg	edi
	# v291 = iconst.i16 -1
	# v292 = icmp.ne v290, v291
	cmp	di, 65535
	setne	dil
	# v293 = iconst.i32 42
	mov	r8d, 42
	# v294 = select v292, v293, v287
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v295 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v296 = trunc.i16 v295
	# v297 = bnot v296
	not	edi
	# v298 = iconst.i16 -2
	# v299 = icmp.ne v297, v298
	cmp	di, 65534
	setne	dil
	# v300 = iconst.i32 43
	mov	r8d, 43
	# v301 = select v299, v300, v294
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v302 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v303 = trunc.i16 v302
	# v304 = clz v303
	movzx	eax, di
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 16
	mov	edi, eax
	# v305 = iconst.i16 15
	# v306 = icmp.ne v304, v305
	cmp	di, 15
	setne	dil
	# v307 = iconst.i32 44
	mov	r8d, 44
	# v308 = select v306, v307, v301
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v309 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v310 = trunc.i16 v309
	# v311 = ctz v310
	mov	eax, edi
	or	eax, 65536
	bsf	eax, eax
	mov	edi, eax
	# v312 = iconst.i16 0
	# v313 = icmp.ne v311, v312
	cmp	di, 0
	setne	dil
	# v314 = iconst.i32 45
	mov	r8d, 45
	# v315 = select v313, v314, v308
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v316 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v317 = trunc.i16 v316
	# v318 = popcnt v317
	movzx	eax, di
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v319 = iconst.i16 1
	# v320 = icmp.ne v318, v319
	cmp	di, 1
	setne	dil
	# v321 = iconst.i32 46
	mov	r8d, 46
	# v322 = select v320, v321, v315
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v323 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v324 = trunc.i16 v323
	# v325 = bswap v324
	rol	di, 8
	# v326 = iconst.i16 256
	# v327 = icmp.ne v325, v326
	cmp	di, 256
	setne	dil
	# v328 = iconst.i32 47
	mov	r8d, 47
	# v329 = select v327, v328, v322
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v330 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v331 = trunc.i16 v330
	# v332 = ineg v331
	neg	edi
	# v333 = iconst.i16 1
	# v334 = icmp.ne v332, v333
	cmp	di, 1
	setne	dil
	# v335 = iconst.i32 48
	mov	r8d, 48
	# v336 = select v334, v335, v329
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v337 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v338 = trunc.i16 v337
	# v339 = bnot v338
	not	edi
	# v340 = iconst.i16 0
	# v341 = icmp.ne v339, v340
	cmp	di, 0
	setne	dil
	# v342 = iconst.i32 49
	mov	r8d, 49
	# v343 = select v341, v342, v336
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v344 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v345 = trunc.i16 v344
	# v346 = clz v345
	movzx	eax, di
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 16
	mov	edi, eax
	# v347 = iconst.i16 0
	# v348 = icmp.ne v346, v347
	cmp	di, 0
	setne	dil
	# v349 = iconst.i32 50
	mov	r8d, 50
	# v350 = select v348, v349, v343
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v351 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v352 = trunc.i16 v351
	# v353 = ctz v352
	mov	eax, edi
	or	eax, 65536
	bsf	eax, eax
	mov	edi, eax
	# v354 = iconst.i16 0
	# v355 = icmp.ne v353, v354
	cmp	di, 0
	setne	dil
	# v356 = iconst.i32 51
	mov	r8d, 51
	# v357 = select v355, v356, v350
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v358 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v359 = trunc.i16 v358
	# v360 = popcnt v359
	movzx	eax, di
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v361 = iconst.i16 16
	# v362 = icmp.ne v360, v361
	cmp	di, 16
	setne	dil
	# v363 = iconst.i32 52
	mov	r8d, 52
	# v364 = select v362, v363, v357
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v365 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v366 = trunc.i16 v365
	# v367 = bswap v366
	rol	di, 8
	# v368 = iconst.i16 -1
	# v369 = icmp.ne v367, v368
	cmp	di, 65535
	setne	dil
	# v370 = iconst.i32 53
	mov	r8d, 53
	# v371 = select v369, v370, v364
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v372 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v373 = trunc.i16 v372
	# v374 = ineg v373
	neg	edi
	# v375 = iconst.i16 -32768
	# v376 = icmp.ne v374, v375
	cmp	di, 32768
	setne	dil
	# v377 = iconst.i32 54
	mov	r8d, 54
	# v378 = select v376, v377, v371
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v379 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v380 = trunc.i16 v379
	# v381 = bnot v380
	not	edi
	# v382 = iconst.i16 32767
	# v383 = icmp.ne v381, v382
	cmp	di, 32767
	setne	dil
	# v384 = iconst.i32 55
	mov	r8d, 55
	# v385 = select v383, v384, v378
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v386 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v387 = trunc.i16 v386
	# v388 = clz v387
	movzx	eax, di
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 16
	mov	edi, eax
	# v389 = iconst.i16 0
	# v390 = icmp.ne v388, v389
	cmp	di, 0
	setne	dil
	# v391 = iconst.i32 56
	mov	r8d, 56
	# v392 = select v390, v391, v385
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v393 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v394 = trunc.i16 v393
	# v395 = ctz v394
	mov	eax, edi
	or	eax, 65536
	bsf	eax, eax
	mov	edi, eax
	# v396 = iconst.i16 15
	# v397 = icmp.ne v395, v396
	cmp	di, 15
	setne	dil
	# v398 = iconst.i32 57
	mov	r8d, 57
	# v399 = select v397, v398, v392
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v400 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v401 = trunc.i16 v400
	# v402 = popcnt v401
	movzx	eax, di
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v403 = iconst.i16 1
	# v404 = icmp.ne v402, v403
	cmp	di, 1
	setne	dil
	# v405 = iconst.i32 58
	mov	r8d, 58
	# v406 = select v404, v405, v399
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v407 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v408 = trunc.i16 v407
	# v409 = bswap v408
	rol	di, 8
	# v410 = iconst.i16 128
	# v411 = icmp.ne v409, v410
	cmp	di, 128
	setne	dil
	# v412 = iconst.i32 59
	mov	r8d, 59
	# v413 = select v411, v412, v406
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v414 = iconst.i64 -6502179613573316518
	movabs	rdi, 11944564460136235098
	# v415 = trunc.i16 v414
	# v416 = ineg v415
	neg	edi
	# v417 = iconst.i16 -90
	# v418 = icmp.ne v416, v417
	cmp	di, 65446
	setne	dil
	# v419 = iconst.i32 60
	mov	r8d, 60
	# v420 = select v418, v419, v413
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v421 = iconst.i64 -6502179613573316518
	movabs	rdi, 11944564460136235098
	# v422 = trunc.i16 v421
	# v423 = bnot v422
	not	edi
	# v424 = iconst.i16 -91
	# v425 = icmp.ne v423, v424
	cmp	di, 65445
	setne	dil
	# v426 = iconst.i32 61
	mov	r8d, 61
	# v427 = select v425, v426, v420
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v428 = iconst.i64 -6502179613573316518
	movabs	rdi, 11944564460136235098
	# v429 = trunc.i16 v428
	# v430 = clz v429
	movzx	eax, di
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 16
	mov	edi, eax
	# v431 = iconst.i16 9
	# v432 = icmp.ne v430, v431
	cmp	di, 9
	setne	dil
	# v433 = iconst.i32 62
	mov	r8d, 62
	# v434 = select v432, v433, v427
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v435 = iconst.i64 -6502179613573316518
	movabs	rdi, 11944564460136235098
	# v436 = trunc.i16 v435
	# v437 = ctz v436
	mov	eax, edi
	or	eax, 65536
	bsf	eax, eax
	mov	edi, eax
	# v438 = iconst.i16 1
	# v439 = icmp.ne v437, v438
	cmp	di, 1
	setne	dil
	# v440 = iconst.i32 63
	mov	r8d, 63
	# v441 = select v439, v440, v434
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v442 = iconst.i64 -6502179613573316518
	movabs	rdi, 11944564460136235098
	# v443 = trunc.i16 v442
	# v444 = popcnt v443
	movzx	eax, di
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v445 = iconst.i16 4
	# v446 = icmp.ne v444, v445
	cmp	di, 4
	setne	dil
	# v447 = iconst.i32 64
	mov	r8d, 64
	# v448 = select v446, v447, v441
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v449 = iconst.i64 -6502179613573316518
	movabs	rdi, 11944564460136235098
	# v450 = trunc.i16 v449
	# v451 = bswap v450
	rol	di, 8
	# v452 = iconst.i16 23040
	# v453 = icmp.ne v451, v452
	cmp	di, 23040
	setne	dil
	# v454 = iconst.i32 65
	mov	r8d, 65
	# v455 = select v453, v454, v448
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v456 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v457 = trunc.i16 v456
	# v458 = ineg v457
	neg	edi
	# v459 = iconst.i16 12817
	# v460 = icmp.ne v458, v459
	cmp	di, 12817
	setne	dil
	# v461 = iconst.i32 66
	mov	r8d, 66
	# v462 = select v460, v461, v455
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v463 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v464 = trunc.i16 v463
	# v465 = bnot v464
	not	edi
	# v466 = iconst.i16 12816
	# v467 = icmp.ne v465, v466
	cmp	di, 12816
	setne	dil
	# v468 = iconst.i32 67
	mov	r8d, 67
	# v469 = select v467, v468, v462
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v470 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v471 = trunc.i16 v470
	# v472 = clz v471
	movzx	eax, di
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 16
	mov	edi, eax
	# v473 = iconst.i16 0
	# v474 = icmp.ne v472, v473
	cmp	di, 0
	setne	dil
	# v475 = iconst.i32 68
	mov	r8d, 68
	# v476 = select v474, v475, v469
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v477 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v478 = trunc.i16 v477
	# v479 = ctz v478
	mov	eax, edi
	or	eax, 65536
	bsf	eax, eax
	mov	edi, eax
	# v480 = iconst.i16 0
	# v481 = icmp.ne v479, v480
	cmp	di, 0
	setne	dil
	# v482 = iconst.i32 69
	mov	r8d, 69
	# v483 = select v481, v482, v476
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v484 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v485 = trunc.i16 v484
	# v486 = popcnt v485
	movzx	eax, di
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v487 = iconst.i16 12
	# v488 = icmp.ne v486, v487
	cmp	di, 12
	setne	dil
	# v489 = iconst.i32 70
	mov	r8d, 70
	# v490 = select v488, v489, v483
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v491 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v492 = trunc.i16 v491
	# v493 = bswap v492
	rol	di, 8
	# v494 = iconst.i16 -4147
	# v495 = icmp.ne v493, v494
	cmp	di, 61389
	setne	dil
	# v496 = iconst.i32 71
	mov	r8d, 71
	# v497 = select v495, v496, v490
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v498 = iconst.i64 -6502179613573316352
	movabs	rdi, 11944564460136235264
	# v499 = trunc.i16 v498
	# v500 = ineg v499
	neg	edi
	# v501 = iconst.i16 -256
	# v502 = icmp.ne v500, v501
	cmp	di, 65280
	setne	dil
	# v503 = iconst.i32 72
	mov	r8d, 72
	# v504 = select v502, v503, v497
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v505 = iconst.i64 -6502179613573316352
	movabs	rdi, 11944564460136235264
	# v506 = trunc.i16 v505
	# v507 = bnot v506
	not	edi
	# v508 = iconst.i16 -257
	# v509 = icmp.ne v507, v508
	cmp	di, 65279
	setne	dil
	# v510 = iconst.i32 73
	mov	r8d, 73
	# v511 = select v509, v510, v504
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v512 = iconst.i64 -6502179613573316352
	movabs	rdi, 11944564460136235264
	# v513 = trunc.i16 v512
	# v514 = clz v513
	movzx	eax, di
	bsr	eax, eax
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	sub	eax, 16
	mov	edi, eax
	# v515 = iconst.i16 7
	# v516 = icmp.ne v514, v515
	cmp	di, 7
	setne	dil
	# v517 = iconst.i32 74
	mov	r8d, 74
	# v518 = select v516, v517, v511
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v519 = iconst.i64 -6502179613573316352
	movabs	rdi, 11944564460136235264
	# v520 = trunc.i16 v519
	# v521 = ctz v520
	mov	eax, edi
	or	eax, 65536
	bsf	eax, eax
	mov	edi, eax
	# v522 = iconst.i16 8
	# v523 = icmp.ne v521, v522
	cmp	di, 8
	setne	dil
	# v524 = iconst.i32 75
	mov	r8d, 75
	# v525 = select v523, v524, v518
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v526 = iconst.i64 -6502179613573316352
	movabs	rdi, 11944564460136235264
	# v527 = trunc.i16 v526
	# v528 = popcnt v527
	movzx	eax, di
	mov	eax, eax
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v529 = iconst.i16 1
	# v530 = icmp.ne v528, v529
	cmp	di, 1
	setne	dil
	# v531 = iconst.i32 76
	mov	r8d, 76
	# v532 = select v530, v531, v525
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v533 = iconst.i64 -6502179613573316352
	movabs	rdi, 11944564460136235264
	# v534 = trunc.i16 v533
	# v535 = bswap v534
	rol	di, 8
	# v536 = iconst.i16 1
	# v537 = icmp.ne v535, v536
	cmp	di, 1
	setne	dil
	# v538 = iconst.i32 77
	mov	r8d, 77
	# v539 = select v537, v538, v532
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v540 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v541 = trunc.i32 v540
	# v542 = ineg v541
	neg	edi
	# v543 = iconst.i32 0
	# v544 = icmp.ne v542, v543
	cmp	edi, 0
	setne	dil
	# v545 = iconst.i32 78
	mov	r8d, 78
	# v546 = select v544, v545, v539
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v547 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v548 = trunc.i32 v547
	# v549 = bnot v548
	not	edi
	# v550 = iconst.i32 -1
	# v551 = icmp.ne v549, v550
	cmp	edi, 4294967295
	setne	dil
	# v552 = iconst.i32 79
	mov	r8d, 79
	# v553 = select v551, v552, v546
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v554 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v555 = trunc.i32 v554
	# v556 = clz v555
	bsr	eax, edi
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	mov	edi, eax
	# v557 = iconst.i32 32
	# v558 = icmp.ne v556, v557
	cmp	edi, 32
	setne	dil
	# v559 = iconst.i32 80
	mov	r8d, 80
	# v560 = select v558, v559, v553
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v561 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v562 = trunc.i32 v561
	# v563 = ctz v562
	bsf	eax, edi
	mov	ecx, 32
	cmove	eax, ecx
	mov	rdi, rax
	# v564 = iconst.i32 32
	# v565 = icmp.ne v563, v564
	cmp	edi, 32
	setne	dil
	# v566 = iconst.i32 81
	mov	r8d, 81
	# v567 = select v565, v566, v560
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v568 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v569 = trunc.i32 v568
	# v570 = popcnt v569
	mov	eax, edi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v571 = iconst.i32 0
	# v572 = icmp.ne v570, v571
	cmp	edi, 0
	setne	dil
	# v573 = iconst.i32 82
	mov	r8d, 82
	# v574 = select v572, v573, v567
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v575 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v576 = trunc.i32 v575
	# v577 = bswap v576
	bswap	edi
	# v578 = iconst.i32 0
	# v579 = icmp.ne v577, v578
	cmp	edi, 0
	setne	dil
	# v580 = iconst.i32 83
	mov	r8d, 83
	# v581 = select v579, v580, v574
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v582 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v583 = trunc.i32 v582
	# v584 = ineg v583
	neg	edi
	# v585 = iconst.i32 -1
	# v586 = icmp.ne v584, v585
	cmp	edi, 4294967295
	setne	dil
	# v587 = iconst.i32 84
	mov	r8d, 84
	# v588 = select v586, v587, v581
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v589 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v590 = trunc.i32 v589
	# v591 = bnot v590
	not	edi
	# v592 = iconst.i32 -2
	# v593 = icmp.ne v591, v592
	cmp	edi, 4294967294
	setne	dil
	# v594 = iconst.i32 85
	mov	r8d, 85
	# v595 = select v593, v594, v588
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v596 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v597 = trunc.i32 v596
	# v598 = clz v597
	bsr	eax, edi
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	mov	edi, eax
	# v599 = iconst.i32 31
	# v600 = icmp.ne v598, v599
	cmp	edi, 31
	setne	dil
	# v601 = iconst.i32 86
	mov	r8d, 86
	# v602 = select v600, v601, v595
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v603 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v604 = trunc.i32 v603
	# v605 = ctz v604
	bsf	eax, edi
	mov	ecx, 32
	cmove	eax, ecx
	mov	rdi, rax
	# v606 = iconst.i32 0
	# v607 = icmp.ne v605, v606
	cmp	edi, 0
	setne	dil
	# v608 = iconst.i32 87
	mov	r8d, 87
	# v609 = select v607, v608, v602
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v610 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v611 = trunc.i32 v610
	# v612 = popcnt v611
	mov	eax, edi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v613 = iconst.i32 1
	# v614 = icmp.ne v612, v613
	cmp	edi, 1
	setne	dil
	# v615 = iconst.i32 88
	mov	r8d, 88
	# v616 = select v614, v615, v609
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v617 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v618 = trunc.i32 v617
	# v619 = bswap v618
	bswap	edi
	# v620 = iconst.i32 16777216
	# v621 = icmp.ne v619, v620
	cmp	edi, 16777216
	setne	dil
	# v622 = iconst.i32 89
	mov	r8d, 89
	# v623 = select v621, v622, v616
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v624 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v625 = trunc.i32 v624
	# v626 = ineg v625
	neg	edi
	# v627 = iconst.i32 1
	# v628 = icmp.ne v626, v627
	cmp	edi, 1
	setne	dil
	# v629 = iconst.i32 90
	mov	r8d, 90
	# v630 = select v628, v629, v623
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v631 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v632 = trunc.i32 v631
	# v633 = bnot v632
	not	edi
	# v634 = iconst.i32 0
	# v635 = icmp.ne v633, v634
	cmp	edi, 0
	setne	dil
	# v636 = iconst.i32 91
	mov	r8d, 91
	# v637 = select v635, v636, v630
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v638 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v639 = trunc.i32 v638
	# v640 = clz v639
	bsr	eax, edi
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	mov	edi, eax
	# v641 = iconst.i32 0
	# v642 = icmp.ne v640, v641
	cmp	edi, 0
	setne	dil
	# v643 = iconst.i32 92
	mov	r8d, 92
	# v644 = select v642, v643, v637
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v645 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v646 = trunc.i32 v645
	# v647 = ctz v646
	bsf	eax, edi
	mov	ecx, 32
	cmove	eax, ecx
	mov	rdi, rax
	# v648 = iconst.i32 0
	# v649 = icmp.ne v647, v648
	cmp	edi, 0
	setne	dil
	# v650 = iconst.i32 93
	mov	r8d, 93
	# v651 = select v649, v650, v644
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v652 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v653 = trunc.i32 v652
	# v654 = popcnt v653
	mov	eax, edi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v655 = iconst.i32 32
	# v656 = icmp.ne v654, v655
	cmp	edi, 32
	setne	dil
	# v657 = iconst.i32 94
	mov	r8d, 94
	# v658 = select v656, v657, v651
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v659 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v660 = trunc.i32 v659
	# v661 = bswap v660
	bswap	edi
	# v662 = iconst.i32 -1
	# v663 = icmp.ne v661, v662
	cmp	edi, 4294967295
	setne	dil
	# v664 = iconst.i32 95
	mov	r8d, 95
	# v665 = select v663, v664, v658
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v666 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v667 = trunc.i32 v666
	# v668 = ineg v667
	neg	edi
	# v669 = iconst.i32 -2147483648
	# v670 = icmp.ne v668, v669
	cmp	edi, 2147483648
	setne	dil
	# v671 = iconst.i32 96
	mov	r8d, 96
	# v672 = select v670, v671, v665
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v673 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v674 = trunc.i32 v673
	# v675 = bnot v674
	not	edi
	# v676 = iconst.i32 2147483647
	# v677 = icmp.ne v675, v676
	cmp	edi, 2147483647
	setne	dil
	# v678 = iconst.i32 97
	mov	r8d, 97
	# v679 = select v677, v678, v672
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v680 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v681 = trunc.i32 v680
	# v682 = clz v681
	bsr	eax, edi
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	mov	edi, eax
	# v683 = iconst.i32 0
	# v684 = icmp.ne v682, v683
	cmp	edi, 0
	setne	dil
	# v685 = iconst.i32 98
	mov	r8d, 98
	# v686 = select v684, v685, v679
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v687 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v688 = trunc.i32 v687
	# v689 = ctz v688
	bsf	eax, edi
	mov	ecx, 32
	cmove	eax, ecx
	mov	rdi, rax
	# v690 = iconst.i32 31
	# v691 = icmp.ne v689, v690
	cmp	edi, 31
	setne	dil
	# v692 = iconst.i32 99
	mov	r8d, 99
	# v693 = select v691, v692, v686
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v694 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v695 = trunc.i32 v694
	# v696 = popcnt v695
	mov	eax, edi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v697 = iconst.i32 1
	# v698 = icmp.ne v696, v697
	cmp	edi, 1
	setne	dil
	# v699 = iconst.i32 100
	mov	r8d, 100
	# v700 = select v698, v699, v693
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v701 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v702 = trunc.i32 v701
	# v703 = bswap v702
	bswap	edi
	# v704 = iconst.i32 128
	# v705 = icmp.ne v703, v704
	cmp	edi, 128
	setne	dil
	# v706 = iconst.i32 101
	mov	r8d, 101
	# v707 = select v705, v706, v700
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v708 = iconst.i64 -6502179615371427750
	movabs	rdi, 11944564458338123866
	# v709 = trunc.i32 v708
	# v710 = ineg v709
	neg	edi
	# v711 = iconst.i32 -90
	# v712 = icmp.ne v710, v711
	cmp	edi, 4294967206
	setne	dil
	# v713 = iconst.i32 102
	mov	r8d, 102
	# v714 = select v712, v713, v707
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v715 = iconst.i64 -6502179615371427750
	movabs	rdi, 11944564458338123866
	# v716 = trunc.i32 v715
	# v717 = bnot v716
	not	edi
	# v718 = iconst.i32 -91
	# v719 = icmp.ne v717, v718
	cmp	edi, 4294967205
	setne	dil
	# v720 = iconst.i32 103
	mov	r8d, 103
	# v721 = select v719, v720, v714
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v722 = iconst.i64 -6502179615371427750
	movabs	rdi, 11944564458338123866
	# v723 = trunc.i32 v722
	# v724 = clz v723
	bsr	eax, edi
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	mov	edi, eax
	# v725 = iconst.i32 25
	# v726 = icmp.ne v724, v725
	cmp	edi, 25
	setne	dil
	# v727 = iconst.i32 104
	mov	r8d, 104
	# v728 = select v726, v727, v721
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v729 = iconst.i64 -6502179615371427750
	movabs	rdi, 11944564458338123866
	# v730 = trunc.i32 v729
	# v731 = ctz v730
	bsf	eax, edi
	mov	ecx, 32
	cmove	eax, ecx
	mov	rdi, rax
	# v732 = iconst.i32 1
	# v733 = icmp.ne v731, v732
	cmp	edi, 1
	setne	dil
	# v734 = iconst.i32 105
	mov	r8d, 105
	# v735 = select v733, v734, v728
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v736 = iconst.i64 -6502179615371427750
	movabs	rdi, 11944564458338123866
	# v737 = trunc.i32 v736
	# v738 = popcnt v737
	mov	eax, edi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v739 = iconst.i32 4
	# v740 = icmp.ne v738, v739
	cmp	edi, 4
	setne	dil
	# v741 = iconst.i32 106
	mov	r8d, 106
	# v742 = select v740, v741, v735
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v743 = iconst.i64 -6502179615371427750
	movabs	rdi, 11944564458338123866
	# v744 = trunc.i32 v743
	# v745 = bswap v744
	bswap	edi
	# v746 = iconst.i32 1509949440
	# v747 = icmp.ne v745, v746
	cmp	edi, 1509949440
	setne	dil
	# v748 = iconst.i32 107
	mov	r8d, 107
	# v749 = select v747, v748, v742
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v750 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v751 = trunc.i32 v750
	# v752 = ineg v751
	neg	edi
	# v753 = iconst.i32 1985229329
	# v754 = icmp.ne v752, v753
	cmp	edi, 1985229329
	setne	dil
	# v755 = iconst.i32 108
	mov	r8d, 108
	# v756 = select v754, v755, v749
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v757 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v758 = trunc.i32 v757
	# v759 = bnot v758
	not	edi
	# v760 = iconst.i32 1985229328
	# v761 = icmp.ne v759, v760
	cmp	edi, 1985229328
	setne	dil
	# v762 = iconst.i32 109
	mov	r8d, 109
	# v763 = select v761, v762, v756
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v764 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v765 = trunc.i32 v764
	# v766 = clz v765
	bsr	eax, edi
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	mov	edi, eax
	# v767 = iconst.i32 0
	# v768 = icmp.ne v766, v767
	cmp	edi, 0
	setne	dil
	# v769 = iconst.i32 110
	mov	r8d, 110
	# v770 = select v768, v769, v763
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v771 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v772 = trunc.i32 v771
	# v773 = ctz v772
	bsf	eax, edi
	mov	ecx, 32
	cmove	eax, ecx
	mov	rdi, rax
	# v774 = iconst.i32 0
	# v775 = icmp.ne v773, v774
	cmp	edi, 0
	setne	dil
	# v776 = iconst.i32 111
	mov	r8d, 111
	# v777 = select v775, v776, v770
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v778 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v779 = trunc.i32 v778
	# v780 = popcnt v779
	mov	eax, edi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v781 = iconst.i32 20
	# v782 = icmp.ne v780, v781
	cmp	edi, 20
	setne	dil
	# v783 = iconst.i32 112
	mov	r8d, 112
	# v784 = select v782, v783, v777
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v785 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v786 = trunc.i32 v785
	# v787 = bswap v786
	bswap	edi
	# v788 = iconst.i32 -271733879
	# v789 = icmp.ne v787, v788
	cmp	edi, 4023233417
	setne	dil
	# v790 = iconst.i32 113
	mov	r8d, 113
	# v791 = select v789, v790, v784
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v792 = iconst.i64 -6502179615371362304
	movabs	rdi, 11944564458338189312
	# v793 = trunc.i32 v792
	# v794 = ineg v793
	neg	edi
	# v795 = iconst.i32 -65536
	# v796 = icmp.ne v794, v795
	cmp	edi, 4294901760
	setne	dil
	# v797 = iconst.i32 114
	mov	r8d, 114
	# v798 = select v796, v797, v791
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v799 = iconst.i64 -6502179615371362304
	movabs	rdi, 11944564458338189312
	# v800 = trunc.i32 v799
	# v801 = bnot v800
	not	edi
	# v802 = iconst.i32 -65537
	# v803 = icmp.ne v801, v802
	cmp	edi, 4294901759
	setne	dil
	# v804 = iconst.i32 115
	mov	r8d, 115
	# v805 = select v803, v804, v798
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v806 = iconst.i64 -6502179615371362304
	movabs	rdi, 11944564458338189312
	# v807 = trunc.i32 v806
	# v808 = clz v807
	bsr	eax, edi
	mov	ecx, 63
	cmove	eax, ecx
	xor	eax, 31
	mov	edi, eax
	# v809 = iconst.i32 15
	# v810 = icmp.ne v808, v809
	cmp	edi, 15
	setne	dil
	# v811 = iconst.i32 116
	mov	r8d, 116
	# v812 = select v810, v811, v805
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v813 = iconst.i64 -6502179615371362304
	movabs	rdi, 11944564458338189312
	# v814 = trunc.i32 v813
	# v815 = ctz v814
	bsf	eax, edi
	mov	ecx, 32
	cmove	eax, ecx
	mov	rdi, rax
	# v816 = iconst.i32 16
	# v817 = icmp.ne v815, v816
	cmp	edi, 16
	setne	dil
	# v818 = iconst.i32 117
	mov	r8d, 117
	# v819 = select v817, v818, v812
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v820 = iconst.i64 -6502179615371362304
	movabs	rdi, 11944564458338189312
	# v821 = trunc.i32 v820
	# v822 = popcnt v821
	mov	eax, edi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v823 = iconst.i32 1
	# v824 = icmp.ne v822, v823
	cmp	edi, 1
	setne	dil
	# v825 = iconst.i32 118
	mov	r8d, 118
	# v826 = select v824, v825, v819
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v827 = iconst.i64 -6502179615371362304
	movabs	rdi, 11944564458338189312
	# v828 = trunc.i32 v827
	# v829 = bswap v828
	bswap	edi
	# v830 = iconst.i32 256
	# v831 = icmp.ne v829, v830
	cmp	edi, 256
	setne	dil
	# v832 = iconst.i32 119
	mov	r8d, 119
	# v833 = select v831, v832, v826
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v834 = iconst.i64 0
	xor	edi, edi
	# v835 = ineg v834
	neg	rdi
	# v836 = iconst.i64 0
	# v837 = icmp.ne v835, v836
	cmp	rdi, 0
	setne	dil
	# v838 = iconst.i32 120
	mov	r8d, 120
	# v839 = select v837, v838, v833
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v840 = iconst.i64 0
	xor	edi, edi
	# v841 = bnot v840
	not	rdi
	# v842 = iconst.i64 -1
	# v843 = icmp.ne v841, v842
	cmp	rdi, -1
	setne	dil
	# v844 = iconst.i32 121
	mov	r8d, 121
	# v845 = select v843, v844, v839
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v846 = iconst.i64 0
	xor	edi, edi
	# v847 = clz v846
	bsr	rax, rdi
	mov	ecx, 127
	cmove	rax, rcx
	xor	eax, 63
	mov	edi, eax
	# v848 = iconst.i64 64
	# v849 = icmp.ne v847, v848
	cmp	rdi, 64
	setne	dil
	# v850 = iconst.i32 122
	mov	r8d, 122
	# v851 = select v849, v850, v845
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v852 = iconst.i64 0
	xor	edi, edi
	# v853 = ctz v852
	bsf	rax, rdi
	mov	ecx, 64
	cmove	rax, rcx
	mov	rdi, rax
	# v854 = iconst.i64 64
	# v855 = icmp.ne v853, v854
	cmp	rdi, 64
	setne	dil
	# v856 = iconst.i32 123
	mov	r8d, 123
	# v857 = select v855, v856, v851
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v858 = iconst.i64 0
	xor	edi, edi
	# v859 = popcnt v858
	mov	rax, rdi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v860 = iconst.i64 0
	# v861 = icmp.ne v859, v860
	cmp	rdi, 0
	setne	dil
	# v862 = iconst.i32 124
	mov	r8d, 124
	# v863 = select v861, v862, v857
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v864 = iconst.i64 0
	xor	edi, edi
	# v865 = bswap v864
	bswap	rdi
	# v866 = iconst.i64 0
	# v867 = icmp.ne v865, v866
	cmp	rdi, 0
	setne	dil
	# v868 = iconst.i32 125
	mov	r8d, 125
	# v869 = select v867, v868, v863
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v870 = iconst.i64 1
	mov	edi, 1
	# v871 = ineg v870
	neg	rdi
	# v872 = iconst.i64 -1
	# v873 = icmp.ne v871, v872
	cmp	rdi, -1
	setne	dil
	# v874 = iconst.i32 126
	mov	r8d, 126
	# v875 = select v873, v874, v869
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v876 = iconst.i64 1
	mov	edi, 1
	# v877 = bnot v876
	not	rdi
	# v878 = iconst.i64 -2
	# v879 = icmp.ne v877, v878
	cmp	rdi, -2
	setne	dil
	# v880 = iconst.i32 127
	mov	r8d, 127
	# v881 = select v879, v880, v875
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v882 = iconst.i64 1
	mov	edi, 1
	# v883 = clz v882
	bsr	rax, rdi
	mov	ecx, 127
	cmove	rax, rcx
	xor	eax, 63
	mov	edi, eax
	# v884 = iconst.i64 63
	# v885 = icmp.ne v883, v884
	cmp	rdi, 63
	setne	dil
	# v886 = iconst.i32 128
	mov	r8d, 128
	# v887 = select v885, v886, v881
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v888 = iconst.i64 1
	mov	edi, 1
	# v889 = ctz v888
	bsf	rax, rdi
	mov	ecx, 64
	cmove	rax, rcx
	mov	rdi, rax
	# v890 = iconst.i64 0
	# v891 = icmp.ne v889, v890
	cmp	rdi, 0
	setne	dil
	# v892 = iconst.i32 129
	mov	r8d, 129
	# v893 = select v891, v892, v887
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v894 = iconst.i64 1
	mov	edi, 1
	# v895 = popcnt v894
	mov	rax, rdi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v896 = iconst.i64 1
	# v897 = icmp.ne v895, v896
	cmp	rdi, 1
	setne	dil
	# v898 = iconst.i32 130
	mov	r8d, 130
	# v899 = select v897, v898, v893
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v900 = iconst.i64 1
	mov	edi, 1
	# v901 = bswap v900
	bswap	rdi
	# v902 = iconst.i64 72057594037927936
	movabs	r8, 72057594037927936
	# v903 = icmp.ne v901, v902
	cmp	rdi, r8
	setne	dil
	# v904 = iconst.i32 131
	mov	r8d, 131
	# v905 = select v903, v904, v899
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v906 = iconst.i64 -1
	mov	rdi, -1
	# v907 = ineg v906
	neg	rdi
	# v908 = iconst.i64 1
	# v909 = icmp.ne v907, v908
	cmp	rdi, 1
	setne	dil
	# v910 = iconst.i32 132
	mov	r8d, 132
	# v911 = select v909, v910, v905
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v912 = iconst.i64 -1
	mov	rdi, -1
	# v913 = bnot v912
	not	rdi
	# v914 = iconst.i64 0
	# v915 = icmp.ne v913, v914
	cmp	rdi, 0
	setne	dil
	# v916 = iconst.i32 133
	mov	r8d, 133
	# v917 = select v915, v916, v911
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v918 = iconst.i64 -1
	mov	rdi, -1
	# v919 = clz v918
	bsr	rax, rdi
	mov	ecx, 127
	cmove	rax, rcx
	xor	eax, 63
	mov	edi, eax
	# v920 = iconst.i64 0
	# v921 = icmp.ne v919, v920
	cmp	rdi, 0
	setne	dil
	# v922 = iconst.i32 134
	mov	r8d, 134
	# v923 = select v921, v922, v917
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v924 = iconst.i64 -1
	mov	rdi, -1
	# v925 = ctz v924
	bsf	rax, rdi
	mov	ecx, 64
	cmove	rax, rcx
	mov	rdi, rax
	# v926 = iconst.i64 0
	# v927 = icmp.ne v925, v926
	cmp	rdi, 0
	setne	dil
	# v928 = iconst.i32 135
	mov	r8d, 135
	# v929 = select v927, v928, v923
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v930 = iconst.i64 -1
	mov	rdi, -1
	# v931 = popcnt v930
	mov	rax, rdi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v932 = iconst.i64 64
	# v933 = icmp.ne v931, v932
	cmp	rdi, 64
	setne	dil
	# v934 = iconst.i32 136
	mov	r8d, 136
	# v935 = select v933, v934, v929
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v936 = iconst.i64 -1
	mov	rdi, -1
	# v937 = bswap v936
	bswap	rdi
	# v938 = iconst.i64 -1
	# v939 = icmp.ne v937, v938
	cmp	rdi, -1
	setne	dil
	# v940 = iconst.i32 137
	mov	r8d, 137
	# v941 = select v939, v940, v935
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v942 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v943 = ineg v942
	neg	rdi
	# v944 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v945 = icmp.ne v943, v944
	cmp	rdi, r8
	setne	dil
	# v946 = iconst.i32 138
	mov	r8d, 138
	# v947 = select v945, v946, v941
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v948 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v949 = bnot v948
	not	rdi
	# v950 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v951 = icmp.ne v949, v950
	cmp	rdi, r8
	setne	dil
	# v952 = iconst.i32 139
	mov	r8d, 139
	# v953 = select v951, v952, v947
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v954 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v955 = clz v954
	bsr	rax, rdi
	mov	ecx, 127
	cmove	rax, rcx
	xor	eax, 63
	mov	edi, eax
	# v956 = iconst.i64 0
	# v957 = icmp.ne v955, v956
	cmp	rdi, 0
	setne	dil
	# v958 = iconst.i32 140
	mov	r8d, 140
	# v959 = select v957, v958, v953
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v960 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v961 = ctz v960
	bsf	rax, rdi
	mov	ecx, 64
	cmove	rax, rcx
	mov	rdi, rax
	# v962 = iconst.i64 63
	# v963 = icmp.ne v961, v962
	cmp	rdi, 63
	setne	dil
	# v964 = iconst.i32 141
	mov	r8d, 141
	# v965 = select v963, v964, v959
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v966 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v967 = popcnt v966
	mov	rax, rdi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v968 = iconst.i64 1
	# v969 = icmp.ne v967, v968
	cmp	rdi, 1
	setne	dil
	# v970 = iconst.i32 142
	mov	r8d, 142
	# v971 = select v969, v970, v965
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v972 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v973 = bswap v972
	bswap	rdi
	# v974 = iconst.i64 128
	# v975 = icmp.ne v973, v974
	cmp	rdi, 128
	setne	dil
	# v976 = iconst.i32 143
	mov	r8d, 143
	# v977 = select v975, v976, v971
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v978 = iconst.i64 90
	mov	edi, 90
	# v979 = ineg v978
	neg	rdi
	# v980 = iconst.i64 -90
	# v981 = icmp.ne v979, v980
	cmp	rdi, -90
	setne	dil
	# v982 = iconst.i32 144
	mov	r8d, 144
	# v983 = select v981, v982, v977
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v984 = iconst.i64 90
	mov	edi, 90
	# v985 = bnot v984
	not	rdi
	# v986 = iconst.i64 -91
	# v987 = icmp.ne v985, v986
	cmp	rdi, -91
	setne	dil
	# v988 = iconst.i32 145
	mov	r8d, 145
	# v989 = select v987, v988, v983
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v990 = iconst.i64 90
	mov	edi, 90
	# v991 = clz v990
	bsr	rax, rdi
	mov	ecx, 127
	cmove	rax, rcx
	xor	eax, 63
	mov	edi, eax
	# v992 = iconst.i64 57
	# v993 = icmp.ne v991, v992
	cmp	rdi, 57
	setne	dil
	# v994 = iconst.i32 146
	mov	r8d, 146
	# v995 = select v993, v994, v989
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v996 = iconst.i64 90
	mov	edi, 90
	# v997 = ctz v996
	bsf	rax, rdi
	mov	ecx, 64
	cmove	rax, rcx
	mov	rdi, rax
	# v998 = iconst.i64 1
	# v999 = icmp.ne v997, v998
	cmp	rdi, 1
	setne	dil
	# v1000 = iconst.i32 147
	mov	r8d, 147
	# v1001 = select v999, v1000, v995
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1002 = iconst.i64 90
	mov	edi, 90
	# v1003 = popcnt v1002
	mov	rax, rdi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v1004 = iconst.i64 4
	# v1005 = icmp.ne v1003, v1004
	cmp	rdi, 4
	setne	dil
	# v1006 = iconst.i32 148
	mov	r8d, 148
	# v1007 = select v1005, v1006, v1001
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1008 = iconst.i64 90
	mov	edi, 90
	# v1009 = bswap v1008
	bswap	rdi
	# v1010 = iconst.i64 6485183463413514240
	movabs	r8, 6485183463413514240
	# v1011 = icmp.ne v1009, v1010
	cmp	rdi, r8
	setne	dil
	# v1012 = iconst.i32 149
	mov	r8d, 149
	# v1013 = select v1011, v1012, v1007
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1014 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v1015 = ineg v1014
	neg	rdi
	# v1016 = iconst.i64 -81985529216486895
	movabs	r8, 18364758544493064721
	# v1017 = icmp.ne v1015, v1016
	cmp	rdi, r8
	setne	dil
	# v1018 = iconst.i32 150
	mov	r8d, 150
	# v1019 = select v1017, v1018, v1013
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1020 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v1021 = bnot v1020
	not	rdi
	# v1022 = iconst.i64 -81985529216486896
	movabs	r8, 18364758544493064720
	# v1023 = icmp.ne v1021, v1022
	cmp	rdi, r8
	setne	dil
	# v1024 = iconst.i32 151
	mov	r8d, 151
	# v1025 = select v1023, v1024, v1019
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1026 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v1027 = clz v1026
	bsr	rax, rdi
	mov	ecx, 127
	cmove	rax, rcx
	xor	eax, 63
	mov	edi, eax
	# v1028 = iconst.i64 7
	# v1029 = icmp.ne v1027, v1028
	cmp	rdi, 7
	setne	dil
	# v1030 = iconst.i32 152
	mov	r8d, 152
	# v1031 = select v1029, v1030, v1025
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1032 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v1033 = ctz v1032
	bsf	rax, rdi
	mov	ecx, 64
	cmove	rax, rcx
	mov	rdi, rax
	# v1034 = iconst.i64 0
	# v1035 = icmp.ne v1033, v1034
	cmp	rdi, 0
	setne	dil
	# v1036 = iconst.i32 153
	mov	r8d, 153
	# v1037 = select v1035, v1036, v1031
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1038 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v1039 = popcnt v1038
	mov	rax, rdi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v1040 = iconst.i64 32
	# v1041 = icmp.ne v1039, v1040
	cmp	rdi, 32
	setne	dil
	# v1042 = iconst.i32 154
	mov	r8d, 154
	# v1043 = select v1041, v1042, v1037
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1044 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v1045 = bswap v1044
	bswap	rdi
	# v1046 = iconst.i64 -1167088121787636991
	movabs	r8, 17279655951921914625
	# v1047 = icmp.ne v1045, v1046
	cmp	rdi, r8
	setne	dil
	# v1048 = iconst.i32 155
	mov	r8d, 155
	# v1049 = select v1047, v1048, v1043
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1050 = iconst.i64 4294967296
	movabs	rdi, 4294967296
	# v1051 = ineg v1050
	neg	rdi
	# v1052 = iconst.i64 -4294967296
	movabs	r8, 18446744069414584320
	# v1053 = icmp.ne v1051, v1052
	cmp	rdi, r8
	setne	dil
	# v1054 = iconst.i32 156
	mov	r8d, 156
	# v1055 = select v1053, v1054, v1049
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1056 = iconst.i64 4294967296
	movabs	rdi, 4294967296
	# v1057 = bnot v1056
	not	rdi
	# v1058 = iconst.i64 -4294967297
	movabs	r8, 18446744069414584319
	# v1059 = icmp.ne v1057, v1058
	cmp	rdi, r8
	setne	dil
	# v1060 = iconst.i32 157
	mov	r8d, 157
	# v1061 = select v1059, v1060, v1055
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1062 = iconst.i64 4294967296
	movabs	rdi, 4294967296
	# v1063 = clz v1062
	bsr	rax, rdi
	mov	ecx, 127
	cmove	rax, rcx
	xor	eax, 63
	mov	edi, eax
	# v1064 = iconst.i64 31
	# v1065 = icmp.ne v1063, v1064
	cmp	rdi, 31
	setne	dil
	# v1066 = iconst.i32 158
	mov	r8d, 158
	# v1067 = select v1065, v1066, v1061
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1068 = iconst.i64 4294967296
	movabs	rdi, 4294967296
	# v1069 = ctz v1068
	bsf	rax, rdi
	mov	ecx, 64
	cmove	rax, rcx
	mov	rdi, rax
	# v1070 = iconst.i64 32
	# v1071 = icmp.ne v1069, v1070
	cmp	rdi, 32
	setne	dil
	# v1072 = iconst.i32 159
	mov	r8d, 159
	# v1073 = select v1071, v1072, v1067
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1074 = iconst.i64 4294967296
	movabs	rdi, 4294967296
	# v1075 = popcnt v1074
	mov	rax, rdi
	mov	rcx, rax
	shr	rcx, 1
	movabs	rdx, 6148914691236517205
	and	rcx, rdx
	sub	rax, rcx
	mov	rcx, rax
	shr	rcx, 2
	movabs	rdx, 3689348814741910323
	and	rax, rdx
	and	rcx, rdx
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 4
	add	rax, rcx
	movabs	rdx, 1085102592571150095
	and	rax, rdx
	mov	rcx, rax
	shr	rcx, 8
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 16
	add	rax, rcx
	mov	rcx, rax
	shr	rcx, 32
	add	rax, rcx
	and	eax, 127
	mov	edi, eax
	# v1076 = iconst.i64 1
	# v1077 = icmp.ne v1075, v1076
	cmp	rdi, 1
	setne	dil
	# v1078 = iconst.i32 160
	mov	r8d, 160
	# v1079 = select v1077, v1078, v1073
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1080 = iconst.i64 4294967296
	movabs	rdi, 4294967296
	# v1081 = bswap v1080
	bswap	rdi
	# v1082 = iconst.i64 16777216
	# v1083 = icmp.ne v1081, v1082
	cmp	rdi, 16777216
	setne	dil
	# v1084 = iconst.i32 161
	mov	r8d, 161
	# v1085 = select v1083, v1084, v1079
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# return v1085
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
