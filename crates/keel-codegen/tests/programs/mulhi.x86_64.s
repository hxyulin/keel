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
	# v3 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v4 = trunc.i8 v3
	# v5 = umulhi v2, v4
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v6 = iconst.i8 0
	# v7 = icmp.ne v5, v6
	cmp	dil, 0
	setne	dil
	# v8 = iconst.i32 1
	mov	r8d, 1
	# v9 = select v7, v8, v0
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v10 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v11 = trunc.i8 v10
	# v12 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v13 = trunc.i8 v12
	# v14 = smulhi v11, v13
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v15 = iconst.i8 0
	# v16 = icmp.ne v14, v15
	cmp	dil, 0
	setne	dil
	# v17 = iconst.i32 2
	mov	r8d, 2
	# v18 = select v16, v17, v9
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v19 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v20 = trunc.i8 v19
	# v21 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v22 = trunc.i8 v21
	# v23 = umulhi v20, v22
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v24 = iconst.i8 0
	# v25 = icmp.ne v23, v24
	cmp	dil, 0
	setne	dil
	# v26 = iconst.i32 3
	mov	r8d, 3
	# v27 = select v25, v26, v18
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v28 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v29 = trunc.i8 v28
	# v30 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v31 = trunc.i8 v30
	# v32 = smulhi v29, v31
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v33 = iconst.i8 0
	# v34 = icmp.ne v32, v33
	cmp	dil, 0
	setne	dil
	# v35 = iconst.i32 4
	mov	r8d, 4
	# v36 = select v34, v35, v27
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v37 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v38 = trunc.i8 v37
	# v39 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v40 = trunc.i8 v39
	# v41 = umulhi v38, v40
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v42 = iconst.i8 0
	# v43 = icmp.ne v41, v42
	cmp	dil, 0
	setne	dil
	# v44 = iconst.i32 5
	mov	r8d, 5
	# v45 = select v43, v44, v36
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v46 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v47 = trunc.i8 v46
	# v48 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v49 = trunc.i8 v48
	# v50 = smulhi v47, v49
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v51 = iconst.i8 0
	# v52 = icmp.ne v50, v51
	cmp	dil, 0
	setne	dil
	# v53 = iconst.i32 6
	mov	r8d, 6
	# v54 = select v52, v53, v45
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v55 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v56 = trunc.i8 v55
	# v57 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v58 = trunc.i8 v57
	# v59 = umulhi v56, v58
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v60 = iconst.i8 0
	# v61 = icmp.ne v59, v60
	cmp	dil, 0
	setne	dil
	# v62 = iconst.i32 7
	mov	r8d, 7
	# v63 = select v61, v62, v54
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v64 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v65 = trunc.i8 v64
	# v66 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v67 = trunc.i8 v66
	# v68 = smulhi v65, v67
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v69 = iconst.i8 0
	# v70 = icmp.ne v68, v69
	cmp	dil, 0
	setne	dil
	# v71 = iconst.i32 8
	mov	r8d, 8
	# v72 = select v70, v71, v63
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v73 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v74 = trunc.i8 v73
	# v75 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v76 = trunc.i8 v75
	# v77 = umulhi v74, v76
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v78 = iconst.i8 0
	# v79 = icmp.ne v77, v78
	cmp	dil, 0
	setne	dil
	# v80 = iconst.i32 9
	mov	r8d, 9
	# v81 = select v79, v80, v72
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v82 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v83 = trunc.i8 v82
	# v84 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v85 = trunc.i8 v84
	# v86 = smulhi v83, v85
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v87 = iconst.i8 -1
	# v88 = icmp.ne v86, v87
	cmp	dil, 255
	setne	dil
	# v89 = iconst.i32 10
	mov	r8d, 10
	# v90 = select v88, v89, v81
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v91 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v92 = trunc.i8 v91
	# v93 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v94 = trunc.i8 v93
	# v95 = umulhi v92, v94
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v96 = iconst.i8 0
	# v97 = icmp.ne v95, v96
	cmp	dil, 0
	setne	dil
	# v98 = iconst.i32 11
	mov	r8d, 11
	# v99 = select v97, v98, v90
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v100 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v101 = trunc.i8 v100
	# v102 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v103 = trunc.i8 v102
	# v104 = smulhi v101, v103
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v105 = iconst.i8 -1
	# v106 = icmp.ne v104, v105
	cmp	dil, 255
	setne	dil
	# v107 = iconst.i32 12
	mov	r8d, 12
	# v108 = select v106, v107, v99
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v109 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v110 = trunc.i8 v109
	# v111 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v112 = trunc.i8 v111
	# v113 = umulhi v110, v112
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v114 = iconst.i8 0
	# v115 = icmp.ne v113, v114
	cmp	dil, 0
	setne	dil
	# v116 = iconst.i32 13
	mov	r8d, 13
	# v117 = select v115, v116, v108
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v118 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v119 = trunc.i8 v118
	# v120 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v121 = trunc.i8 v120
	# v122 = smulhi v119, v121
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v123 = iconst.i8 0
	# v124 = icmp.ne v122, v123
	cmp	dil, 0
	setne	dil
	# v125 = iconst.i32 14
	mov	r8d, 14
	# v126 = select v124, v125, v117
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v127 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v128 = trunc.i8 v127
	# v129 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v130 = trunc.i8 v129
	# v131 = umulhi v128, v130
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v132 = iconst.i8 0
	# v133 = icmp.ne v131, v132
	cmp	dil, 0
	setne	dil
	# v134 = iconst.i32 15
	mov	r8d, 15
	# v135 = select v133, v134, v126
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v136 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v137 = trunc.i8 v136
	# v138 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v139 = trunc.i8 v138
	# v140 = smulhi v137, v139
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v141 = iconst.i8 0
	# v142 = icmp.ne v140, v141
	cmp	dil, 0
	setne	dil
	# v143 = iconst.i32 16
	mov	r8d, 16
	# v144 = select v142, v143, v135
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v145 = iconst.i64 -6502179613573254142
	movabs	rdi, 11944564460136297474
	# v146 = trunc.i8 v145
	# v147 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v148 = trunc.i8 v147
	# v149 = umulhi v146, v148
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v150 = iconst.i8 1
	# v151 = icmp.ne v149, v150
	cmp	dil, 1
	setne	dil
	# v152 = iconst.i32 17
	mov	r8d, 17
	# v153 = select v151, v152, v144
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v154 = iconst.i64 -6502179613573254142
	movabs	rdi, 11944564460136297474
	# v155 = trunc.i8 v154
	# v156 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v157 = trunc.i8 v156
	# v158 = smulhi v155, v157
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v159 = iconst.i8 -1
	# v160 = icmp.ne v158, v159
	cmp	dil, 255
	setne	dil
	# v161 = iconst.i32 18
	mov	r8d, 18
	# v162 = select v160, v161, v153
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v163 = iconst.i64 -6502179613573254142
	movabs	rdi, 11944564460136297474
	# v164 = trunc.i8 v163
	# v165 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v166 = trunc.i8 v165
	# v167 = umulhi v164, v166
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v168 = iconst.i8 1
	# v169 = icmp.ne v167, v168
	cmp	dil, 1
	setne	dil
	# v170 = iconst.i32 19
	mov	r8d, 19
	# v171 = select v169, v170, v162
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v172 = iconst.i64 -6502179613573254142
	movabs	rdi, 11944564460136297474
	# v173 = trunc.i8 v172
	# v174 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v175 = trunc.i8 v174
	# v176 = smulhi v173, v175
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v177 = iconst.i8 -1
	# v178 = icmp.ne v176, v177
	cmp	dil, 255
	setne	dil
	# v179 = iconst.i32 20
	mov	r8d, 20
	# v180 = select v178, v179, v171
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v181 = iconst.i64 -6502179613573254142
	movabs	rdi, 11944564460136297474
	# v182 = trunc.i8 v181
	# v183 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v184 = trunc.i8 v183
	# v185 = umulhi v182, v184
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v186 = iconst.i8 0
	# v187 = icmp.ne v185, v186
	cmp	dil, 0
	setne	dil
	# v188 = iconst.i32 21
	mov	r8d, 21
	# v189 = select v187, v188, v180
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v190 = iconst.i64 -6502179613573254142
	movabs	rdi, 11944564460136297474
	# v191 = trunc.i8 v190
	# v192 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v193 = trunc.i8 v192
	# v194 = smulhi v191, v193
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v195 = iconst.i8 0
	# v196 = icmp.ne v194, v195
	cmp	dil, 0
	setne	dil
	# v197 = iconst.i32 22
	mov	r8d, 22
	# v198 = select v196, v197, v189
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v199 = iconst.i64 -6502179613573254142
	movabs	rdi, 11944564460136297474
	# v200 = trunc.i8 v199
	# v201 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v202 = trunc.i8 v201
	# v203 = umulhi v200, v202
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v204 = iconst.i8 0
	# v205 = icmp.ne v203, v204
	cmp	dil, 0
	setne	dil
	# v206 = iconst.i32 23
	mov	r8d, 23
	# v207 = select v205, v206, v198
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v208 = iconst.i64 -6502179613573254142
	movabs	rdi, 11944564460136297474
	# v209 = trunc.i8 v208
	# v210 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v211 = trunc.i8 v210
	# v212 = smulhi v209, v211
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v213 = iconst.i8 0
	# v214 = icmp.ne v212, v213
	cmp	dil, 0
	setne	dil
	# v215 = iconst.i32 24
	mov	r8d, 24
	# v216 = select v214, v215, v207
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v217 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v218 = trunc.i8 v217
	# v219 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v220 = trunc.i8 v219
	# v221 = umulhi v218, v220
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v222 = iconst.i8 -2
	# v223 = icmp.ne v221, v222
	cmp	dil, 254
	setne	dil
	# v224 = iconst.i32 25
	mov	r8d, 25
	# v225 = select v223, v224, v216
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v226 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v227 = trunc.i8 v226
	# v228 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v229 = trunc.i8 v228
	# v230 = smulhi v227, v229
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v231 = iconst.i8 0
	# v232 = icmp.ne v230, v231
	cmp	dil, 0
	setne	dil
	# v233 = iconst.i32 26
	mov	r8d, 26
	# v234 = select v232, v233, v225
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v235 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v236 = trunc.i8 v235
	# v237 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v238 = trunc.i8 v237
	# v239 = umulhi v236, v238
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v240 = iconst.i8 127
	# v241 = icmp.ne v239, v240
	cmp	dil, 127
	setne	dil
	# v242 = iconst.i32 27
	mov	r8d, 27
	# v243 = select v241, v242, v234
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v244 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v245 = trunc.i8 v244
	# v246 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v247 = trunc.i8 v246
	# v248 = smulhi v245, v247
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v249 = iconst.i8 0
	# v250 = icmp.ne v248, v249
	cmp	dil, 0
	setne	dil
	# v251 = iconst.i32 28
	mov	r8d, 28
	# v252 = select v250, v251, v243
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v253 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v254 = trunc.i8 v253
	# v255 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v256 = trunc.i8 v255
	# v257 = umulhi v254, v256
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v258 = iconst.i8 2
	# v259 = icmp.ne v257, v258
	cmp	dil, 2
	setne	dil
	# v260 = iconst.i32 29
	mov	r8d, 29
	# v261 = select v259, v260, v252
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v262 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v263 = trunc.i8 v262
	# v264 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v265 = trunc.i8 v264
	# v266 = smulhi v263, v265
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v267 = iconst.i8 -1
	# v268 = icmp.ne v266, v267
	cmp	dil, 255
	setne	dil
	# v269 = iconst.i32 30
	mov	r8d, 30
	# v270 = select v268, v269, v261
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v271 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v272 = trunc.i8 v271
	# v273 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v274 = trunc.i8 v273
	# v275 = umulhi v272, v274
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v276 = iconst.i8 126
	# v277 = icmp.ne v275, v276
	cmp	dil, 126
	setne	dil
	# v278 = iconst.i32 31
	mov	r8d, 31
	# v279 = select v277, v278, v270
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v280 = iconst.i64 -6502179613573253889
	movabs	rdi, 11944564460136297727
	# v281 = trunc.i8 v280
	# v282 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v283 = trunc.i8 v282
	# v284 = smulhi v281, v283
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v285 = iconst.i8 -1
	# v286 = icmp.ne v284, v285
	cmp	dil, 255
	setne	dil
	# v287 = iconst.i32 32
	mov	r8d, 32
	# v288 = select v286, v287, v279
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v289 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v290 = trunc.i8 v289
	# v291 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v292 = trunc.i8 v291
	# v293 = umulhi v290, v292
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v294 = iconst.i8 127
	# v295 = icmp.ne v293, v294
	cmp	dil, 127
	setne	dil
	# v296 = iconst.i32 33
	mov	r8d, 33
	# v297 = select v295, v296, v288
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v298 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v299 = trunc.i8 v298
	# v300 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v301 = trunc.i8 v300
	# v302 = smulhi v299, v301
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v303 = iconst.i8 0
	# v304 = icmp.ne v302, v303
	cmp	dil, 0
	setne	dil
	# v305 = iconst.i32 34
	mov	r8d, 34
	# v306 = select v304, v305, v297
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v307 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v308 = trunc.i8 v307
	# v309 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v310 = trunc.i8 v309
	# v311 = umulhi v308, v310
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v312 = iconst.i8 64
	# v313 = icmp.ne v311, v312
	cmp	dil, 64
	setne	dil
	# v314 = iconst.i32 35
	mov	r8d, 35
	# v315 = select v313, v314, v306
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v316 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v317 = trunc.i8 v316
	# v318 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v319 = trunc.i8 v318
	# v320 = smulhi v317, v319
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v321 = iconst.i8 64
	# v322 = icmp.ne v320, v321
	cmp	dil, 64
	setne	dil
	# v323 = iconst.i32 36
	mov	r8d, 36
	# v324 = select v322, v323, v315
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v325 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v326 = trunc.i8 v325
	# v327 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v328 = trunc.i8 v327
	# v329 = umulhi v326, v328
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v330 = iconst.i8 1
	# v331 = icmp.ne v329, v330
	cmp	dil, 1
	setne	dil
	# v332 = iconst.i32 37
	mov	r8d, 37
	# v333 = select v331, v332, v324
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v334 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v335 = trunc.i8 v334
	# v336 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v337 = trunc.i8 v336
	# v338 = smulhi v335, v337
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v339 = iconst.i8 -2
	# v340 = icmp.ne v338, v339
	cmp	dil, 254
	setne	dil
	# v341 = iconst.i32 38
	mov	r8d, 38
	# v342 = select v340, v341, v333
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v343 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v344 = trunc.i8 v343
	# v345 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v346 = trunc.i8 v345
	# v347 = umulhi v344, v346
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v348 = iconst.i8 63
	# v349 = icmp.ne v347, v348
	cmp	dil, 63
	setne	dil
	# v350 = iconst.i32 39
	mov	r8d, 39
	# v351 = select v349, v350, v342
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v352 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v353 = trunc.i8 v352
	# v354 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v355 = trunc.i8 v354
	# v356 = smulhi v353, v355
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v357 = iconst.i8 -64
	# v358 = icmp.ne v356, v357
	cmp	dil, 192
	setne	dil
	# v359 = iconst.i32 40
	mov	r8d, 40
	# v360 = select v358, v359, v351
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v361 = iconst.i64 -6502179613573254017
	movabs	rdi, 11944564460136297599
	# v362 = trunc.i8 v361
	# v363 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v364 = trunc.i8 v363
	# v365 = umulhi v362, v364
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v366 = iconst.i8 126
	# v367 = icmp.ne v365, v366
	cmp	dil, 126
	setne	dil
	# v368 = iconst.i32 41
	mov	r8d, 41
	# v369 = select v367, v368, v360
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v370 = iconst.i64 -6502179613573254017
	movabs	rdi, 11944564460136297599
	# v371 = trunc.i8 v370
	# v372 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v373 = trunc.i8 v372
	# v374 = smulhi v371, v373
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v375 = iconst.i8 -1
	# v376 = icmp.ne v374, v375
	cmp	dil, 255
	setne	dil
	# v377 = iconst.i32 42
	mov	r8d, 42
	# v378 = select v376, v377, v369
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v379 = iconst.i64 -6502179613573254017
	movabs	rdi, 11944564460136297599
	# v380 = trunc.i8 v379
	# v381 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v382 = trunc.i8 v381
	# v383 = umulhi v380, v382
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v384 = iconst.i8 63
	# v385 = icmp.ne v383, v384
	cmp	dil, 63
	setne	dil
	# v386 = iconst.i32 43
	mov	r8d, 43
	# v387 = select v385, v386, v378
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v388 = iconst.i64 -6502179613573254017
	movabs	rdi, 11944564460136297599
	# v389 = trunc.i8 v388
	# v390 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v391 = trunc.i8 v390
	# v392 = smulhi v389, v391
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v393 = iconst.i8 -64
	# v394 = icmp.ne v392, v393
	cmp	dil, 192
	setne	dil
	# v395 = iconst.i32 44
	mov	r8d, 44
	# v396 = select v394, v395, v387
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v397 = iconst.i64 -6502179613573254017
	movabs	rdi, 11944564460136297599
	# v398 = trunc.i8 v397
	# v399 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v400 = trunc.i8 v399
	# v401 = umulhi v398, v400
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v402 = iconst.i8 1
	# v403 = icmp.ne v401, v402
	cmp	dil, 1
	setne	dil
	# v404 = iconst.i32 45
	mov	r8d, 45
	# v405 = select v403, v404, v396
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v406 = iconst.i64 -6502179613573254017
	movabs	rdi, 11944564460136297599
	# v407 = trunc.i8 v406
	# v408 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v409 = trunc.i8 v408
	# v410 = smulhi v407, v409
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v411 = iconst.i8 1
	# v412 = icmp.ne v410, v411
	cmp	dil, 1
	setne	dil
	# v413 = iconst.i32 46
	mov	r8d, 46
	# v414 = select v412, v413, v405
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v415 = iconst.i64 -6502179613573254017
	movabs	rdi, 11944564460136297599
	# v416 = trunc.i8 v415
	# v417 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v418 = trunc.i8 v417
	# v419 = umulhi v416, v418
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v420 = iconst.i8 63
	# v421 = icmp.ne v419, v420
	cmp	dil, 63
	setne	dil
	# v422 = iconst.i32 47
	mov	r8d, 47
	# v423 = select v421, v422, v414
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v424 = iconst.i64 -6502179613573254017
	movabs	rdi, 11944564460136297599
	# v425 = trunc.i8 v424
	# v426 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v427 = trunc.i8 v426
	# v428 = smulhi v425, v427
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v429 = iconst.i8 63
	# v430 = icmp.ne v428, v429
	cmp	dil, 63
	setne	dil
	# v431 = iconst.i32 48
	mov	r8d, 48
	# v432 = select v430, v431, v423
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v433 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v434 = trunc.i8 v433
	# v435 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v436 = trunc.i8 v435
	# v437 = umulhi v434, v436
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v438 = iconst.i8 89
	# v439 = icmp.ne v437, v438
	cmp	dil, 89
	setne	dil
	# v440 = iconst.i32 49
	mov	r8d, 49
	# v441 = select v439, v440, v432
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v442 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v443 = trunc.i8 v442
	# v444 = iconst.i64 -6502179613573253889
	movabs	r8, 11944564460136297727
	# v445 = trunc.i8 v444
	# v446 = smulhi v443, v445
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v447 = iconst.i8 -1
	# v448 = icmp.ne v446, v447
	cmp	dil, 255
	setne	dil
	# v449 = iconst.i32 50
	mov	r8d, 50
	# v450 = select v448, v449, v441
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v451 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v452 = trunc.i8 v451
	# v453 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v454 = trunc.i8 v453
	# v455 = umulhi v452, v454
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v456 = iconst.i8 45
	# v457 = icmp.ne v455, v456
	cmp	dil, 45
	setne	dil
	# v458 = iconst.i32 51
	mov	r8d, 51
	# v459 = select v457, v458, v450
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v460 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v461 = trunc.i8 v460
	# v462 = iconst.i64 -6502179613573254016
	movabs	r8, 11944564460136297600
	# v463 = trunc.i8 v462
	# v464 = smulhi v461, v463
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v465 = iconst.i8 -45
	# v466 = icmp.ne v464, v465
	cmp	dil, 211
	setne	dil
	# v467 = iconst.i32 52
	mov	r8d, 52
	# v468 = select v466, v467, v459
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v469 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v470 = trunc.i8 v469
	# v471 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v472 = trunc.i8 v471
	# v473 = umulhi v470, v472
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v474 = iconst.i8 1
	# v475 = icmp.ne v473, v474
	cmp	dil, 1
	setne	dil
	# v476 = iconst.i32 53
	mov	r8d, 53
	# v477 = select v475, v476, v468
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v478 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v479 = trunc.i8 v478
	# v480 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v481 = trunc.i8 v480
	# v482 = smulhi v479, v481
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v483 = iconst.i8 1
	# v484 = icmp.ne v482, v483
	cmp	dil, 1
	setne	dil
	# v485 = iconst.i32 54
	mov	r8d, 54
	# v486 = select v484, v485, v477
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v487 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v488 = trunc.i8 v487
	# v489 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v490 = trunc.i8 v489
	# v491 = umulhi v488, v490
	movzx	eax, dil
	movzx	ecx, r8b
	imul	eax, ecx
	shr	eax, 8
	mov	edi, eax
	# v492 = iconst.i8 44
	# v493 = icmp.ne v491, v492
	cmp	dil, 44
	setne	dil
	# v494 = iconst.i32 55
	mov	r8d, 55
	# v495 = select v493, v494, v486
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v496 = iconst.i64 -6502179613573254054
	movabs	rdi, 11944564460136297562
	# v497 = trunc.i8 v496
	# v498 = iconst.i64 -6502179613573254017
	movabs	r8, 11944564460136297599
	# v499 = trunc.i8 v498
	# v500 = smulhi v497, v499
	movsx	eax, dil
	movsx	ecx, r8b
	imul	eax, ecx
	sar	eax, 8
	mov	edi, eax
	# v501 = iconst.i8 44
	# v502 = icmp.ne v500, v501
	cmp	dil, 44
	setne	dil
	# v503 = iconst.i32 56
	mov	r8d, 56
	# v504 = select v502, v503, v495
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v505 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v506 = trunc.i16 v505
	# v507 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v508 = trunc.i16 v507
	# v509 = umulhi v506, v508
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v510 = iconst.i16 0
	# v511 = icmp.ne v509, v510
	cmp	di, 0
	setne	dil
	# v512 = iconst.i32 57
	mov	r8d, 57
	# v513 = select v511, v512, v504
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v514 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v515 = trunc.i16 v514
	# v516 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v517 = trunc.i16 v516
	# v518 = smulhi v515, v517
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v519 = iconst.i16 0
	# v520 = icmp.ne v518, v519
	cmp	di, 0
	setne	dil
	# v521 = iconst.i32 58
	mov	r8d, 58
	# v522 = select v520, v521, v513
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v523 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v524 = trunc.i16 v523
	# v525 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v526 = trunc.i16 v525
	# v527 = umulhi v524, v526
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v528 = iconst.i16 0
	# v529 = icmp.ne v527, v528
	cmp	di, 0
	setne	dil
	# v530 = iconst.i32 59
	mov	r8d, 59
	# v531 = select v529, v530, v522
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v532 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v533 = trunc.i16 v532
	# v534 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v535 = trunc.i16 v534
	# v536 = smulhi v533, v535
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v537 = iconst.i16 0
	# v538 = icmp.ne v536, v537
	cmp	di, 0
	setne	dil
	# v539 = iconst.i32 60
	mov	r8d, 60
	# v540 = select v538, v539, v531
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v541 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v542 = trunc.i16 v541
	# v543 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v544 = trunc.i16 v543
	# v545 = umulhi v542, v544
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v546 = iconst.i16 0
	# v547 = icmp.ne v545, v546
	cmp	di, 0
	setne	dil
	# v548 = iconst.i32 61
	mov	r8d, 61
	# v549 = select v547, v548, v540
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v550 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v551 = trunc.i16 v550
	# v552 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v553 = trunc.i16 v552
	# v554 = smulhi v551, v553
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v555 = iconst.i16 0
	# v556 = icmp.ne v554, v555
	cmp	di, 0
	setne	dil
	# v557 = iconst.i32 62
	mov	r8d, 62
	# v558 = select v556, v557, v549
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v559 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v560 = trunc.i16 v559
	# v561 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v562 = trunc.i16 v561
	# v563 = umulhi v560, v562
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v564 = iconst.i16 0
	# v565 = icmp.ne v563, v564
	cmp	di, 0
	setne	dil
	# v566 = iconst.i32 63
	mov	r8d, 63
	# v567 = select v565, v566, v558
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v568 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v569 = trunc.i16 v568
	# v570 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v571 = trunc.i16 v570
	# v572 = smulhi v569, v571
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v573 = iconst.i16 0
	# v574 = icmp.ne v572, v573
	cmp	di, 0
	setne	dil
	# v575 = iconst.i32 64
	mov	r8d, 64
	# v576 = select v574, v575, v567
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v577 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v578 = trunc.i16 v577
	# v579 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v580 = trunc.i16 v579
	# v581 = umulhi v578, v580
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v582 = iconst.i16 0
	# v583 = icmp.ne v581, v582
	cmp	di, 0
	setne	dil
	# v584 = iconst.i32 65
	mov	r8d, 65
	# v585 = select v583, v584, v576
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v586 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v587 = trunc.i16 v586
	# v588 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v589 = trunc.i16 v588
	# v590 = smulhi v587, v589
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v591 = iconst.i16 -1
	# v592 = icmp.ne v590, v591
	cmp	di, 65535
	setne	dil
	# v593 = iconst.i32 66
	mov	r8d, 66
	# v594 = select v592, v593, v585
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v595 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v596 = trunc.i16 v595
	# v597 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v598 = trunc.i16 v597
	# v599 = umulhi v596, v598
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v600 = iconst.i16 0
	# v601 = icmp.ne v599, v600
	cmp	di, 0
	setne	dil
	# v602 = iconst.i32 67
	mov	r8d, 67
	# v603 = select v601, v602, v594
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v604 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v605 = trunc.i16 v604
	# v606 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v607 = trunc.i16 v606
	# v608 = smulhi v605, v607
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v609 = iconst.i16 -1
	# v610 = icmp.ne v608, v609
	cmp	di, 65535
	setne	dil
	# v611 = iconst.i32 68
	mov	r8d, 68
	# v612 = select v610, v611, v603
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v613 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v614 = trunc.i16 v613
	# v615 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v616 = trunc.i16 v615
	# v617 = umulhi v614, v616
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v618 = iconst.i16 0
	# v619 = icmp.ne v617, v618
	cmp	di, 0
	setne	dil
	# v620 = iconst.i32 69
	mov	r8d, 69
	# v621 = select v619, v620, v612
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v622 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v623 = trunc.i16 v622
	# v624 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v625 = trunc.i16 v624
	# v626 = smulhi v623, v625
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v627 = iconst.i16 0
	# v628 = icmp.ne v626, v627
	cmp	di, 0
	setne	dil
	# v629 = iconst.i32 70
	mov	r8d, 70
	# v630 = select v628, v629, v621
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v631 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v632 = trunc.i16 v631
	# v633 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v634 = trunc.i16 v633
	# v635 = umulhi v632, v634
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v636 = iconst.i16 0
	# v637 = icmp.ne v635, v636
	cmp	di, 0
	setne	dil
	# v638 = iconst.i32 71
	mov	r8d, 71
	# v639 = select v637, v638, v630
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v640 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v641 = trunc.i16 v640
	# v642 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v643 = trunc.i16 v642
	# v644 = smulhi v641, v643
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v645 = iconst.i16 0
	# v646 = icmp.ne v644, v645
	cmp	di, 0
	setne	dil
	# v647 = iconst.i32 72
	mov	r8d, 72
	# v648 = select v646, v647, v639
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v649 = iconst.i64 -6502179613573316606
	movabs	rdi, 11944564460136235010
	# v650 = trunc.i16 v649
	# v651 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v652 = trunc.i16 v651
	# v653 = umulhi v650, v652
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v654 = iconst.i16 1
	# v655 = icmp.ne v653, v654
	cmp	di, 1
	setne	dil
	# v656 = iconst.i32 73
	mov	r8d, 73
	# v657 = select v655, v656, v648
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v658 = iconst.i64 -6502179613573316606
	movabs	rdi, 11944564460136235010
	# v659 = trunc.i16 v658
	# v660 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v661 = trunc.i16 v660
	# v662 = smulhi v659, v661
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v663 = iconst.i16 -1
	# v664 = icmp.ne v662, v663
	cmp	di, 65535
	setne	dil
	# v665 = iconst.i32 74
	mov	r8d, 74
	# v666 = select v664, v665, v657
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v667 = iconst.i64 -6502179613573316606
	movabs	rdi, 11944564460136235010
	# v668 = trunc.i16 v667
	# v669 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v670 = trunc.i16 v669
	# v671 = umulhi v668, v670
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v672 = iconst.i16 1
	# v673 = icmp.ne v671, v672
	cmp	di, 1
	setne	dil
	# v674 = iconst.i32 75
	mov	r8d, 75
	# v675 = select v673, v674, v666
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v676 = iconst.i64 -6502179613573316606
	movabs	rdi, 11944564460136235010
	# v677 = trunc.i16 v676
	# v678 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v679 = trunc.i16 v678
	# v680 = smulhi v677, v679
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v681 = iconst.i16 -1
	# v682 = icmp.ne v680, v681
	cmp	di, 65535
	setne	dil
	# v683 = iconst.i32 76
	mov	r8d, 76
	# v684 = select v682, v683, v675
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v685 = iconst.i64 -6502179613573316606
	movabs	rdi, 11944564460136235010
	# v686 = trunc.i16 v685
	# v687 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v688 = trunc.i16 v687
	# v689 = umulhi v686, v688
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v690 = iconst.i16 0
	# v691 = icmp.ne v689, v690
	cmp	di, 0
	setne	dil
	# v692 = iconst.i32 77
	mov	r8d, 77
	# v693 = select v691, v692, v684
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v694 = iconst.i64 -6502179613573316606
	movabs	rdi, 11944564460136235010
	# v695 = trunc.i16 v694
	# v696 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v697 = trunc.i16 v696
	# v698 = smulhi v695, v697
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v699 = iconst.i16 0
	# v700 = icmp.ne v698, v699
	cmp	di, 0
	setne	dil
	# v701 = iconst.i32 78
	mov	r8d, 78
	# v702 = select v700, v701, v693
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v703 = iconst.i64 -6502179613573316606
	movabs	rdi, 11944564460136235010
	# v704 = trunc.i16 v703
	# v705 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v706 = trunc.i16 v705
	# v707 = umulhi v704, v706
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v708 = iconst.i16 0
	# v709 = icmp.ne v707, v708
	cmp	di, 0
	setne	dil
	# v710 = iconst.i32 79
	mov	r8d, 79
	# v711 = select v709, v710, v702
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v712 = iconst.i64 -6502179613573316606
	movabs	rdi, 11944564460136235010
	# v713 = trunc.i16 v712
	# v714 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v715 = trunc.i16 v714
	# v716 = smulhi v713, v715
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v717 = iconst.i16 0
	# v718 = icmp.ne v716, v717
	cmp	di, 0
	setne	dil
	# v719 = iconst.i32 80
	mov	r8d, 80
	# v720 = select v718, v719, v711
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v721 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v722 = trunc.i16 v721
	# v723 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v724 = trunc.i16 v723
	# v725 = umulhi v722, v724
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v726 = iconst.i16 -2
	# v727 = icmp.ne v725, v726
	cmp	di, 65534
	setne	dil
	# v728 = iconst.i32 81
	mov	r8d, 81
	# v729 = select v727, v728, v720
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v730 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v731 = trunc.i16 v730
	# v732 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v733 = trunc.i16 v732
	# v734 = smulhi v731, v733
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v735 = iconst.i16 0
	# v736 = icmp.ne v734, v735
	cmp	di, 0
	setne	dil
	# v737 = iconst.i32 82
	mov	r8d, 82
	# v738 = select v736, v737, v729
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v739 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v740 = trunc.i16 v739
	# v741 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v742 = trunc.i16 v741
	# v743 = umulhi v740, v742
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v744 = iconst.i16 32767
	# v745 = icmp.ne v743, v744
	cmp	di, 32767
	setne	dil
	# v746 = iconst.i32 83
	mov	r8d, 83
	# v747 = select v745, v746, v738
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v748 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v749 = trunc.i16 v748
	# v750 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v751 = trunc.i16 v750
	# v752 = smulhi v749, v751
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v753 = iconst.i16 0
	# v754 = icmp.ne v752, v753
	cmp	di, 0
	setne	dil
	# v755 = iconst.i32 84
	mov	r8d, 84
	# v756 = select v754, v755, v747
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v757 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v758 = trunc.i16 v757
	# v759 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v760 = trunc.i16 v759
	# v761 = umulhi v758, v760
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v762 = iconst.i16 2
	# v763 = icmp.ne v761, v762
	cmp	di, 2
	setne	dil
	# v764 = iconst.i32 85
	mov	r8d, 85
	# v765 = select v763, v764, v756
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v766 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v767 = trunc.i16 v766
	# v768 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v769 = trunc.i16 v768
	# v770 = smulhi v767, v769
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v771 = iconst.i16 -1
	# v772 = icmp.ne v770, v771
	cmp	di, 65535
	setne	dil
	# v773 = iconst.i32 86
	mov	r8d, 86
	# v774 = select v772, v773, v765
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v775 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v776 = trunc.i16 v775
	# v777 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v778 = trunc.i16 v777
	# v779 = umulhi v776, v778
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v780 = iconst.i16 32766
	# v781 = icmp.ne v779, v780
	cmp	di, 32766
	setne	dil
	# v782 = iconst.i32 87
	mov	r8d, 87
	# v783 = select v781, v782, v774
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v784 = iconst.i64 -6502179613573251073
	movabs	rdi, 11944564460136300543
	# v785 = trunc.i16 v784
	# v786 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v787 = trunc.i16 v786
	# v788 = smulhi v785, v787
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v789 = iconst.i16 -1
	# v790 = icmp.ne v788, v789
	cmp	di, 65535
	setne	dil
	# v791 = iconst.i32 88
	mov	r8d, 88
	# v792 = select v790, v791, v783
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v793 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v794 = trunc.i16 v793
	# v795 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v796 = trunc.i16 v795
	# v797 = umulhi v794, v796
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v798 = iconst.i16 32767
	# v799 = icmp.ne v797, v798
	cmp	di, 32767
	setne	dil
	# v800 = iconst.i32 89
	mov	r8d, 89
	# v801 = select v799, v800, v792
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v802 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v803 = trunc.i16 v802
	# v804 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v805 = trunc.i16 v804
	# v806 = smulhi v803, v805
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v807 = iconst.i16 0
	# v808 = icmp.ne v806, v807
	cmp	di, 0
	setne	dil
	# v809 = iconst.i32 90
	mov	r8d, 90
	# v810 = select v808, v809, v801
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v811 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v812 = trunc.i16 v811
	# v813 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v814 = trunc.i16 v813
	# v815 = umulhi v812, v814
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v816 = iconst.i16 16384
	# v817 = icmp.ne v815, v816
	cmp	di, 16384
	setne	dil
	# v818 = iconst.i32 91
	mov	r8d, 91
	# v819 = select v817, v818, v810
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v820 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v821 = trunc.i16 v820
	# v822 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v823 = trunc.i16 v822
	# v824 = smulhi v821, v823
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v825 = iconst.i16 16384
	# v826 = icmp.ne v824, v825
	cmp	di, 16384
	setne	dil
	# v827 = iconst.i32 92
	mov	r8d, 92
	# v828 = select v826, v827, v819
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v829 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v830 = trunc.i16 v829
	# v831 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v832 = trunc.i16 v831
	# v833 = umulhi v830, v832
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v834 = iconst.i16 1
	# v835 = icmp.ne v833, v834
	cmp	di, 1
	setne	dil
	# v836 = iconst.i32 93
	mov	r8d, 93
	# v837 = select v835, v836, v828
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v838 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v839 = trunc.i16 v838
	# v840 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v841 = trunc.i16 v840
	# v842 = smulhi v839, v841
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v843 = iconst.i16 -2
	# v844 = icmp.ne v842, v843
	cmp	di, 65534
	setne	dil
	# v845 = iconst.i32 94
	mov	r8d, 94
	# v846 = select v844, v845, v837
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v847 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v848 = trunc.i16 v847
	# v849 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v850 = trunc.i16 v849
	# v851 = umulhi v848, v850
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v852 = iconst.i16 16383
	# v853 = icmp.ne v851, v852
	cmp	di, 16383
	setne	dil
	# v854 = iconst.i32 95
	mov	r8d, 95
	# v855 = select v853, v854, v846
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v856 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v857 = trunc.i16 v856
	# v858 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v859 = trunc.i16 v858
	# v860 = smulhi v857, v859
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v861 = iconst.i16 -16384
	# v862 = icmp.ne v860, v861
	cmp	di, 49152
	setne	dil
	# v863 = iconst.i32 96
	mov	r8d, 96
	# v864 = select v862, v863, v855
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v865 = iconst.i64 -6502179613573283841
	movabs	rdi, 11944564460136267775
	# v866 = trunc.i16 v865
	# v867 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v868 = trunc.i16 v867
	# v869 = umulhi v866, v868
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v870 = iconst.i16 32766
	# v871 = icmp.ne v869, v870
	cmp	di, 32766
	setne	dil
	# v872 = iconst.i32 97
	mov	r8d, 97
	# v873 = select v871, v872, v864
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v874 = iconst.i64 -6502179613573283841
	movabs	rdi, 11944564460136267775
	# v875 = trunc.i16 v874
	# v876 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v877 = trunc.i16 v876
	# v878 = smulhi v875, v877
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v879 = iconst.i16 -1
	# v880 = icmp.ne v878, v879
	cmp	di, 65535
	setne	dil
	# v881 = iconst.i32 98
	mov	r8d, 98
	# v882 = select v880, v881, v873
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v883 = iconst.i64 -6502179613573283841
	movabs	rdi, 11944564460136267775
	# v884 = trunc.i16 v883
	# v885 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v886 = trunc.i16 v885
	# v887 = umulhi v884, v886
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v888 = iconst.i16 16383
	# v889 = icmp.ne v887, v888
	cmp	di, 16383
	setne	dil
	# v890 = iconst.i32 99
	mov	r8d, 99
	# v891 = select v889, v890, v882
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v892 = iconst.i64 -6502179613573283841
	movabs	rdi, 11944564460136267775
	# v893 = trunc.i16 v892
	# v894 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v895 = trunc.i16 v894
	# v896 = smulhi v893, v895
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v897 = iconst.i16 -16384
	# v898 = icmp.ne v896, v897
	cmp	di, 49152
	setne	dil
	# v899 = iconst.i32 100
	mov	r8d, 100
	# v900 = select v898, v899, v891
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v901 = iconst.i64 -6502179613573283841
	movabs	rdi, 11944564460136267775
	# v902 = trunc.i16 v901
	# v903 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v904 = trunc.i16 v903
	# v905 = umulhi v902, v904
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v906 = iconst.i16 1
	# v907 = icmp.ne v905, v906
	cmp	di, 1
	setne	dil
	# v908 = iconst.i32 101
	mov	r8d, 101
	# v909 = select v907, v908, v900
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v910 = iconst.i64 -6502179613573283841
	movabs	rdi, 11944564460136267775
	# v911 = trunc.i16 v910
	# v912 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v913 = trunc.i16 v912
	# v914 = smulhi v911, v913
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v915 = iconst.i16 1
	# v916 = icmp.ne v914, v915
	cmp	di, 1
	setne	dil
	# v917 = iconst.i32 102
	mov	r8d, 102
	# v918 = select v916, v917, v909
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v919 = iconst.i64 -6502179613573283841
	movabs	rdi, 11944564460136267775
	# v920 = trunc.i16 v919
	# v921 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v922 = trunc.i16 v921
	# v923 = umulhi v920, v922
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v924 = iconst.i16 16383
	# v925 = icmp.ne v923, v924
	cmp	di, 16383
	setne	dil
	# v926 = iconst.i32 103
	mov	r8d, 103
	# v927 = select v925, v926, v918
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v928 = iconst.i64 -6502179613573283841
	movabs	rdi, 11944564460136267775
	# v929 = trunc.i16 v928
	# v930 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v931 = trunc.i16 v930
	# v932 = smulhi v929, v931
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v933 = iconst.i16 16383
	# v934 = icmp.ne v932, v933
	cmp	di, 16383
	setne	dil
	# v935 = iconst.i32 104
	mov	r8d, 104
	# v936 = select v934, v935, v927
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v937 = iconst.i64 -6502179613573293478
	movabs	rdi, 11944564460136258138
	# v938 = trunc.i16 v937
	# v939 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v940 = trunc.i16 v939
	# v941 = umulhi v938, v940
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v942 = iconst.i16 23129
	# v943 = icmp.ne v941, v942
	cmp	di, 23129
	setne	dil
	# v944 = iconst.i32 105
	mov	r8d, 105
	# v945 = select v943, v944, v936
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v946 = iconst.i64 -6502179613573293478
	movabs	rdi, 11944564460136258138
	# v947 = trunc.i16 v946
	# v948 = iconst.i64 -6502179613573251073
	movabs	r8, 11944564460136300543
	# v949 = trunc.i16 v948
	# v950 = smulhi v947, v949
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v951 = iconst.i16 -1
	# v952 = icmp.ne v950, v951
	cmp	di, 65535
	setne	dil
	# v953 = iconst.i32 106
	mov	r8d, 106
	# v954 = select v952, v953, v945
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v955 = iconst.i64 -6502179613573293478
	movabs	rdi, 11944564460136258138
	# v956 = trunc.i16 v955
	# v957 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v958 = trunc.i16 v957
	# v959 = umulhi v956, v958
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v960 = iconst.i16 11565
	# v961 = icmp.ne v959, v960
	cmp	di, 11565
	setne	dil
	# v962 = iconst.i32 107
	mov	r8d, 107
	# v963 = select v961, v962, v954
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v964 = iconst.i64 -6502179613573293478
	movabs	rdi, 11944564460136258138
	# v965 = trunc.i16 v964
	# v966 = iconst.i64 -6502179613573283840
	movabs	r8, 11944564460136267776
	# v967 = trunc.i16 v966
	# v968 = smulhi v965, v967
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v969 = iconst.i16 -11565
	# v970 = icmp.ne v968, v969
	cmp	di, 53971
	setne	dil
	# v971 = iconst.i32 108
	mov	r8d, 108
	# v972 = select v970, v971, v963
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v973 = iconst.i64 -6502179613573293478
	movabs	rdi, 11944564460136258138
	# v974 = trunc.i16 v973
	# v975 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v976 = trunc.i16 v975
	# v977 = umulhi v974, v976
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v978 = iconst.i16 1
	# v979 = icmp.ne v977, v978
	cmp	di, 1
	setne	dil
	# v980 = iconst.i32 109
	mov	r8d, 109
	# v981 = select v979, v980, v972
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v982 = iconst.i64 -6502179613573293478
	movabs	rdi, 11944564460136258138
	# v983 = trunc.i16 v982
	# v984 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v985 = trunc.i16 v984
	# v986 = smulhi v983, v985
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v987 = iconst.i16 1
	# v988 = icmp.ne v986, v987
	cmp	di, 1
	setne	dil
	# v989 = iconst.i32 110
	mov	r8d, 110
	# v990 = select v988, v989, v981
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v991 = iconst.i64 -6502179613573293478
	movabs	rdi, 11944564460136258138
	# v992 = trunc.i16 v991
	# v993 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v994 = trunc.i16 v993
	# v995 = umulhi v992, v994
	movzx	eax, di
	movzx	ecx, r8w
	imul	eax, ecx
	shr	eax, 16
	mov	edi, eax
	# v996 = iconst.i16 11564
	# v997 = icmp.ne v995, v996
	cmp	di, 11564
	setne	dil
	# v998 = iconst.i32 111
	mov	r8d, 111
	# v999 = select v997, v998, v990
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1000 = iconst.i64 -6502179613573293478
	movabs	rdi, 11944564460136258138
	# v1001 = trunc.i16 v1000
	# v1002 = iconst.i64 -6502179613573283841
	movabs	r8, 11944564460136267775
	# v1003 = trunc.i16 v1002
	# v1004 = smulhi v1001, v1003
	movsx	eax, di
	movsx	ecx, r8w
	imul	eax, ecx
	sar	eax, 16
	mov	edi, eax
	# v1005 = iconst.i16 11564
	# v1006 = icmp.ne v1004, v1005
	cmp	di, 11564
	setne	dil
	# v1007 = iconst.i32 112
	mov	r8d, 112
	# v1008 = select v1006, v1007, v999
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1009 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v1010 = trunc.i32 v1009
	# v1011 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1012 = trunc.i32 v1011
	# v1013 = umulhi v1010, v1012
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1014 = iconst.i32 0
	# v1015 = icmp.ne v1013, v1014
	cmp	edi, 0
	setne	dil
	# v1016 = iconst.i32 113
	mov	r8d, 113
	# v1017 = select v1015, v1016, v1008
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1018 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v1019 = trunc.i32 v1018
	# v1020 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1021 = trunc.i32 v1020
	# v1022 = smulhi v1019, v1021
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1023 = iconst.i32 0
	# v1024 = icmp.ne v1022, v1023
	cmp	edi, 0
	setne	dil
	# v1025 = iconst.i32 114
	mov	r8d, 114
	# v1026 = select v1024, v1025, v1017
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1027 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v1028 = trunc.i32 v1027
	# v1029 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1030 = trunc.i32 v1029
	# v1031 = umulhi v1028, v1030
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1032 = iconst.i32 0
	# v1033 = icmp.ne v1031, v1032
	cmp	edi, 0
	setne	dil
	# v1034 = iconst.i32 115
	mov	r8d, 115
	# v1035 = select v1033, v1034, v1026
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1036 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v1037 = trunc.i32 v1036
	# v1038 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1039 = trunc.i32 v1038
	# v1040 = smulhi v1037, v1039
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1041 = iconst.i32 0
	# v1042 = icmp.ne v1040, v1041
	cmp	edi, 0
	setne	dil
	# v1043 = iconst.i32 116
	mov	r8d, 116
	# v1044 = select v1042, v1043, v1035
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1045 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v1046 = trunc.i32 v1045
	# v1047 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1048 = trunc.i32 v1047
	# v1049 = umulhi v1046, v1048
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1050 = iconst.i32 0
	# v1051 = icmp.ne v1049, v1050
	cmp	edi, 0
	setne	dil
	# v1052 = iconst.i32 117
	mov	r8d, 117
	# v1053 = select v1051, v1052, v1044
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1054 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v1055 = trunc.i32 v1054
	# v1056 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1057 = trunc.i32 v1056
	# v1058 = smulhi v1055, v1057
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1059 = iconst.i32 0
	# v1060 = icmp.ne v1058, v1059
	cmp	edi, 0
	setne	dil
	# v1061 = iconst.i32 118
	mov	r8d, 118
	# v1062 = select v1060, v1061, v1053
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1063 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v1064 = trunc.i32 v1063
	# v1065 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1066 = trunc.i32 v1065
	# v1067 = umulhi v1064, v1066
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1068 = iconst.i32 0
	# v1069 = icmp.ne v1067, v1068
	cmp	edi, 0
	setne	dil
	# v1070 = iconst.i32 119
	mov	r8d, 119
	# v1071 = select v1069, v1070, v1062
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1072 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v1073 = trunc.i32 v1072
	# v1074 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1075 = trunc.i32 v1074
	# v1076 = smulhi v1073, v1075
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1077 = iconst.i32 0
	# v1078 = icmp.ne v1076, v1077
	cmp	edi, 0
	setne	dil
	# v1079 = iconst.i32 120
	mov	r8d, 120
	# v1080 = select v1078, v1079, v1071
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1081 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v1082 = trunc.i32 v1081
	# v1083 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1084 = trunc.i32 v1083
	# v1085 = umulhi v1082, v1084
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1086 = iconst.i32 0
	# v1087 = icmp.ne v1085, v1086
	cmp	edi, 0
	setne	dil
	# v1088 = iconst.i32 121
	mov	r8d, 121
	# v1089 = select v1087, v1088, v1080
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1090 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v1091 = trunc.i32 v1090
	# v1092 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1093 = trunc.i32 v1092
	# v1094 = smulhi v1091, v1093
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1095 = iconst.i32 -1
	# v1096 = icmp.ne v1094, v1095
	cmp	edi, 4294967295
	setne	dil
	# v1097 = iconst.i32 122
	mov	r8d, 122
	# v1098 = select v1096, v1097, v1089
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1099 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v1100 = trunc.i32 v1099
	# v1101 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1102 = trunc.i32 v1101
	# v1103 = umulhi v1100, v1102
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1104 = iconst.i32 0
	# v1105 = icmp.ne v1103, v1104
	cmp	edi, 0
	setne	dil
	# v1106 = iconst.i32 123
	mov	r8d, 123
	# v1107 = select v1105, v1106, v1098
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1108 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v1109 = trunc.i32 v1108
	# v1110 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1111 = trunc.i32 v1110
	# v1112 = smulhi v1109, v1111
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1113 = iconst.i32 -1
	# v1114 = icmp.ne v1112, v1113
	cmp	edi, 4294967295
	setne	dil
	# v1115 = iconst.i32 124
	mov	r8d, 124
	# v1116 = select v1114, v1115, v1107
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1117 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v1118 = trunc.i32 v1117
	# v1119 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1120 = trunc.i32 v1119
	# v1121 = umulhi v1118, v1120
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1122 = iconst.i32 0
	# v1123 = icmp.ne v1121, v1122
	cmp	edi, 0
	setne	dil
	# v1124 = iconst.i32 125
	mov	r8d, 125
	# v1125 = select v1123, v1124, v1116
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1126 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v1127 = trunc.i32 v1126
	# v1128 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1129 = trunc.i32 v1128
	# v1130 = smulhi v1127, v1129
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1131 = iconst.i32 0
	# v1132 = icmp.ne v1130, v1131
	cmp	edi, 0
	setne	dil
	# v1133 = iconst.i32 126
	mov	r8d, 126
	# v1134 = select v1132, v1133, v1125
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1135 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v1136 = trunc.i32 v1135
	# v1137 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1138 = trunc.i32 v1137
	# v1139 = umulhi v1136, v1138
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1140 = iconst.i32 0
	# v1141 = icmp.ne v1139, v1140
	cmp	edi, 0
	setne	dil
	# v1142 = iconst.i32 127
	mov	r8d, 127
	# v1143 = select v1141, v1142, v1134
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1144 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v1145 = trunc.i32 v1144
	# v1146 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1147 = trunc.i32 v1146
	# v1148 = smulhi v1145, v1147
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1149 = iconst.i32 0
	# v1150 = icmp.ne v1148, v1149
	cmp	edi, 0
	setne	dil
	# v1151 = iconst.i32 128
	mov	r8d, 128
	# v1152 = select v1150, v1151, v1143
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1153 = iconst.i64 -6502179615371427838
	movabs	rdi, 11944564458338123778
	# v1154 = trunc.i32 v1153
	# v1155 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1156 = trunc.i32 v1155
	# v1157 = umulhi v1154, v1156
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1158 = iconst.i32 1
	# v1159 = icmp.ne v1157, v1158
	cmp	edi, 1
	setne	dil
	# v1160 = iconst.i32 129
	mov	r8d, 129
	# v1161 = select v1159, v1160, v1152
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1162 = iconst.i64 -6502179615371427838
	movabs	rdi, 11944564458338123778
	# v1163 = trunc.i32 v1162
	# v1164 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1165 = trunc.i32 v1164
	# v1166 = smulhi v1163, v1165
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1167 = iconst.i32 -1
	# v1168 = icmp.ne v1166, v1167
	cmp	edi, 4294967295
	setne	dil
	# v1169 = iconst.i32 130
	mov	r8d, 130
	# v1170 = select v1168, v1169, v1161
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1171 = iconst.i64 -6502179615371427838
	movabs	rdi, 11944564458338123778
	# v1172 = trunc.i32 v1171
	# v1173 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1174 = trunc.i32 v1173
	# v1175 = umulhi v1172, v1174
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1176 = iconst.i32 1
	# v1177 = icmp.ne v1175, v1176
	cmp	edi, 1
	setne	dil
	# v1178 = iconst.i32 131
	mov	r8d, 131
	# v1179 = select v1177, v1178, v1170
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1180 = iconst.i64 -6502179615371427838
	movabs	rdi, 11944564458338123778
	# v1181 = trunc.i32 v1180
	# v1182 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1183 = trunc.i32 v1182
	# v1184 = smulhi v1181, v1183
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1185 = iconst.i32 -1
	# v1186 = icmp.ne v1184, v1185
	cmp	edi, 4294967295
	setne	dil
	# v1187 = iconst.i32 132
	mov	r8d, 132
	# v1188 = select v1186, v1187, v1179
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1189 = iconst.i64 -6502179615371427838
	movabs	rdi, 11944564458338123778
	# v1190 = trunc.i32 v1189
	# v1191 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1192 = trunc.i32 v1191
	# v1193 = umulhi v1190, v1192
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1194 = iconst.i32 0
	# v1195 = icmp.ne v1193, v1194
	cmp	edi, 0
	setne	dil
	# v1196 = iconst.i32 133
	mov	r8d, 133
	# v1197 = select v1195, v1196, v1188
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1198 = iconst.i64 -6502179615371427838
	movabs	rdi, 11944564458338123778
	# v1199 = trunc.i32 v1198
	# v1200 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1201 = trunc.i32 v1200
	# v1202 = smulhi v1199, v1201
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1203 = iconst.i32 0
	# v1204 = icmp.ne v1202, v1203
	cmp	edi, 0
	setne	dil
	# v1205 = iconst.i32 134
	mov	r8d, 134
	# v1206 = select v1204, v1205, v1197
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1207 = iconst.i64 -6502179615371427838
	movabs	rdi, 11944564458338123778
	# v1208 = trunc.i32 v1207
	# v1209 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1210 = trunc.i32 v1209
	# v1211 = umulhi v1208, v1210
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1212 = iconst.i32 0
	# v1213 = icmp.ne v1211, v1212
	cmp	edi, 0
	setne	dil
	# v1214 = iconst.i32 135
	mov	r8d, 135
	# v1215 = select v1213, v1214, v1206
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1216 = iconst.i64 -6502179615371427838
	movabs	rdi, 11944564458338123778
	# v1217 = trunc.i32 v1216
	# v1218 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1219 = trunc.i32 v1218
	# v1220 = smulhi v1217, v1219
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1221 = iconst.i32 0
	# v1222 = icmp.ne v1220, v1221
	cmp	edi, 0
	setne	dil
	# v1223 = iconst.i32 136
	mov	r8d, 136
	# v1224 = select v1222, v1223, v1215
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1225 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v1226 = trunc.i32 v1225
	# v1227 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1228 = trunc.i32 v1227
	# v1229 = umulhi v1226, v1228
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1230 = iconst.i32 -2
	# v1231 = icmp.ne v1229, v1230
	cmp	edi, 4294967294
	setne	dil
	# v1232 = iconst.i32 137
	mov	r8d, 137
	# v1233 = select v1231, v1232, v1224
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1234 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v1235 = trunc.i32 v1234
	# v1236 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1237 = trunc.i32 v1236
	# v1238 = smulhi v1235, v1237
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1239 = iconst.i32 0
	# v1240 = icmp.ne v1238, v1239
	cmp	edi, 0
	setne	dil
	# v1241 = iconst.i32 138
	mov	r8d, 138
	# v1242 = select v1240, v1241, v1233
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1243 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v1244 = trunc.i32 v1243
	# v1245 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1246 = trunc.i32 v1245
	# v1247 = umulhi v1244, v1246
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1248 = iconst.i32 2147483647
	# v1249 = icmp.ne v1247, v1248
	cmp	edi, 2147483647
	setne	dil
	# v1250 = iconst.i32 139
	mov	r8d, 139
	# v1251 = select v1249, v1250, v1242
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1252 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v1253 = trunc.i32 v1252
	# v1254 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1255 = trunc.i32 v1254
	# v1256 = smulhi v1253, v1255
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1257 = iconst.i32 0
	# v1258 = icmp.ne v1256, v1257
	cmp	edi, 0
	setne	dil
	# v1259 = iconst.i32 140
	mov	r8d, 140
	# v1260 = select v1258, v1259, v1251
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1261 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v1262 = trunc.i32 v1261
	# v1263 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1264 = trunc.i32 v1263
	# v1265 = umulhi v1262, v1264
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1266 = iconst.i32 2
	# v1267 = icmp.ne v1265, v1266
	cmp	edi, 2
	setne	dil
	# v1268 = iconst.i32 141
	mov	r8d, 141
	# v1269 = select v1267, v1268, v1260
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1270 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v1271 = trunc.i32 v1270
	# v1272 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1273 = trunc.i32 v1272
	# v1274 = smulhi v1271, v1273
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1275 = iconst.i32 -1
	# v1276 = icmp.ne v1274, v1275
	cmp	edi, 4294967295
	setne	dil
	# v1277 = iconst.i32 142
	mov	r8d, 142
	# v1278 = select v1276, v1277, v1269
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1279 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v1280 = trunc.i32 v1279
	# v1281 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1282 = trunc.i32 v1281
	# v1283 = umulhi v1280, v1282
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1284 = iconst.i32 2147483646
	# v1285 = icmp.ne v1283, v1284
	cmp	edi, 2147483646
	setne	dil
	# v1286 = iconst.i32 143
	mov	r8d, 143
	# v1287 = select v1285, v1286, v1278
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1288 = iconst.i64 -6502179611076460545
	movabs	rdi, 11944564462633091071
	# v1289 = trunc.i32 v1288
	# v1290 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1291 = trunc.i32 v1290
	# v1292 = smulhi v1289, v1291
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1293 = iconst.i32 -1
	# v1294 = icmp.ne v1292, v1293
	cmp	edi, 4294967295
	setne	dil
	# v1295 = iconst.i32 144
	mov	r8d, 144
	# v1296 = select v1294, v1295, v1287
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1297 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v1298 = trunc.i32 v1297
	# v1299 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1300 = trunc.i32 v1299
	# v1301 = umulhi v1298, v1300
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1302 = iconst.i32 2147483647
	# v1303 = icmp.ne v1301, v1302
	cmp	edi, 2147483647
	setne	dil
	# v1304 = iconst.i32 145
	mov	r8d, 145
	# v1305 = select v1303, v1304, v1296
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1306 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v1307 = trunc.i32 v1306
	# v1308 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1309 = trunc.i32 v1308
	# v1310 = smulhi v1307, v1309
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1311 = iconst.i32 0
	# v1312 = icmp.ne v1310, v1311
	cmp	edi, 0
	setne	dil
	# v1313 = iconst.i32 146
	mov	r8d, 146
	# v1314 = select v1312, v1313, v1305
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1315 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v1316 = trunc.i32 v1315
	# v1317 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1318 = trunc.i32 v1317
	# v1319 = umulhi v1316, v1318
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1320 = iconst.i32 1073741824
	# v1321 = icmp.ne v1319, v1320
	cmp	edi, 1073741824
	setne	dil
	# v1322 = iconst.i32 147
	mov	r8d, 147
	# v1323 = select v1321, v1322, v1314
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1324 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v1325 = trunc.i32 v1324
	# v1326 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1327 = trunc.i32 v1326
	# v1328 = smulhi v1325, v1327
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1329 = iconst.i32 1073741824
	# v1330 = icmp.ne v1328, v1329
	cmp	edi, 1073741824
	setne	dil
	# v1331 = iconst.i32 148
	mov	r8d, 148
	# v1332 = select v1330, v1331, v1323
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1333 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v1334 = trunc.i32 v1333
	# v1335 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1336 = trunc.i32 v1335
	# v1337 = umulhi v1334, v1336
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1338 = iconst.i32 1
	# v1339 = icmp.ne v1337, v1338
	cmp	edi, 1
	setne	dil
	# v1340 = iconst.i32 149
	mov	r8d, 149
	# v1341 = select v1339, v1340, v1332
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1342 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v1343 = trunc.i32 v1342
	# v1344 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1345 = trunc.i32 v1344
	# v1346 = smulhi v1343, v1345
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1347 = iconst.i32 -2
	# v1348 = icmp.ne v1346, v1347
	cmp	edi, 4294967294
	setne	dil
	# v1349 = iconst.i32 150
	mov	r8d, 150
	# v1350 = select v1348, v1349, v1341
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1351 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v1352 = trunc.i32 v1351
	# v1353 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1354 = trunc.i32 v1353
	# v1355 = umulhi v1352, v1354
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1356 = iconst.i32 1073741823
	# v1357 = icmp.ne v1355, v1356
	cmp	edi, 1073741823
	setne	dil
	# v1358 = iconst.i32 151
	mov	r8d, 151
	# v1359 = select v1357, v1358, v1350
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1360 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v1361 = trunc.i32 v1360
	# v1362 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1363 = trunc.i32 v1362
	# v1364 = smulhi v1361, v1363
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1365 = iconst.i32 -1073741824
	# v1366 = icmp.ne v1364, v1365
	cmp	edi, 3221225472
	setne	dil
	# v1367 = iconst.i32 152
	mov	r8d, 152
	# v1368 = select v1366, v1367, v1359
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1369 = iconst.i64 -6502179613223944193
	movabs	rdi, 11944564460485607423
	# v1370 = trunc.i32 v1369
	# v1371 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1372 = trunc.i32 v1371
	# v1373 = umulhi v1370, v1372
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1374 = iconst.i32 2147483646
	# v1375 = icmp.ne v1373, v1374
	cmp	edi, 2147483646
	setne	dil
	# v1376 = iconst.i32 153
	mov	r8d, 153
	# v1377 = select v1375, v1376, v1368
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1378 = iconst.i64 -6502179613223944193
	movabs	rdi, 11944564460485607423
	# v1379 = trunc.i32 v1378
	# v1380 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1381 = trunc.i32 v1380
	# v1382 = smulhi v1379, v1381
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1383 = iconst.i32 -1
	# v1384 = icmp.ne v1382, v1383
	cmp	edi, 4294967295
	setne	dil
	# v1385 = iconst.i32 154
	mov	r8d, 154
	# v1386 = select v1384, v1385, v1377
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1387 = iconst.i64 -6502179613223944193
	movabs	rdi, 11944564460485607423
	# v1388 = trunc.i32 v1387
	# v1389 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1390 = trunc.i32 v1389
	# v1391 = umulhi v1388, v1390
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1392 = iconst.i32 1073741823
	# v1393 = icmp.ne v1391, v1392
	cmp	edi, 1073741823
	setne	dil
	# v1394 = iconst.i32 155
	mov	r8d, 155
	# v1395 = select v1393, v1394, v1386
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1396 = iconst.i64 -6502179613223944193
	movabs	rdi, 11944564460485607423
	# v1397 = trunc.i32 v1396
	# v1398 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1399 = trunc.i32 v1398
	# v1400 = smulhi v1397, v1399
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1401 = iconst.i32 -1073741824
	# v1402 = icmp.ne v1400, v1401
	cmp	edi, 3221225472
	setne	dil
	# v1403 = iconst.i32 156
	mov	r8d, 156
	# v1404 = select v1402, v1403, v1395
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1405 = iconst.i64 -6502179613223944193
	movabs	rdi, 11944564460485607423
	# v1406 = trunc.i32 v1405
	# v1407 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1408 = trunc.i32 v1407
	# v1409 = umulhi v1406, v1408
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1410 = iconst.i32 1
	# v1411 = icmp.ne v1409, v1410
	cmp	edi, 1
	setne	dil
	# v1412 = iconst.i32 157
	mov	r8d, 157
	# v1413 = select v1411, v1412, v1404
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1414 = iconst.i64 -6502179613223944193
	movabs	rdi, 11944564460485607423
	# v1415 = trunc.i32 v1414
	# v1416 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1417 = trunc.i32 v1416
	# v1418 = smulhi v1415, v1417
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1419 = iconst.i32 1
	# v1420 = icmp.ne v1418, v1419
	cmp	edi, 1
	setne	dil
	# v1421 = iconst.i32 158
	mov	r8d, 158
	# v1422 = select v1420, v1421, v1413
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1423 = iconst.i64 -6502179613223944193
	movabs	rdi, 11944564460485607423
	# v1424 = trunc.i32 v1423
	# v1425 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1426 = trunc.i32 v1425
	# v1427 = umulhi v1424, v1426
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1428 = iconst.i32 1073741823
	# v1429 = icmp.ne v1427, v1428
	cmp	edi, 1073741823
	setne	dil
	# v1430 = iconst.i32 159
	mov	r8d, 159
	# v1431 = select v1429, v1430, v1422
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1432 = iconst.i64 -6502179613223944193
	movabs	rdi, 11944564460485607423
	# v1433 = trunc.i32 v1432
	# v1434 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1435 = trunc.i32 v1434
	# v1436 = smulhi v1433, v1435
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1437 = iconst.i32 1073741823
	# v1438 = icmp.ne v1436, v1437
	cmp	edi, 1073741823
	setne	dil
	# v1439 = iconst.i32 160
	mov	r8d, 160
	# v1440 = select v1438, v1439, v1431
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1441 = iconst.i64 -6502179613855557030
	movabs	rdi, 11944564459853994586
	# v1442 = trunc.i32 v1441
	# v1443 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1444 = trunc.i32 v1443
	# v1445 = umulhi v1442, v1444
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1446 = iconst.i32 1515870809
	# v1447 = icmp.ne v1445, v1446
	cmp	edi, 1515870809
	setne	dil
	# v1448 = iconst.i32 161
	mov	r8d, 161
	# v1449 = select v1447, v1448, v1440
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1450 = iconst.i64 -6502179613855557030
	movabs	rdi, 11944564459853994586
	# v1451 = trunc.i32 v1450
	# v1452 = iconst.i64 -6502179611076460545
	movabs	r8, 11944564462633091071
	# v1453 = trunc.i32 v1452
	# v1454 = smulhi v1451, v1453
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1455 = iconst.i32 -1
	# v1456 = icmp.ne v1454, v1455
	cmp	edi, 4294967295
	setne	dil
	# v1457 = iconst.i32 162
	mov	r8d, 162
	# v1458 = select v1456, v1457, v1449
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1459 = iconst.i64 -6502179613855557030
	movabs	rdi, 11944564459853994586
	# v1460 = trunc.i32 v1459
	# v1461 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1462 = trunc.i32 v1461
	# v1463 = umulhi v1460, v1462
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1464 = iconst.i32 757935405
	# v1465 = icmp.ne v1463, v1464
	cmp	edi, 757935405
	setne	dil
	# v1466 = iconst.i32 163
	mov	r8d, 163
	# v1467 = select v1465, v1466, v1458
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1468 = iconst.i64 -6502179613855557030
	movabs	rdi, 11944564459853994586
	# v1469 = trunc.i32 v1468
	# v1470 = iconst.i64 -6502179613223944192
	movabs	r8, 11944564460485607424
	# v1471 = trunc.i32 v1470
	# v1472 = smulhi v1469, v1471
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1473 = iconst.i32 -757935405
	# v1474 = icmp.ne v1472, v1473
	cmp	edi, 3537031891
	setne	dil
	# v1475 = iconst.i32 164
	mov	r8d, 164
	# v1476 = select v1474, v1475, v1467
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1477 = iconst.i64 -6502179613855557030
	movabs	rdi, 11944564459853994586
	# v1478 = trunc.i32 v1477
	# v1479 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1480 = trunc.i32 v1479
	# v1481 = umulhi v1478, v1480
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1482 = iconst.i32 1
	# v1483 = icmp.ne v1481, v1482
	cmp	edi, 1
	setne	dil
	# v1484 = iconst.i32 165
	mov	r8d, 165
	# v1485 = select v1483, v1484, v1476
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1486 = iconst.i64 -6502179613855557030
	movabs	rdi, 11944564459853994586
	# v1487 = trunc.i32 v1486
	# v1488 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v1489 = trunc.i32 v1488
	# v1490 = smulhi v1487, v1489
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1491 = iconst.i32 1
	# v1492 = icmp.ne v1490, v1491
	cmp	edi, 1
	setne	dil
	# v1493 = iconst.i32 166
	mov	r8d, 166
	# v1494 = select v1492, v1493, v1485
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1495 = iconst.i64 -6502179613855557030
	movabs	rdi, 11944564459853994586
	# v1496 = trunc.i32 v1495
	# v1497 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1498 = trunc.i32 v1497
	# v1499 = umulhi v1496, v1498
	mov	rax, rdi
	mul	r8d
	mov	rdi, rdx
	# v1500 = iconst.i32 757935404
	# v1501 = icmp.ne v1499, v1500
	cmp	edi, 757935404
	setne	dil
	# v1502 = iconst.i32 167
	mov	r8d, 167
	# v1503 = select v1501, v1502, v1494
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1504 = iconst.i64 -6502179613855557030
	movabs	rdi, 11944564459853994586
	# v1505 = trunc.i32 v1504
	# v1506 = iconst.i64 -6502179613223944193
	movabs	r8, 11944564460485607423
	# v1507 = trunc.i32 v1506
	# v1508 = smulhi v1505, v1507
	mov	rax, rdi
	imul	r8d
	mov	rdi, rdx
	# v1509 = iconst.i32 757935404
	# v1510 = icmp.ne v1508, v1509
	cmp	edi, 757935404
	setne	dil
	# v1511 = iconst.i32 168
	mov	r8d, 168
	# v1512 = select v1510, v1511, v1503
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1513 = iconst.i64 0
	xor	edi, edi
	# v1514 = iconst.i64 -1
	mov	r8, -1
	# v1515 = umulhi v1513, v1514
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1516 = iconst.i64 0
	# v1517 = icmp.ne v1515, v1516
	cmp	rdi, 0
	setne	dil
	# v1518 = iconst.i32 169
	mov	r8d, 169
	# v1519 = select v1517, v1518, v1512
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1520 = iconst.i64 0
	xor	edi, edi
	# v1521 = iconst.i64 -1
	mov	r8, -1
	# v1522 = smulhi v1520, v1521
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1523 = iconst.i64 0
	# v1524 = icmp.ne v1522, v1523
	cmp	rdi, 0
	setne	dil
	# v1525 = iconst.i32 170
	mov	r8d, 170
	# v1526 = select v1524, v1525, v1519
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1527 = iconst.i64 0
	xor	edi, edi
	# v1528 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1529 = umulhi v1527, v1528
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1530 = iconst.i64 0
	# v1531 = icmp.ne v1529, v1530
	cmp	rdi, 0
	setne	dil
	# v1532 = iconst.i32 171
	mov	r8d, 171
	# v1533 = select v1531, v1532, v1526
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1534 = iconst.i64 0
	xor	edi, edi
	# v1535 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1536 = smulhi v1534, v1535
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1537 = iconst.i64 0
	# v1538 = icmp.ne v1536, v1537
	cmp	rdi, 0
	setne	dil
	# v1539 = iconst.i32 172
	mov	r8d, 172
	# v1540 = select v1538, v1539, v1533
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1541 = iconst.i64 0
	xor	edi, edi
	# v1542 = iconst.i64 3
	mov	r8d, 3
	# v1543 = umulhi v1541, v1542
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1544 = iconst.i64 0
	# v1545 = icmp.ne v1543, v1544
	cmp	rdi, 0
	setne	dil
	# v1546 = iconst.i32 173
	mov	r8d, 173
	# v1547 = select v1545, v1546, v1540
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1548 = iconst.i64 0
	xor	edi, edi
	# v1549 = iconst.i64 3
	mov	r8d, 3
	# v1550 = smulhi v1548, v1549
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1551 = iconst.i64 0
	# v1552 = icmp.ne v1550, v1551
	cmp	rdi, 0
	setne	dil
	# v1553 = iconst.i32 174
	mov	r8d, 174
	# v1554 = select v1552, v1553, v1547
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1555 = iconst.i64 0
	xor	edi, edi
	# v1556 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1557 = umulhi v1555, v1556
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1558 = iconst.i64 0
	# v1559 = icmp.ne v1557, v1558
	cmp	rdi, 0
	setne	dil
	# v1560 = iconst.i32 175
	mov	r8d, 175
	# v1561 = select v1559, v1560, v1554
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1562 = iconst.i64 0
	xor	edi, edi
	# v1563 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1564 = smulhi v1562, v1563
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1565 = iconst.i64 0
	# v1566 = icmp.ne v1564, v1565
	cmp	rdi, 0
	setne	dil
	# v1567 = iconst.i32 176
	mov	r8d, 176
	# v1568 = select v1566, v1567, v1561
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1569 = iconst.i64 1
	mov	edi, 1
	# v1570 = iconst.i64 -1
	mov	r8, -1
	# v1571 = umulhi v1569, v1570
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1572 = iconst.i64 0
	# v1573 = icmp.ne v1571, v1572
	cmp	rdi, 0
	setne	dil
	# v1574 = iconst.i32 177
	mov	r8d, 177
	# v1575 = select v1573, v1574, v1568
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1576 = iconst.i64 1
	mov	edi, 1
	# v1577 = iconst.i64 -1
	mov	r8, -1
	# v1578 = smulhi v1576, v1577
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1579 = iconst.i64 -1
	# v1580 = icmp.ne v1578, v1579
	cmp	rdi, -1
	setne	dil
	# v1581 = iconst.i32 178
	mov	r8d, 178
	# v1582 = select v1580, v1581, v1575
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1583 = iconst.i64 1
	mov	edi, 1
	# v1584 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1585 = umulhi v1583, v1584
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1586 = iconst.i64 0
	# v1587 = icmp.ne v1585, v1586
	cmp	rdi, 0
	setne	dil
	# v1588 = iconst.i32 179
	mov	r8d, 179
	# v1589 = select v1587, v1588, v1582
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1590 = iconst.i64 1
	mov	edi, 1
	# v1591 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1592 = smulhi v1590, v1591
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1593 = iconst.i64 -1
	# v1594 = icmp.ne v1592, v1593
	cmp	rdi, -1
	setne	dil
	# v1595 = iconst.i32 180
	mov	r8d, 180
	# v1596 = select v1594, v1595, v1589
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1597 = iconst.i64 1
	mov	edi, 1
	# v1598 = iconst.i64 3
	mov	r8d, 3
	# v1599 = umulhi v1597, v1598
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1600 = iconst.i64 0
	# v1601 = icmp.ne v1599, v1600
	cmp	rdi, 0
	setne	dil
	# v1602 = iconst.i32 181
	mov	r8d, 181
	# v1603 = select v1601, v1602, v1596
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1604 = iconst.i64 1
	mov	edi, 1
	# v1605 = iconst.i64 3
	mov	r8d, 3
	# v1606 = smulhi v1604, v1605
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1607 = iconst.i64 0
	# v1608 = icmp.ne v1606, v1607
	cmp	rdi, 0
	setne	dil
	# v1609 = iconst.i32 182
	mov	r8d, 182
	# v1610 = select v1608, v1609, v1603
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1611 = iconst.i64 1
	mov	edi, 1
	# v1612 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1613 = umulhi v1611, v1612
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1614 = iconst.i64 0
	# v1615 = icmp.ne v1613, v1614
	cmp	rdi, 0
	setne	dil
	# v1616 = iconst.i32 183
	mov	r8d, 183
	# v1617 = select v1615, v1616, v1610
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1618 = iconst.i64 1
	mov	edi, 1
	# v1619 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1620 = smulhi v1618, v1619
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1621 = iconst.i64 0
	# v1622 = icmp.ne v1620, v1621
	cmp	rdi, 0
	setne	dil
	# v1623 = iconst.i32 184
	mov	r8d, 184
	# v1624 = select v1622, v1623, v1617
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1625 = iconst.i64 2
	mov	edi, 2
	# v1626 = iconst.i64 -1
	mov	r8, -1
	# v1627 = umulhi v1625, v1626
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1628 = iconst.i64 1
	# v1629 = icmp.ne v1627, v1628
	cmp	rdi, 1
	setne	dil
	# v1630 = iconst.i32 185
	mov	r8d, 185
	# v1631 = select v1629, v1630, v1624
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1632 = iconst.i64 2
	mov	edi, 2
	# v1633 = iconst.i64 -1
	mov	r8, -1
	# v1634 = smulhi v1632, v1633
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1635 = iconst.i64 -1
	# v1636 = icmp.ne v1634, v1635
	cmp	rdi, -1
	setne	dil
	# v1637 = iconst.i32 186
	mov	r8d, 186
	# v1638 = select v1636, v1637, v1631
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1639 = iconst.i64 2
	mov	edi, 2
	# v1640 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1641 = umulhi v1639, v1640
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1642 = iconst.i64 1
	# v1643 = icmp.ne v1641, v1642
	cmp	rdi, 1
	setne	dil
	# v1644 = iconst.i32 187
	mov	r8d, 187
	# v1645 = select v1643, v1644, v1638
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1646 = iconst.i64 2
	mov	edi, 2
	# v1647 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1648 = smulhi v1646, v1647
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1649 = iconst.i64 -1
	# v1650 = icmp.ne v1648, v1649
	cmp	rdi, -1
	setne	dil
	# v1651 = iconst.i32 188
	mov	r8d, 188
	# v1652 = select v1650, v1651, v1645
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1653 = iconst.i64 2
	mov	edi, 2
	# v1654 = iconst.i64 3
	mov	r8d, 3
	# v1655 = umulhi v1653, v1654
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1656 = iconst.i64 0
	# v1657 = icmp.ne v1655, v1656
	cmp	rdi, 0
	setne	dil
	# v1658 = iconst.i32 189
	mov	r8d, 189
	# v1659 = select v1657, v1658, v1652
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1660 = iconst.i64 2
	mov	edi, 2
	# v1661 = iconst.i64 3
	mov	r8d, 3
	# v1662 = smulhi v1660, v1661
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1663 = iconst.i64 0
	# v1664 = icmp.ne v1662, v1663
	cmp	rdi, 0
	setne	dil
	# v1665 = iconst.i32 190
	mov	r8d, 190
	# v1666 = select v1664, v1665, v1659
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1667 = iconst.i64 2
	mov	edi, 2
	# v1668 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1669 = umulhi v1667, v1668
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1670 = iconst.i64 0
	# v1671 = icmp.ne v1669, v1670
	cmp	rdi, 0
	setne	dil
	# v1672 = iconst.i32 191
	mov	r8d, 191
	# v1673 = select v1671, v1672, v1666
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1674 = iconst.i64 2
	mov	edi, 2
	# v1675 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1676 = smulhi v1674, v1675
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1677 = iconst.i64 0
	# v1678 = icmp.ne v1676, v1677
	cmp	rdi, 0
	setne	dil
	# v1679 = iconst.i32 192
	mov	r8d, 192
	# v1680 = select v1678, v1679, v1673
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1681 = iconst.i64 -1
	mov	rdi, -1
	# v1682 = iconst.i64 -1
	mov	r8, -1
	# v1683 = umulhi v1681, v1682
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1684 = iconst.i64 -2
	# v1685 = icmp.ne v1683, v1684
	cmp	rdi, -2
	setne	dil
	# v1686 = iconst.i32 193
	mov	r8d, 193
	# v1687 = select v1685, v1686, v1680
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1688 = iconst.i64 -1
	mov	rdi, -1
	# v1689 = iconst.i64 -1
	mov	r8, -1
	# v1690 = smulhi v1688, v1689
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1691 = iconst.i64 0
	# v1692 = icmp.ne v1690, v1691
	cmp	rdi, 0
	setne	dil
	# v1693 = iconst.i32 194
	mov	r8d, 194
	# v1694 = select v1692, v1693, v1687
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1695 = iconst.i64 -1
	mov	rdi, -1
	# v1696 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1697 = umulhi v1695, v1696
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1698 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1699 = icmp.ne v1697, v1698
	cmp	rdi, r8
	setne	dil
	# v1700 = iconst.i32 195
	mov	r8d, 195
	# v1701 = select v1699, v1700, v1694
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1702 = iconst.i64 -1
	mov	rdi, -1
	# v1703 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1704 = smulhi v1702, v1703
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1705 = iconst.i64 0
	# v1706 = icmp.ne v1704, v1705
	cmp	rdi, 0
	setne	dil
	# v1707 = iconst.i32 196
	mov	r8d, 196
	# v1708 = select v1706, v1707, v1701
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1709 = iconst.i64 -1
	mov	rdi, -1
	# v1710 = iconst.i64 3
	mov	r8d, 3
	# v1711 = umulhi v1709, v1710
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1712 = iconst.i64 2
	# v1713 = icmp.ne v1711, v1712
	cmp	rdi, 2
	setne	dil
	# v1714 = iconst.i32 197
	mov	r8d, 197
	# v1715 = select v1713, v1714, v1708
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1716 = iconst.i64 -1
	mov	rdi, -1
	# v1717 = iconst.i64 3
	mov	r8d, 3
	# v1718 = smulhi v1716, v1717
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1719 = iconst.i64 -1
	# v1720 = icmp.ne v1718, v1719
	cmp	rdi, -1
	setne	dil
	# v1721 = iconst.i32 198
	mov	r8d, 198
	# v1722 = select v1720, v1721, v1715
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1723 = iconst.i64 -1
	mov	rdi, -1
	# v1724 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1725 = umulhi v1723, v1724
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1726 = iconst.i64 9223372036854775806
	movabs	r8, 9223372036854775806
	# v1727 = icmp.ne v1725, v1726
	cmp	rdi, r8
	setne	dil
	# v1728 = iconst.i32 199
	mov	r8d, 199
	# v1729 = select v1727, v1728, v1722
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1730 = iconst.i64 -1
	mov	rdi, -1
	# v1731 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1732 = smulhi v1730, v1731
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1733 = iconst.i64 -1
	# v1734 = icmp.ne v1732, v1733
	cmp	rdi, -1
	setne	dil
	# v1735 = iconst.i32 200
	mov	r8d, 200
	# v1736 = select v1734, v1735, v1729
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1737 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v1738 = iconst.i64 -1
	mov	r8, -1
	# v1739 = umulhi v1737, v1738
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1740 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1741 = icmp.ne v1739, v1740
	cmp	rdi, r8
	setne	dil
	# v1742 = iconst.i32 201
	mov	r8d, 201
	# v1743 = select v1741, v1742, v1736
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1744 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v1745 = iconst.i64 -1
	mov	r8, -1
	# v1746 = smulhi v1744, v1745
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1747 = iconst.i64 0
	# v1748 = icmp.ne v1746, v1747
	cmp	rdi, 0
	setne	dil
	# v1749 = iconst.i32 202
	mov	r8d, 202
	# v1750 = select v1748, v1749, v1743
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1751 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v1752 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1753 = umulhi v1751, v1752
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1754 = iconst.i64 4611686018427387904
	movabs	r8, 4611686018427387904
	# v1755 = icmp.ne v1753, v1754
	cmp	rdi, r8
	setne	dil
	# v1756 = iconst.i32 203
	mov	r8d, 203
	# v1757 = select v1755, v1756, v1750
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1758 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v1759 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1760 = smulhi v1758, v1759
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1761 = iconst.i64 4611686018427387904
	movabs	r8, 4611686018427387904
	# v1762 = icmp.ne v1760, v1761
	cmp	rdi, r8
	setne	dil
	# v1763 = iconst.i32 204
	mov	r8d, 204
	# v1764 = select v1762, v1763, v1757
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1765 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v1766 = iconst.i64 3
	mov	r8d, 3
	# v1767 = umulhi v1765, v1766
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1768 = iconst.i64 1
	# v1769 = icmp.ne v1767, v1768
	cmp	rdi, 1
	setne	dil
	# v1770 = iconst.i32 205
	mov	r8d, 205
	# v1771 = select v1769, v1770, v1764
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1772 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v1773 = iconst.i64 3
	mov	r8d, 3
	# v1774 = smulhi v1772, v1773
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1775 = iconst.i64 -2
	# v1776 = icmp.ne v1774, v1775
	cmp	rdi, -2
	setne	dil
	# v1777 = iconst.i32 206
	mov	r8d, 206
	# v1778 = select v1776, v1777, v1771
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1779 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v1780 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1781 = umulhi v1779, v1780
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1782 = iconst.i64 4611686018427387903
	movabs	r8, 4611686018427387903
	# v1783 = icmp.ne v1781, v1782
	cmp	rdi, r8
	setne	dil
	# v1784 = iconst.i32 207
	mov	r8d, 207
	# v1785 = select v1783, v1784, v1778
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1786 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v1787 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1788 = smulhi v1786, v1787
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1789 = iconst.i64 -4611686018427387904
	movabs	r8, 13835058055282163712
	# v1790 = icmp.ne v1788, v1789
	cmp	rdi, r8
	setne	dil
	# v1791 = iconst.i32 208
	mov	r8d, 208
	# v1792 = select v1790, v1791, v1785
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1793 = iconst.i64 9223372036854775807
	movabs	rdi, 9223372036854775807
	# v1794 = iconst.i64 -1
	mov	r8, -1
	# v1795 = umulhi v1793, v1794
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1796 = iconst.i64 9223372036854775806
	movabs	r8, 9223372036854775806
	# v1797 = icmp.ne v1795, v1796
	cmp	rdi, r8
	setne	dil
	# v1798 = iconst.i32 209
	mov	r8d, 209
	# v1799 = select v1797, v1798, v1792
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1800 = iconst.i64 9223372036854775807
	movabs	rdi, 9223372036854775807
	# v1801 = iconst.i64 -1
	mov	r8, -1
	# v1802 = smulhi v1800, v1801
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1803 = iconst.i64 -1
	# v1804 = icmp.ne v1802, v1803
	cmp	rdi, -1
	setne	dil
	# v1805 = iconst.i32 210
	mov	r8d, 210
	# v1806 = select v1804, v1805, v1799
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1807 = iconst.i64 9223372036854775807
	movabs	rdi, 9223372036854775807
	# v1808 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1809 = umulhi v1807, v1808
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1810 = iconst.i64 4611686018427387903
	movabs	r8, 4611686018427387903
	# v1811 = icmp.ne v1809, v1810
	cmp	rdi, r8
	setne	dil
	# v1812 = iconst.i32 211
	mov	r8d, 211
	# v1813 = select v1811, v1812, v1806
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1814 = iconst.i64 9223372036854775807
	movabs	rdi, 9223372036854775807
	# v1815 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1816 = smulhi v1814, v1815
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1817 = iconst.i64 -4611686018427387904
	movabs	r8, 13835058055282163712
	# v1818 = icmp.ne v1816, v1817
	cmp	rdi, r8
	setne	dil
	# v1819 = iconst.i32 212
	mov	r8d, 212
	# v1820 = select v1818, v1819, v1813
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1821 = iconst.i64 9223372036854775807
	movabs	rdi, 9223372036854775807
	# v1822 = iconst.i64 3
	mov	r8d, 3
	# v1823 = umulhi v1821, v1822
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1824 = iconst.i64 1
	# v1825 = icmp.ne v1823, v1824
	cmp	rdi, 1
	setne	dil
	# v1826 = iconst.i32 213
	mov	r8d, 213
	# v1827 = select v1825, v1826, v1820
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1828 = iconst.i64 9223372036854775807
	movabs	rdi, 9223372036854775807
	# v1829 = iconst.i64 3
	mov	r8d, 3
	# v1830 = smulhi v1828, v1829
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1831 = iconst.i64 1
	# v1832 = icmp.ne v1830, v1831
	cmp	rdi, 1
	setne	dil
	# v1833 = iconst.i32 214
	mov	r8d, 214
	# v1834 = select v1832, v1833, v1827
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1835 = iconst.i64 9223372036854775807
	movabs	rdi, 9223372036854775807
	# v1836 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1837 = umulhi v1835, v1836
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1838 = iconst.i64 4611686018427387903
	movabs	r8, 4611686018427387903
	# v1839 = icmp.ne v1837, v1838
	cmp	rdi, r8
	setne	dil
	# v1840 = iconst.i32 215
	mov	r8d, 215
	# v1841 = select v1839, v1840, v1834
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1842 = iconst.i64 9223372036854775807
	movabs	rdi, 9223372036854775807
	# v1843 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1844 = smulhi v1842, v1843
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1845 = iconst.i64 4611686018427387903
	movabs	r8, 4611686018427387903
	# v1846 = icmp.ne v1844, v1845
	cmp	rdi, r8
	setne	dil
	# v1847 = iconst.i32 216
	mov	r8d, 216
	# v1848 = select v1846, v1847, v1841
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1849 = iconst.i64 6510615555426900570
	movabs	rdi, 6510615555426900570
	# v1850 = iconst.i64 -1
	mov	r8, -1
	# v1851 = umulhi v1849, v1850
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1852 = iconst.i64 6510615555426900569
	movabs	r8, 6510615555426900569
	# v1853 = icmp.ne v1851, v1852
	cmp	rdi, r8
	setne	dil
	# v1854 = iconst.i32 217
	mov	r8d, 217
	# v1855 = select v1853, v1854, v1848
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1856 = iconst.i64 6510615555426900570
	movabs	rdi, 6510615555426900570
	# v1857 = iconst.i64 -1
	mov	r8, -1
	# v1858 = smulhi v1856, v1857
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1859 = iconst.i64 -1
	# v1860 = icmp.ne v1858, v1859
	cmp	rdi, -1
	setne	dil
	# v1861 = iconst.i32 218
	mov	r8d, 218
	# v1862 = select v1860, v1861, v1855
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1863 = iconst.i64 6510615555426900570
	movabs	rdi, 6510615555426900570
	# v1864 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1865 = umulhi v1863, v1864
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1866 = iconst.i64 3255307777713450285
	movabs	r8, 3255307777713450285
	# v1867 = icmp.ne v1865, v1866
	cmp	rdi, r8
	setne	dil
	# v1868 = iconst.i32 219
	mov	r8d, 219
	# v1869 = select v1867, v1868, v1862
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1870 = iconst.i64 6510615555426900570
	movabs	rdi, 6510615555426900570
	# v1871 = iconst.i64 -9223372036854775808
	movabs	r8, 9223372036854775808
	# v1872 = smulhi v1870, v1871
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1873 = iconst.i64 -3255307777713450285
	movabs	r8, 15191436295996101331
	# v1874 = icmp.ne v1872, v1873
	cmp	rdi, r8
	setne	dil
	# v1875 = iconst.i32 220
	mov	r8d, 220
	# v1876 = select v1874, v1875, v1869
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1877 = iconst.i64 6510615555426900570
	movabs	rdi, 6510615555426900570
	# v1878 = iconst.i64 3
	mov	r8d, 3
	# v1879 = umulhi v1877, v1878
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1880 = iconst.i64 1
	# v1881 = icmp.ne v1879, v1880
	cmp	rdi, 1
	setne	dil
	# v1882 = iconst.i32 221
	mov	r8d, 221
	# v1883 = select v1881, v1882, v1876
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1884 = iconst.i64 6510615555426900570
	movabs	rdi, 6510615555426900570
	# v1885 = iconst.i64 3
	mov	r8d, 3
	# v1886 = smulhi v1884, v1885
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1887 = iconst.i64 1
	# v1888 = icmp.ne v1886, v1887
	cmp	rdi, 1
	setne	dil
	# v1889 = iconst.i32 222
	mov	r8d, 222
	# v1890 = select v1888, v1889, v1883
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1891 = iconst.i64 6510615555426900570
	movabs	rdi, 6510615555426900570
	# v1892 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1893 = umulhi v1891, v1892
	mov	rax, rdi
	mul	r8
	mov	rdi, rdx
	# v1894 = iconst.i64 3255307777713450284
	movabs	r8, 3255307777713450284
	# v1895 = icmp.ne v1893, v1894
	cmp	rdi, r8
	setne	dil
	# v1896 = iconst.i32 223
	mov	r8d, 223
	# v1897 = select v1895, v1896, v1890
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v1898 = iconst.i64 6510615555426900570
	movabs	rdi, 6510615555426900570
	# v1899 = iconst.i64 9223372036854775807
	movabs	r8, 9223372036854775807
	# v1900 = smulhi v1898, v1899
	mov	rax, rdi
	imul	r8
	mov	rdi, rdx
	# v1901 = iconst.i64 3255307777713450284
	movabs	r8, 3255307777713450284
	# v1902 = icmp.ne v1900, v1901
	cmp	rdi, r8
	setne	dil
	# v1903 = iconst.i32 224
	mov	r8d, 224
	# v1904 = select v1902, v1903, v1897
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# return v1904
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
