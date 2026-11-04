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
	# v3 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v4 = trunc.i8 v3
	# v5 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v6 = trunc.i8 v5
	# v7 = select v2, v4, v6
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v8 = iconst.i8 -103
	# v9 = icmp.ne v7, v8
	cmp	dil, 153
	setne	dil
	# v10 = iconst.i32 1
	mov	r8d, 1
	# v11 = select v9, v10, v0
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v12 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v13 = trunc.i8 v12
	# v14 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v15 = trunc.i16 v14
	# v16 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v17 = trunc.i16 v16
	# v18 = select v13, v15, v17
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v19 = iconst.i16 -26215
	# v20 = icmp.ne v18, v19
	cmp	di, 39321
	setne	dil
	# v21 = iconst.i32 2
	mov	r8d, 2
	# v22 = select v20, v21, v11
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v23 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v24 = trunc.i8 v23
	# v25 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v26 = trunc.i32 v25
	# v27 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v28 = trunc.i32 v27
	# v29 = select v24, v26, v28
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v30 = iconst.i32 1431673241
	# v31 = icmp.ne v29, v30
	cmp	edi, 1431673241
	setne	dil
	# v32 = iconst.i32 3
	mov	r8d, 3
	# v33 = select v31, v32, v22
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v34 = iconst.i64 -6502179613573254144
	movabs	rdi, 11944564460136297472
	# v35 = trunc.i8 v34
	# v36 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v37 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v38 = select v35, v36, v37
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v39 = iconst.i64 8608461802446363033
	movabs	r8, 8608461802446363033
	# v40 = icmp.ne v38, v39
	cmp	rdi, r8
	setne	dil
	# v41 = iconst.i32 4
	mov	r8d, 4
	# v42 = select v40, v41, v33
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v43 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v44 = trunc.i8 v43
	# v45 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v46 = trunc.i8 v45
	# v47 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v48 = trunc.i8 v47
	# v49 = select v44, v46, v48
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v50 = iconst.i8 68
	# v51 = icmp.ne v49, v50
	cmp	dil, 68
	setne	dil
	# v52 = iconst.i32 5
	mov	r8d, 5
	# v53 = select v51, v52, v42
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v54 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v55 = trunc.i8 v54
	# v56 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v57 = trunc.i16 v56
	# v58 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v59 = trunc.i16 v58
	# v60 = select v55, v57, v59
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v61 = iconst.i16 17476
	# v62 = icmp.ne v60, v61
	cmp	di, 17476
	setne	dil
	# v63 = iconst.i32 6
	mov	r8d, 6
	# v64 = select v62, v63, v53
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v65 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v66 = trunc.i8 v65
	# v67 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v68 = trunc.i32 v67
	# v69 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v70 = trunc.i32 v69
	# v71 = select v66, v68, v70
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v72 = iconst.i32 858997828
	# v73 = icmp.ne v71, v72
	cmp	edi, 858997828
	setne	dil
	# v74 = iconst.i32 7
	mov	r8d, 7
	# v75 = select v73, v74, v64
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v76 = iconst.i64 -6502179613573254143
	movabs	rdi, 11944564460136297473
	# v77 = trunc.i8 v76
	# v78 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v79 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v80 = select v77, v78, v79
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v81 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v82 = icmp.ne v80, v81
	cmp	rdi, r8
	setne	dil
	# v83 = iconst.i32 8
	mov	r8d, 8
	# v84 = select v82, v83, v75
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v85 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v86 = trunc.i8 v85
	# v87 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v88 = trunc.i8 v87
	# v89 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v90 = trunc.i8 v89
	# v91 = select v86, v88, v90
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v92 = iconst.i8 68
	# v93 = icmp.ne v91, v92
	cmp	dil, 68
	setne	dil
	# v94 = iconst.i32 9
	mov	r8d, 9
	# v95 = select v93, v94, v84
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v96 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v97 = trunc.i8 v96
	# v98 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v99 = trunc.i16 v98
	# v100 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v101 = trunc.i16 v100
	# v102 = select v97, v99, v101
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v103 = iconst.i16 17476
	# v104 = icmp.ne v102, v103
	cmp	di, 17476
	setne	dil
	# v105 = iconst.i32 10
	mov	r8d, 10
	# v106 = select v104, v105, v95
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v107 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v108 = trunc.i8 v107
	# v109 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v110 = trunc.i32 v109
	# v111 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v112 = trunc.i32 v111
	# v113 = select v108, v110, v112
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v114 = iconst.i32 858997828
	# v115 = icmp.ne v113, v114
	cmp	edi, 858997828
	setne	dil
	# v116 = iconst.i32 11
	mov	r8d, 11
	# v117 = select v115, v116, v106
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v118 = iconst.i64 -6502179613573254016
	movabs	rdi, 11944564460136297600
	# v119 = trunc.i8 v118
	# v120 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v121 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v122 = select v119, v120, v121
	test	dil, dil
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v123 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v124 = icmp.ne v122, v123
	cmp	rdi, r8
	setne	dil
	# v125 = iconst.i32 12
	mov	r8d, 12
	# v126 = select v124, v125, v117
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v127 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v128 = trunc.i16 v127
	# v129 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v130 = trunc.i8 v129
	# v131 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v132 = trunc.i8 v131
	# v133 = select v128, v130, v132
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v134 = iconst.i8 -103
	# v135 = icmp.ne v133, v134
	cmp	dil, 153
	setne	dil
	# v136 = iconst.i32 13
	mov	r8d, 13
	# v137 = select v135, v136, v126
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v138 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v139 = trunc.i16 v138
	# v140 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v141 = trunc.i16 v140
	# v142 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v143 = trunc.i16 v142
	# v144 = select v139, v141, v143
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v145 = iconst.i16 -26215
	# v146 = icmp.ne v144, v145
	cmp	di, 39321
	setne	dil
	# v147 = iconst.i32 14
	mov	r8d, 14
	# v148 = select v146, v147, v137
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v149 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v150 = trunc.i16 v149
	# v151 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v152 = trunc.i32 v151
	# v153 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v154 = trunc.i32 v153
	# v155 = select v150, v152, v154
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v156 = iconst.i32 1431673241
	# v157 = icmp.ne v155, v156
	cmp	edi, 1431673241
	setne	dil
	# v158 = iconst.i32 15
	mov	r8d, 15
	# v159 = select v157, v158, v148
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v160 = iconst.i64 -6502179613573316608
	movabs	rdi, 11944564460136235008
	# v161 = trunc.i16 v160
	# v162 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v163 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v164 = select v161, v162, v163
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v165 = iconst.i64 8608461802446363033
	movabs	r8, 8608461802446363033
	# v166 = icmp.ne v164, v165
	cmp	rdi, r8
	setne	dil
	# v167 = iconst.i32 16
	mov	r8d, 16
	# v168 = select v166, v167, v159
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v169 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v170 = trunc.i16 v169
	# v171 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v172 = trunc.i8 v171
	# v173 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v174 = trunc.i8 v173
	# v175 = select v170, v172, v174
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v176 = iconst.i8 68
	# v177 = icmp.ne v175, v176
	cmp	dil, 68
	setne	dil
	# v178 = iconst.i32 17
	mov	r8d, 17
	# v179 = select v177, v178, v168
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v180 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v181 = trunc.i16 v180
	# v182 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v183 = trunc.i16 v182
	# v184 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v185 = trunc.i16 v184
	# v186 = select v181, v183, v185
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v187 = iconst.i16 17476
	# v188 = icmp.ne v186, v187
	cmp	di, 17476
	setne	dil
	# v189 = iconst.i32 18
	mov	r8d, 18
	# v190 = select v188, v189, v179
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v191 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v192 = trunc.i16 v191
	# v193 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v194 = trunc.i32 v193
	# v195 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v196 = trunc.i32 v195
	# v197 = select v192, v194, v196
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v198 = iconst.i32 858997828
	# v199 = icmp.ne v197, v198
	cmp	edi, 858997828
	setne	dil
	# v200 = iconst.i32 19
	mov	r8d, 19
	# v201 = select v199, v200, v190
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v202 = iconst.i64 -6502179613573316607
	movabs	rdi, 11944564460136235009
	# v203 = trunc.i16 v202
	# v204 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v205 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v206 = select v203, v204, v205
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v207 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v208 = icmp.ne v206, v207
	cmp	rdi, r8
	setne	dil
	# v209 = iconst.i32 20
	mov	r8d, 20
	# v210 = select v208, v209, v201
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v211 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v212 = trunc.i16 v211
	# v213 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v214 = trunc.i8 v213
	# v215 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v216 = trunc.i8 v215
	# v217 = select v212, v214, v216
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v218 = iconst.i8 68
	# v219 = icmp.ne v217, v218
	cmp	dil, 68
	setne	dil
	# v220 = iconst.i32 21
	mov	r8d, 21
	# v221 = select v219, v220, v210
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v222 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v223 = trunc.i16 v222
	# v224 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v225 = trunc.i16 v224
	# v226 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v227 = trunc.i16 v226
	# v228 = select v223, v225, v227
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v229 = iconst.i16 17476
	# v230 = icmp.ne v228, v229
	cmp	di, 17476
	setne	dil
	# v231 = iconst.i32 22
	mov	r8d, 22
	# v232 = select v230, v231, v221
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v233 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v234 = trunc.i16 v233
	# v235 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v236 = trunc.i32 v235
	# v237 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v238 = trunc.i32 v237
	# v239 = select v234, v236, v238
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v240 = iconst.i32 858997828
	# v241 = icmp.ne v239, v240
	cmp	edi, 858997828
	setne	dil
	# v242 = iconst.i32 23
	mov	r8d, 23
	# v243 = select v241, v242, v232
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v244 = iconst.i64 -6502179613573283840
	movabs	rdi, 11944564460136267776
	# v245 = trunc.i16 v244
	# v246 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v247 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v248 = select v245, v246, v247
	test	di, di
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v249 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v250 = icmp.ne v248, v249
	cmp	rdi, r8
	setne	dil
	# v251 = iconst.i32 24
	mov	r8d, 24
	# v252 = select v250, v251, v243
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v253 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v254 = trunc.i32 v253
	# v255 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v256 = trunc.i8 v255
	# v257 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v258 = trunc.i8 v257
	# v259 = select v254, v256, v258
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v260 = iconst.i8 -103
	# v261 = icmp.ne v259, v260
	cmp	dil, 153
	setne	dil
	# v262 = iconst.i32 25
	mov	r8d, 25
	# v263 = select v261, v262, v252
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v264 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v265 = trunc.i32 v264
	# v266 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v267 = trunc.i16 v266
	# v268 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v269 = trunc.i16 v268
	# v270 = select v265, v267, v269
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v271 = iconst.i16 -26215
	# v272 = icmp.ne v270, v271
	cmp	di, 39321
	setne	dil
	# v273 = iconst.i32 26
	mov	r8d, 26
	# v274 = select v272, v273, v263
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v275 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v276 = trunc.i32 v275
	# v277 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v278 = trunc.i32 v277
	# v279 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v280 = trunc.i32 v279
	# v281 = select v276, v278, v280
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v282 = iconst.i32 1431673241
	# v283 = icmp.ne v281, v282
	cmp	edi, 1431673241
	setne	dil
	# v284 = iconst.i32 27
	mov	r8d, 27
	# v285 = select v283, v284, v274
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v286 = iconst.i64 -6502179615371427840
	movabs	rdi, 11944564458338123776
	# v287 = trunc.i32 v286
	# v288 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v289 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v290 = select v287, v288, v289
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v291 = iconst.i64 8608461802446363033
	movabs	r8, 8608461802446363033
	# v292 = icmp.ne v290, v291
	cmp	rdi, r8
	setne	dil
	# v293 = iconst.i32 28
	mov	r8d, 28
	# v294 = select v292, v293, v285
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v295 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v296 = trunc.i32 v295
	# v297 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v298 = trunc.i8 v297
	# v299 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v300 = trunc.i8 v299
	# v301 = select v296, v298, v300
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v302 = iconst.i8 68
	# v303 = icmp.ne v301, v302
	cmp	dil, 68
	setne	dil
	# v304 = iconst.i32 29
	mov	r8d, 29
	# v305 = select v303, v304, v294
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v306 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v307 = trunc.i32 v306
	# v308 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v309 = trunc.i16 v308
	# v310 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v311 = trunc.i16 v310
	# v312 = select v307, v309, v311
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v313 = iconst.i16 17476
	# v314 = icmp.ne v312, v313
	cmp	di, 17476
	setne	dil
	# v315 = iconst.i32 30
	mov	r8d, 30
	# v316 = select v314, v315, v305
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v317 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v318 = trunc.i32 v317
	# v319 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v320 = trunc.i32 v319
	# v321 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v322 = trunc.i32 v321
	# v323 = select v318, v320, v322
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v324 = iconst.i32 858997828
	# v325 = icmp.ne v323, v324
	cmp	edi, 858997828
	setne	dil
	# v326 = iconst.i32 31
	mov	r8d, 31
	# v327 = select v325, v326, v316
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v328 = iconst.i64 -6502179615371427839
	movabs	rdi, 11944564458338123777
	# v329 = trunc.i32 v328
	# v330 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v331 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v332 = select v329, v330, v331
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v333 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v334 = icmp.ne v332, v333
	cmp	rdi, r8
	setne	dil
	# v335 = iconst.i32 32
	mov	r8d, 32
	# v336 = select v334, v335, v327
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v337 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v338 = trunc.i32 v337
	# v339 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v340 = trunc.i8 v339
	# v341 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v342 = trunc.i8 v341
	# v343 = select v338, v340, v342
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v344 = iconst.i8 68
	# v345 = icmp.ne v343, v344
	cmp	dil, 68
	setne	dil
	# v346 = iconst.i32 33
	mov	r8d, 33
	# v347 = select v345, v346, v336
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v348 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v349 = trunc.i32 v348
	# v350 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v351 = trunc.i16 v350
	# v352 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v353 = trunc.i16 v352
	# v354 = select v349, v351, v353
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v355 = iconst.i16 17476
	# v356 = icmp.ne v354, v355
	cmp	di, 17476
	setne	dil
	# v357 = iconst.i32 34
	mov	r8d, 34
	# v358 = select v356, v357, v347
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v359 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v360 = trunc.i32 v359
	# v361 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v362 = trunc.i32 v361
	# v363 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v364 = trunc.i32 v363
	# v365 = select v360, v362, v364
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v366 = iconst.i32 858997828
	# v367 = icmp.ne v365, v366
	cmp	edi, 858997828
	setne	dil
	# v368 = iconst.i32 35
	mov	r8d, 35
	# v369 = select v367, v368, v358
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v370 = iconst.i64 -6502179613223944192
	movabs	rdi, 11944564460485607424
	# v371 = trunc.i32 v370
	# v372 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v373 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v374 = select v371, v372, v373
	test	edi, edi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v375 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v376 = icmp.ne v374, v375
	cmp	rdi, r8
	setne	dil
	# v377 = iconst.i32 36
	mov	r8d, 36
	# v378 = select v376, v377, v369
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v379 = iconst.i64 0
	xor	edi, edi
	# v380 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v381 = trunc.i8 v380
	# v382 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v383 = trunc.i8 v382
	# v384 = select v379, v381, v383
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v385 = iconst.i8 -103
	# v386 = icmp.ne v384, v385
	cmp	dil, 153
	setne	dil
	# v387 = iconst.i32 37
	mov	r8d, 37
	# v388 = select v386, v387, v378
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v389 = iconst.i64 0
	xor	edi, edi
	# v390 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v391 = trunc.i16 v390
	# v392 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v393 = trunc.i16 v392
	# v394 = select v389, v391, v393
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v395 = iconst.i16 -26215
	# v396 = icmp.ne v394, v395
	cmp	di, 39321
	setne	dil
	# v397 = iconst.i32 38
	mov	r8d, 38
	# v398 = select v396, v397, v388
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v399 = iconst.i64 0
	xor	edi, edi
	# v400 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v401 = trunc.i32 v400
	# v402 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v403 = trunc.i32 v402
	# v404 = select v399, v401, v403
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v405 = iconst.i32 1431673241
	# v406 = icmp.ne v404, v405
	cmp	edi, 1431673241
	setne	dil
	# v407 = iconst.i32 39
	mov	r8d, 39
	# v408 = select v406, v407, v398
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v409 = iconst.i64 0
	xor	edi, edi
	# v410 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v411 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v412 = select v409, v410, v411
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v413 = iconst.i64 8608461802446363033
	movabs	r8, 8608461802446363033
	# v414 = icmp.ne v412, v413
	cmp	rdi, r8
	setne	dil
	# v415 = iconst.i32 40
	mov	r8d, 40
	# v416 = select v414, v415, v408
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v417 = iconst.i64 1
	mov	edi, 1
	# v418 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v419 = trunc.i8 v418
	# v420 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v421 = trunc.i8 v420
	# v422 = select v417, v419, v421
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v423 = iconst.i8 68
	# v424 = icmp.ne v422, v423
	cmp	dil, 68
	setne	dil
	# v425 = iconst.i32 41
	mov	r8d, 41
	# v426 = select v424, v425, v416
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v427 = iconst.i64 1
	mov	edi, 1
	# v428 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v429 = trunc.i16 v428
	# v430 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v431 = trunc.i16 v430
	# v432 = select v427, v429, v431
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v433 = iconst.i16 17476
	# v434 = icmp.ne v432, v433
	cmp	di, 17476
	setne	dil
	# v435 = iconst.i32 42
	mov	r8d, 42
	# v436 = select v434, v435, v426
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v437 = iconst.i64 1
	mov	edi, 1
	# v438 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v439 = trunc.i32 v438
	# v440 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v441 = trunc.i32 v440
	# v442 = select v437, v439, v441
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v443 = iconst.i32 858997828
	# v444 = icmp.ne v442, v443
	cmp	edi, 858997828
	setne	dil
	# v445 = iconst.i32 43
	mov	r8d, 43
	# v446 = select v444, v445, v436
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v447 = iconst.i64 1
	mov	edi, 1
	# v448 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v449 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v450 = select v447, v448, v449
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v451 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v452 = icmp.ne v450, v451
	cmp	rdi, r8
	setne	dil
	# v453 = iconst.i32 44
	mov	r8d, 44
	# v454 = select v452, v453, v446
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v455 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v456 = iconst.i64 -6502179613573254076
	movabs	r8, 11944564460136297540
	# v457 = trunc.i8 v456
	# v458 = iconst.i64 -6502179613573253991
	movabs	r9, 11944564460136297625
	# v459 = trunc.i8 v458
	# v460 = select v455, v457, v459
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v461 = iconst.i8 68
	# v462 = icmp.ne v460, v461
	cmp	dil, 68
	setne	dil
	# v463 = iconst.i32 45
	mov	r8d, 45
	# v464 = select v462, v463, v454
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v465 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v466 = iconst.i64 -6502179613573299132
	movabs	r8, 11944564460136252484
	# v467 = trunc.i16 v466
	# v468 = iconst.i64 -6502179613573277287
	movabs	r9, 11944564460136274329
	# v469 = trunc.i16 v468
	# v470 = select v465, v467, v469
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v471 = iconst.i16 17476
	# v472 = icmp.ne v470, v471
	cmp	di, 17476
	setne	dil
	# v473 = iconst.i32 46
	mov	r8d, 46
	# v474 = select v472, v473, v464
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v475 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v476 = iconst.i64 -6502179614512430012
	movabs	r8, 11944564459197121604
	# v477 = trunc.i32 v476
	# v478 = iconst.i64 -6502179613939754599
	movabs	r9, 11944564459769797017
	# v479 = trunc.i32 v478
	# v480 = select v475, v477, v479
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v481 = iconst.i32 858997828
	# v482 = icmp.ne v480, v481
	cmp	edi, 858997828
	setne	dil
	# v483 = iconst.i32 47
	mov	r8d, 47
	# v484 = select v482, v483, v474
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v485 = iconst.i64 -9223372036854775808
	movabs	rdi, 9223372036854775808
	# v486 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v487 = iconst.i64 8608461802446363033
	movabs	r9, 8608461802446363033
	# v488 = select v485, v486, v487
	test	rdi, rdi
	mov	rax, r9
	cmovne	rax, r8
	mov	rdi, rax
	# v489 = iconst.i64 1229801703532086340
	movabs	r8, 1229801703532086340
	# v490 = icmp.ne v488, v489
	cmp	rdi, r8
	setne	dil
	# v491 = iconst.i32 48
	mov	r8d, 48
	# v492 = select v490, v491, v484
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# return v492
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
