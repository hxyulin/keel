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
	# v1 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v2 = trunc.i8 v1
	# v3 = iconst.i64 -6502179613573254144
	movabs	r8, 11944564460136297472
	# v4 = trunc.i8 v3
	# v5 = rotl v2, v4
	mov	ecx, r8d
	rol	dil, cl
	# v6 = iconst.i8 -17
	# v7 = icmp.ne v5, v6
	cmp	dil, 239
	setne	dil
	# v8 = iconst.i32 1
	mov	r8d, 1
	# v9 = select v7, v8, v0
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v10 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v11 = trunc.i8 v10
	# v12 = iconst.i64 -6502179613573254144
	movabs	r8, 11944564460136297472
	# v13 = trunc.i8 v12
	# v14 = rotr v11, v13
	mov	ecx, r8d
	ror	dil, cl
	# v15 = iconst.i8 -17
	# v16 = icmp.ne v14, v15
	cmp	dil, 239
	setne	dil
	# v17 = iconst.i32 2
	mov	r8d, 2
	# v18 = select v16, v17, v9
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v19 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v20 = trunc.i8 v19
	# v21 = iconst.i64 -6502179613573254143
	movabs	r8, 11944564460136297473
	# v22 = trunc.i8 v21
	# v23 = rotl v20, v22
	mov	ecx, r8d
	rol	dil, cl
	# v24 = iconst.i8 -33
	# v25 = icmp.ne v23, v24
	cmp	dil, 223
	setne	dil
	# v26 = iconst.i32 3
	mov	r8d, 3
	# v27 = select v25, v26, v18
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v28 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v29 = trunc.i8 v28
	# v30 = iconst.i64 -6502179613573254143
	movabs	r8, 11944564460136297473
	# v31 = trunc.i8 v30
	# v32 = rotr v29, v31
	mov	ecx, r8d
	ror	dil, cl
	# v33 = iconst.i8 -9
	# v34 = icmp.ne v32, v33
	cmp	dil, 247
	setne	dil
	# v35 = iconst.i32 4
	mov	r8d, 4
	# v36 = select v34, v35, v27
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v37 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v38 = trunc.i8 v37
	# v39 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v40 = trunc.i8 v39
	# v41 = rotl v38, v40
	mov	ecx, r8d
	rol	dil, cl
	# v42 = iconst.i8 127
	# v43 = icmp.ne v41, v42
	cmp	dil, 127
	setne	dil
	# v44 = iconst.i32 5
	mov	r8d, 5
	# v45 = select v43, v44, v36
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v46 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v47 = trunc.i8 v46
	# v48 = iconst.i64 -6502179613573254141
	movabs	r8, 11944564460136297475
	# v49 = trunc.i8 v48
	# v50 = rotr v47, v49
	mov	ecx, r8d
	ror	dil, cl
	# v51 = iconst.i8 -3
	# v52 = icmp.ne v50, v51
	cmp	dil, 253
	setne	dil
	# v53 = iconst.i32 6
	mov	r8d, 6
	# v54 = select v52, v53, v45
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v55 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v56 = trunc.i8 v55
	# v57 = iconst.i64 -6502179613573254137
	movabs	r8, 11944564460136297479
	# v58 = trunc.i8 v57
	# v59 = rotl v56, v58
	mov	ecx, r8d
	rol	dil, cl
	# v60 = iconst.i8 -9
	# v61 = icmp.ne v59, v60
	cmp	dil, 247
	setne	dil
	# v62 = iconst.i32 7
	mov	r8d, 7
	# v63 = select v61, v62, v54
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v64 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v65 = trunc.i8 v64
	# v66 = iconst.i64 -6502179613573254137
	movabs	r8, 11944564460136297479
	# v67 = trunc.i8 v66
	# v68 = rotr v65, v67
	mov	ecx, r8d
	ror	dil, cl
	# v69 = iconst.i8 -33
	# v70 = icmp.ne v68, v69
	cmp	dil, 223
	setne	dil
	# v71 = iconst.i32 8
	mov	r8d, 8
	# v72 = select v70, v71, v63
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v73 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v74 = trunc.i8 v73
	# v75 = iconst.i64 -6502179613573254136
	movabs	r8, 11944564460136297480
	# v76 = trunc.i8 v75
	# v77 = rotl v74, v76
	mov	ecx, r8d
	rol	dil, cl
	# v78 = iconst.i8 -17
	# v79 = icmp.ne v77, v78
	cmp	dil, 239
	setne	dil
	# v80 = iconst.i32 9
	mov	r8d, 9
	# v81 = select v79, v80, v72
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v82 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v83 = trunc.i8 v82
	# v84 = iconst.i64 -6502179613573254136
	movabs	r8, 11944564460136297480
	# v85 = trunc.i8 v84
	# v86 = rotr v83, v85
	mov	ecx, r8d
	ror	dil, cl
	# v87 = iconst.i8 -17
	# v88 = icmp.ne v86, v87
	cmp	dil, 239
	setne	dil
	# v89 = iconst.i32 10
	mov	r8d, 10
	# v90 = select v88, v89, v81
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v91 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v92 = trunc.i8 v91
	# v93 = iconst.i64 -6502179613573254133
	movabs	r8, 11944564460136297483
	# v94 = trunc.i8 v93
	# v95 = rotl v92, v94
	mov	ecx, r8d
	rol	dil, cl
	# v96 = iconst.i8 127
	# v97 = icmp.ne v95, v96
	cmp	dil, 127
	setne	dil
	# v98 = iconst.i32 11
	mov	r8d, 11
	# v99 = select v97, v98, v90
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v100 = iconst.i64 -6502179613573253905
	movabs	rdi, 11944564460136297711
	# v101 = trunc.i8 v100
	# v102 = iconst.i64 -6502179613573254133
	movabs	r8, 11944564460136297483
	# v103 = trunc.i8 v102
	# v104 = rotr v101, v103
	mov	ecx, r8d
	ror	dil, cl
	# v105 = iconst.i8 -3
	# v106 = icmp.ne v104, v105
	cmp	dil, 253
	setne	dil
	# v107 = iconst.i32 12
	mov	r8d, 12
	# v108 = select v106, v107, v99
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v109 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v110 = trunc.i16 v109
	# v111 = iconst.i64 -6502179613573316608
	movabs	r8, 11944564460136235008
	# v112 = trunc.i16 v111
	# v113 = rotl v110, v112
	mov	ecx, r8d
	rol	di, cl
	# v114 = iconst.i16 -12817
	# v115 = icmp.ne v113, v114
	cmp	di, 52719
	setne	dil
	# v116 = iconst.i32 13
	mov	r8d, 13
	# v117 = select v115, v116, v108
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v118 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v119 = trunc.i16 v118
	# v120 = iconst.i64 -6502179613573316608
	movabs	r8, 11944564460136235008
	# v121 = trunc.i16 v120
	# v122 = rotr v119, v121
	mov	ecx, r8d
	ror	di, cl
	# v123 = iconst.i16 -12817
	# v124 = icmp.ne v122, v123
	cmp	di, 52719
	setne	dil
	# v125 = iconst.i32 14
	mov	r8d, 14
	# v126 = select v124, v125, v117
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v127 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v128 = trunc.i16 v127
	# v129 = iconst.i64 -6502179613573316607
	movabs	r8, 11944564460136235009
	# v130 = trunc.i16 v129
	# v131 = rotl v128, v130
	mov	ecx, r8d
	rol	di, cl
	# v132 = iconst.i16 -25633
	# v133 = icmp.ne v131, v132
	cmp	di, 39903
	setne	dil
	# v134 = iconst.i32 15
	mov	r8d, 15
	# v135 = select v133, v134, v126
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v136 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v137 = trunc.i16 v136
	# v138 = iconst.i64 -6502179613573316607
	movabs	r8, 11944564460136235009
	# v139 = trunc.i16 v138
	# v140 = rotr v137, v139
	mov	ecx, r8d
	ror	di, cl
	# v141 = iconst.i16 -6409
	# v142 = icmp.ne v140, v141
	cmp	di, 59127
	setne	dil
	# v143 = iconst.i32 16
	mov	r8d, 16
	# v144 = select v142, v143, v135
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v145 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v146 = trunc.i16 v145
	# v147 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v148 = trunc.i16 v147
	# v149 = rotl v146, v148
	mov	ecx, r8d
	rol	di, cl
	# v150 = iconst.i16 28542
	# v151 = icmp.ne v149, v150
	cmp	di, 28542
	setne	dil
	# v152 = iconst.i32 17
	mov	r8d, 17
	# v153 = select v151, v152, v144
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v154 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v155 = trunc.i16 v154
	# v156 = iconst.i64 -6502179613573316605
	movabs	r8, 11944564460136235011
	# v157 = trunc.i16 v156
	# v158 = rotr v155, v157
	mov	ecx, r8d
	ror	di, cl
	# v159 = iconst.i16 -1603
	# v160 = icmp.ne v158, v159
	cmp	di, 63933
	setne	dil
	# v161 = iconst.i32 18
	mov	r8d, 18
	# v162 = select v160, v161, v153
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v163 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v164 = trunc.i16 v163
	# v165 = iconst.i64 -6502179613573316593
	movabs	r8, 11944564460136235023
	# v166 = trunc.i16 v165
	# v167 = rotl v164, v166
	mov	ecx, r8d
	rol	di, cl
	# v168 = iconst.i16 -6409
	# v169 = icmp.ne v167, v168
	cmp	di, 59127
	setne	dil
	# v170 = iconst.i32 19
	mov	r8d, 19
	# v171 = select v169, v170, v162
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v172 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v173 = trunc.i16 v172
	# v174 = iconst.i64 -6502179613573316593
	movabs	r8, 11944564460136235023
	# v175 = trunc.i16 v174
	# v176 = rotr v173, v175
	mov	ecx, r8d
	ror	di, cl
	# v177 = iconst.i16 -25633
	# v178 = icmp.ne v176, v177
	cmp	di, 39903
	setne	dil
	# v179 = iconst.i32 20
	mov	r8d, 20
	# v180 = select v178, v179, v171
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v181 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v182 = trunc.i16 v181
	# v183 = iconst.i64 -6502179613573316592
	movabs	r8, 11944564460136235024
	# v184 = trunc.i16 v183
	# v185 = rotl v182, v184
	mov	ecx, r8d
	rol	di, cl
	# v186 = iconst.i16 -12817
	# v187 = icmp.ne v185, v186
	cmp	di, 52719
	setne	dil
	# v188 = iconst.i32 21
	mov	r8d, 21
	# v189 = select v187, v188, v180
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v190 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v191 = trunc.i16 v190
	# v192 = iconst.i64 -6502179613573316592
	movabs	r8, 11944564460136235024
	# v193 = trunc.i16 v192
	# v194 = rotr v191, v193
	mov	ecx, r8d
	ror	di, cl
	# v195 = iconst.i16 -12817
	# v196 = icmp.ne v194, v195
	cmp	di, 52719
	setne	dil
	# v197 = iconst.i32 22
	mov	r8d, 22
	# v198 = select v196, v197, v189
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v199 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v200 = trunc.i16 v199
	# v201 = iconst.i64 -6502179613573316589
	movabs	r8, 11944564460136235027
	# v202 = trunc.i16 v201
	# v203 = rotl v200, v202
	mov	ecx, r8d
	rol	di, cl
	# v204 = iconst.i16 28542
	# v205 = icmp.ne v203, v204
	cmp	di, 28542
	setne	dil
	# v206 = iconst.i32 23
	mov	r8d, 23
	# v207 = select v205, v206, v198
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v208 = iconst.i64 -6502179613573263889
	movabs	rdi, 11944564460136287727
	# v209 = trunc.i16 v208
	# v210 = iconst.i64 -6502179613573316589
	movabs	r8, 11944564460136235027
	# v211 = trunc.i16 v210
	# v212 = rotr v209, v211
	mov	ecx, r8d
	ror	di, cl
	# v213 = iconst.i16 -1603
	# v214 = icmp.ne v212, v213
	cmp	di, 63933
	setne	dil
	# v215 = iconst.i32 24
	mov	r8d, 24
	# v216 = select v214, v215, v207
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v217 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v218 = trunc.i32 v217
	# v219 = iconst.i64 -6502179615371427840
	movabs	r8, 11944564458338123776
	# v220 = trunc.i32 v219
	# v221 = rotl v218, v220
	mov	ecx, r8d
	rol	edi, cl
	# v222 = iconst.i32 -1985229329
	# v223 = icmp.ne v221, v222
	cmp	edi, 2309737967
	setne	dil
	# v224 = iconst.i32 25
	mov	r8d, 25
	# v225 = select v223, v224, v216
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v226 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v227 = trunc.i32 v226
	# v228 = iconst.i64 -6502179615371427840
	movabs	r8, 11944564458338123776
	# v229 = trunc.i32 v228
	# v230 = rotr v227, v229
	mov	ecx, r8d
	ror	edi, cl
	# v231 = iconst.i32 -1985229329
	# v232 = icmp.ne v230, v231
	cmp	edi, 2309737967
	setne	dil
	# v233 = iconst.i32 26
	mov	r8d, 26
	# v234 = select v232, v233, v225
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v235 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v236 = trunc.i32 v235
	# v237 = iconst.i64 -6502179615371427839
	movabs	r8, 11944564458338123777
	# v238 = trunc.i32 v237
	# v239 = rotl v236, v238
	mov	ecx, r8d
	rol	edi, cl
	# v240 = iconst.i32 324508639
	# v241 = icmp.ne v239, v240
	cmp	edi, 324508639
	setne	dil
	# v242 = iconst.i32 27
	mov	r8d, 27
	# v243 = select v241, v242, v234
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v244 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v245 = trunc.i32 v244
	# v246 = iconst.i64 -6502179615371427839
	movabs	r8, 11944564458338123777
	# v247 = trunc.i32 v246
	# v248 = rotr v245, v247
	mov	ecx, r8d
	ror	edi, cl
	# v249 = iconst.i32 -992614665
	# v250 = icmp.ne v248, v249
	cmp	edi, 3302352631
	setne	dil
	# v251 = iconst.i32 28
	mov	r8d, 28
	# v252 = select v250, v251, v243
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v253 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v254 = trunc.i32 v253
	# v255 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v256 = trunc.i32 v255
	# v257 = rotl v254, v256
	mov	ecx, r8d
	rol	edi, cl
	# v258 = iconst.i32 1298034556
	# v259 = icmp.ne v257, v258
	cmp	edi, 1298034556
	setne	dil
	# v260 = iconst.i32 29
	mov	r8d, 29
	# v261 = select v259, v260, v252
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v262 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v263 = trunc.i32 v262
	# v264 = iconst.i64 -6502179615371427837
	movabs	r8, 11944564458338123779
	# v265 = trunc.i32 v264
	# v266 = rotr v263, v265
	mov	ecx, r8d
	ror	edi, cl
	# v267 = iconst.i32 -248153667
	# v268 = icmp.ne v266, v267
	cmp	edi, 4046813629
	setne	dil
	# v269 = iconst.i32 30
	mov	r8d, 30
	# v270 = select v268, v269, v261
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v271 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v272 = trunc.i32 v271
	# v273 = iconst.i64 -6502179615371427809
	movabs	r8, 11944564458338123807
	# v274 = trunc.i32 v273
	# v275 = rotl v272, v274
	mov	ecx, r8d
	rol	edi, cl
	# v276 = iconst.i32 -992614665
	# v277 = icmp.ne v275, v276
	cmp	edi, 3302352631
	setne	dil
	# v278 = iconst.i32 31
	mov	r8d, 31
	# v279 = select v277, v278, v270
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v280 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v281 = trunc.i32 v280
	# v282 = iconst.i64 -6502179615371427809
	movabs	r8, 11944564458338123807
	# v283 = trunc.i32 v282
	# v284 = rotr v281, v283
	mov	ecx, r8d
	ror	edi, cl
	# v285 = iconst.i32 324508639
	# v286 = icmp.ne v284, v285
	cmp	edi, 324508639
	setne	dil
	# v287 = iconst.i32 32
	mov	r8d, 32
	# v288 = select v286, v287, v279
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v289 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v290 = trunc.i32 v289
	# v291 = iconst.i64 -6502179615371427808
	movabs	r8, 11944564458338123808
	# v292 = trunc.i32 v291
	# v293 = rotl v290, v292
	mov	ecx, r8d
	rol	edi, cl
	# v294 = iconst.i32 -1985229329
	# v295 = icmp.ne v293, v294
	cmp	edi, 2309737967
	setne	dil
	# v296 = iconst.i32 33
	mov	r8d, 33
	# v297 = select v295, v296, v288
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v298 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v299 = trunc.i32 v298
	# v300 = iconst.i64 -6502179615371427808
	movabs	r8, 11944564458338123808
	# v301 = trunc.i32 v300
	# v302 = rotr v299, v301
	mov	ecx, r8d
	ror	edi, cl
	# v303 = iconst.i32 -1985229329
	# v304 = icmp.ne v302, v303
	cmp	edi, 2309737967
	setne	dil
	# v305 = iconst.i32 34
	mov	r8d, 34
	# v306 = select v304, v305, v297
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v307 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v308 = trunc.i32 v307
	# v309 = iconst.i64 -6502179615371427805
	movabs	r8, 11944564458338123811
	# v310 = trunc.i32 v309
	# v311 = rotl v308, v310
	mov	ecx, r8d
	rol	edi, cl
	# v312 = iconst.i32 1298034556
	# v313 = icmp.ne v311, v312
	cmp	edi, 1298034556
	setne	dil
	# v314 = iconst.i32 35
	mov	r8d, 35
	# v315 = select v313, v314, v306
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v316 = iconst.i64 -6502179613061689873
	movabs	rdi, 11944564460647861743
	# v317 = trunc.i32 v316
	# v318 = iconst.i64 -6502179615371427805
	movabs	r8, 11944564458338123811
	# v319 = trunc.i32 v318
	# v320 = rotr v317, v319
	mov	ecx, r8d
	ror	edi, cl
	# v321 = iconst.i32 -248153667
	# v322 = icmp.ne v320, v321
	cmp	edi, 4046813629
	setne	dil
	# v323 = iconst.i32 36
	mov	r8d, 36
	# v324 = select v322, v323, v315
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v325 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v326 = iconst.i64 0
	# v327 = rotl v325, v326
	rol	rdi, 0
	# v328 = iconst.i64 81985529216486895
	movabs	r8, 81985529216486895
	# v329 = icmp.ne v327, v328
	cmp	rdi, r8
	setne	dil
	# v330 = iconst.i32 37
	mov	r8d, 37
	# v331 = select v329, v330, v324
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v332 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v333 = iconst.i64 0
	# v334 = rotr v332, v333
	ror	rdi, 0
	# v335 = iconst.i64 81985529216486895
	movabs	r8, 81985529216486895
	# v336 = icmp.ne v334, v335
	cmp	rdi, r8
	setne	dil
	# v337 = iconst.i32 38
	mov	r8d, 38
	# v338 = select v336, v337, v331
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v339 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v340 = iconst.i64 1
	# v341 = rotl v339, v340
	rol	rdi, 1
	# v342 = iconst.i64 163971058432973790
	movabs	r8, 163971058432973790
	# v343 = icmp.ne v341, v342
	cmp	rdi, r8
	setne	dil
	# v344 = iconst.i32 39
	mov	r8d, 39
	# v345 = select v343, v344, v338
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v346 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v347 = iconst.i64 1
	# v348 = rotr v346, v347
	ror	rdi, 1
	# v349 = iconst.i64 -9182379272246532361
	movabs	r8, 9264364801463019255
	# v350 = icmp.ne v348, v349
	cmp	rdi, r8
	setne	dil
	# v351 = iconst.i32 40
	mov	r8d, 40
	# v352 = select v350, v351, v345
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v353 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v354 = iconst.i64 3
	# v355 = rotl v353, v354
	rol	rdi, 3
	# v356 = iconst.i64 655884233731895160
	movabs	r8, 655884233731895160
	# v357 = icmp.ne v355, v356
	cmp	rdi, r8
	setne	dil
	# v358 = iconst.i32 41
	mov	r8d, 41
	# v359 = select v357, v358, v352
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v360 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v361 = iconst.i64 3
	# v362 = rotr v360, v361
	ror	rdi, 3
	# v363 = iconst.i64 -2295594818061633091
	movabs	r8, 16151149255647918525
	# v364 = icmp.ne v362, v363
	cmp	rdi, r8
	setne	dil
	# v365 = iconst.i32 42
	mov	r8d, 42
	# v366 = select v364, v365, v359
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v367 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v368 = iconst.i64 63
	# v369 = rotl v367, v368
	rol	rdi, 63
	# v370 = iconst.i64 -9182379272246532361
	movabs	r8, 9264364801463019255
	# v371 = icmp.ne v369, v370
	cmp	rdi, r8
	setne	dil
	# v372 = iconst.i32 43
	mov	r8d, 43
	# v373 = select v371, v372, v366
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v374 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v375 = iconst.i64 63
	# v376 = rotr v374, v375
	ror	rdi, 63
	# v377 = iconst.i64 163971058432973790
	movabs	r8, 163971058432973790
	# v378 = icmp.ne v376, v377
	cmp	rdi, r8
	setne	dil
	# v379 = iconst.i32 44
	mov	r8d, 44
	# v380 = select v378, v379, v373
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v381 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v382 = iconst.i64 64
	# v383 = rotl v381, v382
	rol	rdi, 0
	# v384 = iconst.i64 81985529216486895
	movabs	r8, 81985529216486895
	# v385 = icmp.ne v383, v384
	cmp	rdi, r8
	setne	dil
	# v386 = iconst.i32 45
	mov	r8d, 45
	# v387 = select v385, v386, v380
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v388 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v389 = iconst.i64 64
	# v390 = rotr v388, v389
	ror	rdi, 0
	# v391 = iconst.i64 81985529216486895
	movabs	r8, 81985529216486895
	# v392 = icmp.ne v390, v391
	cmp	rdi, r8
	setne	dil
	# v393 = iconst.i32 46
	mov	r8d, 46
	# v394 = select v392, v393, v387
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v395 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v396 = iconst.i64 67
	# v397 = rotl v395, v396
	rol	rdi, 3
	# v398 = iconst.i64 655884233731895160
	movabs	r8, 655884233731895160
	# v399 = icmp.ne v397, v398
	cmp	rdi, r8
	setne	dil
	# v400 = iconst.i32 47
	mov	r8d, 47
	# v401 = select v399, v400, v394
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v402 = iconst.i64 81985529216486895
	movabs	rdi, 81985529216486895
	# v403 = iconst.i64 67
	# v404 = rotr v402, v403
	ror	rdi, 3
	# v405 = iconst.i64 -2295594818061633091
	movabs	r8, 16151149255647918525
	# v406 = icmp.ne v404, v405
	cmp	rdi, r8
	setne	dil
	# v407 = iconst.i32 48
	mov	r8d, 48
	# v408 = select v406, v407, v401
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# v409 = iconst.i64 -9223372036854775807
	movabs	rdi, 9223372036854775809
	# v410 = iconst.i64 -6502179613573254079
	movabs	r8, 11944564460136297537
	# v411 = trunc.i8 v410
	# v412 = rotl v409, v411
	mov	ecx, r8d
	rol	rdi, cl
	# v413 = iconst.i64 3
	# v414 = icmp.ne v412, v413
	cmp	rdi, 3
	setne	dil
	# v415 = iconst.i32 49
	mov	r8d, 49
	# v416 = select v414, v415, v408
	test	dil, dil
	mov	rax, rsi
	cmovne	rax, r8
	mov	rsi, rax
	# return v416
	mov	rax, rsi
	mov	rsp, rbp
	pop	rbp
	ret
.Lmain_end:
	.cfi_endproc
	.size "main", .-"main"
	.section	".note.GNU-stack","",@progbits
