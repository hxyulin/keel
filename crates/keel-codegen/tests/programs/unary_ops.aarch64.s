	.section	__TEXT,__text
	.section	__TEXT,__text
	.globl	_main
	.p2align	2
_main:
Lmain_begin:
	; regalloc: v0=x9 v1=x10 v2=x10 v3=x10 v4=x11 v5=x10 v6=x11 v7=x9 v8=x10 v9=x10 v10=x10 v11=x11 v12=x10 v13=x11 v14=x9 v15=x10 v16=x10 v17=x10 v18=x11 v19=x10 v20=x11 v21=x9 v22=x10 v23=x10 v24=x10 v25=x11 v26=x10 v27=x11 v28=x9 v29=x10 v30=x10 v31=x10 v32=x11 v33=x10 v34=x11 v35=x9 v36=x10 v37=x10 v38=x10 v39=x11 v40=x10 v41=x11 v42=x9 v43=x10 v44=x10 v45=x10 v46=x11 v47=x10 v48=x11 v49=x9 v50=x10 v51=x10 v52=x10 v53=x11 v54=x10 v55=x11 v56=x9 v57=x10 v58=x10 v59=x10 v60=x11 v61=x10 v62=x11 v63=x9 v64=x10 v65=x10 v66=x10 v67=x11 v68=x10 v69=x11 v70=x9 v71=x10 v72=x10 v73=x10 v74=x11 v75=x10 v76=x11 v77=x9 v78=x10 v79=x10 v80=x10 v81=x11 v82=x10 v83=x11 v84=x9 v85=x10 v86=x10 v87=x10 v88=x11 v89=x10 v90=x11 v91=x9 v92=x10 v93=x10 v94=x10 v95=x11 v96=x10 v97=x11 v98=x9 v99=x10 v100=x10 v101=x10 v102=x11 v103=x10 v104=x11 v105=x9 v106=x10 v107=x10 v108=x10 v109=x11 v110=x10 v111=x11 v112=x9 v113=x10 v114=x10 v115=x10 v116=x11 v117=x10 v118=x11 v119=x9 v120=x10 v121=x10 v122=x10 v123=x11 v124=x10 v125=x11 v126=x9 v127=x10 v128=x10 v129=x10 v130=x11 v131=x10 v132=x11 v133=x9 v134=x10 v135=x10 v136=x10 v137=x11 v138=x10 v139=x11 v140=x9 v141=x10 v142=x10 v143=x10 v144=x11 v145=x10 v146=x11 v147=x9 v148=x10 v149=x10 v150=x10 v151=x11 v152=x10 v153=x11 v154=x9 v155=x10 v156=x10 v157=x10 v158=x11 v159=x10 v160=x11 v161=x9 v162=x10 v163=x10 v164=x10 v165=x11 v166=x10 v167=x11 v168=x9 v169=x10 v170=x10 v171=x10 v172=x11 v173=x10 v174=x11 v175=x9 v176=x10 v177=x10 v178=x10 v179=x11 v180=x10 v181=x11 v182=x9 v183=x10 v184=x10 v185=x10 v186=x11 v187=x10 v188=x11 v189=x9 v190=x10 v191=x10 v192=x10 v193=x11 v194=x10 v195=x11 v196=x9 v197=x10 v198=x10 v199=x10 v200=x11 v201=x10 v202=x11 v203=x9 v204=x10 v205=x10 v206=x10 v207=x11 v208=x10 v209=x11 v210=x9 v211=x10 v212=x10 v213=x10 v214=x11 v215=x10 v216=x11 v217=x9 v218=x10 v219=x10 v220=x10 v221=x11 v222=x10 v223=x11 v224=x9 v225=x10 v226=x10 v227=x10 v228=x11 v229=x10 v230=x11 v231=x9 v232=x10 v233=x10 v234=x10 v235=x11 v236=x10 v237=x11 v238=x9 v239=x10 v240=x10 v241=x10 v242=x11 v243=x10 v244=x11 v245=x9 v246=x10 v247=x10 v248=x10 v249=x11 v250=x10 v251=x11 v252=x9 v253=x10 v254=x10 v255=x10 v256=x11 v257=x10 v258=x11 v259=x9 v260=x10 v261=x10 v262=x10 v263=x11 v264=x10 v265=x11 v266=x9 v267=x10 v268=x10 v269=x10 v270=x11 v271=x10 v272=x11 v273=x9 v274=x10 v275=x10 v276=x10 v277=x11 v278=x10 v279=x11 v280=x9 v281=x10 v282=x10 v283=x10 v284=x11 v285=x10 v286=x11 v287=x9 v288=x10 v289=x10 v290=x10 v291=x11 v292=x10 v293=x11 v294=x9 v295=x10 v296=x10 v297=x10 v298=x11 v299=x10 v300=x11 v301=x9 v302=x10 v303=x10 v304=x10 v305=x11 v306=x10 v307=x11 v308=x9 v309=x10 v310=x10 v311=x10 v312=x11 v313=x10 v314=x11 v315=x9 v316=x10 v317=x10 v318=x10 v319=x11 v320=x10 v321=x11 v322=x9 v323=x10 v324=x10 v325=x10 v326=x11 v327=x10 v328=x11 v329=x9 v330=x10 v331=x10 v332=x10 v333=x11 v334=x10 v335=x11 v336=x9 v337=x10 v338=x10 v339=x10 v340=x11 v341=x10 v342=x11 v343=x9 v344=x10 v345=x10 v346=x10 v347=x11 v348=x10 v349=x11 v350=x9 v351=x10 v352=x10 v353=x10 v354=x11 v355=x10 v356=x11 v357=x9 v358=x10 v359=x10 v360=x10 v361=x11 v362=x10 v363=x11 v364=x9 v365=x10 v366=x10 v367=x10 v368=x11 v369=x10 v370=x11 v371=x9 v372=x10 v373=x10 v374=x10 v375=x11 v376=x10 v377=x11 v378=x9 v379=x10 v380=x10 v381=x10 v382=x11 v383=x10 v384=x11 v385=x9 v386=x10 v387=x10 v388=x10 v389=x11 v390=x10 v391=x11 v392=x9 v393=x10 v394=x10 v395=x10 v396=x11 v397=x10 v398=x11 v399=x9 v400=x10 v401=x10 v402=x10 v403=x11 v404=x10 v405=x11 v406=x9 v407=x10 v408=x10 v409=x10 v410=x11 v411=x10 v412=x11 v413=x9 v414=x10 v415=x10 v416=x10 v417=x11 v418=x10 v419=x11 v420=x9 v421=x10 v422=x10 v423=x10 v424=x11 v425=x10 v426=x11 v427=x9 v428=x10 v429=x10 v430=x10 v431=x11 v432=x10 v433=x11 v434=x9 v435=x10 v436=x10 v437=x10 v438=x11 v439=x10 v440=x11 v441=x9 v442=x10 v443=x10 v444=x10 v445=x11 v446=x10 v447=x11 v448=x9 v449=x10 v450=x10 v451=x10 v452=x11 v453=x10 v454=x11 v455=x9 v456=x10 v457=x10 v458=x10 v459=x11 v460=x10 v461=x11 v462=x9 v463=x10 v464=x10 v465=x10 v466=x11 v467=x10 v468=x11 v469=x9 v470=x10 v471=x10 v472=x10 v473=x11 v474=x10 v475=x11 v476=x9 v477=x10 v478=x10 v479=x10 v480=x11 v481=x10 v482=x11 v483=x9 v484=x10 v485=x10 v486=x10 v487=x11 v488=x10 v489=x11 v490=x9 v491=x10 v492=x10 v493=x10 v494=x11 v495=x10 v496=x11 v497=x9 v498=x10 v499=x10 v500=x10 v501=x11 v502=x10 v503=x11 v504=x9 v505=x10 v506=x10 v507=x10 v508=x11 v509=x10 v510=x11 v511=x9 v512=x10 v513=x10 v514=x10 v515=x11 v516=x10 v517=x11 v518=x9 v519=x10 v520=x10 v521=x10 v522=x11 v523=x10 v524=x11 v525=x9 v526=x10 v527=x10 v528=x10 v529=x11 v530=x10 v531=x11 v532=x9 v533=x10 v534=x10 v535=x10 v536=x11 v537=x10 v538=x11 v539=x9 v540=x10 v541=x10 v542=x10 v543=x11 v544=x10 v545=x11 v546=x9 v547=x10 v548=x10 v549=x10 v550=x11 v551=x10 v552=x11 v553=x9 v554=x10 v555=x10 v556=x10 v557=x11 v558=x10 v559=x11 v560=x9 v561=x10 v562=x10 v563=x10 v564=x11 v565=x10 v566=x11 v567=x9 v568=x10 v569=x10 v570=x10 v571=x11 v572=x10 v573=x11 v574=x9 v575=x10 v576=x10 v577=x10 v578=x11 v579=x10 v580=x11 v581=x9 v582=x10 v583=x10 v584=x10 v585=x11 v586=x10 v587=x11 v588=x9 v589=x10 v590=x10 v591=x10 v592=x11 v593=x10 v594=x11 v595=x9 v596=x10 v597=x10 v598=x10 v599=x11 v600=x10 v601=x11 v602=x9 v603=x10 v604=x10 v605=x10 v606=x11 v607=x10 v608=x11 v609=x9 v610=x10 v611=x10 v612=x10 v613=x11 v614=x10 v615=x11 v616=x9 v617=x10 v618=x10 v619=x10 v620=x11 v621=x10 v622=x11 v623=x9 v624=x10 v625=x10 v626=x10 v627=x11 v628=x10 v629=x11 v630=x9 v631=x10 v632=x10 v633=x10 v634=x11 v635=x10 v636=x11 v637=x9 v638=x10 v639=x10 v640=x10 v641=x11 v642=x10 v643=x11 v644=x9 v645=x10 v646=x10 v647=x10 v648=x11 v649=x10 v650=x11 v651=x9 v652=x10 v653=x10 v654=x10 v655=x11 v656=x10 v657=x11 v658=x9 v659=x10 v660=x10 v661=x10 v662=x11 v663=x10 v664=x11 v665=x9 v666=x10 v667=x10 v668=x10 v669=x11 v670=x10 v671=x11 v672=x9 v673=x10 v674=x10 v675=x10 v676=x11 v677=x10 v678=x11 v679=x9 v680=x10 v681=x10 v682=x10 v683=x11 v684=x10 v685=x11 v686=x9 v687=x10 v688=x10 v689=x10 v690=x11 v691=x10 v692=x11 v693=x9 v694=x10 v695=x10 v696=x10 v697=x11 v698=x10 v699=x11 v700=x9 v701=x10 v702=x10 v703=x10 v704=x11 v705=x10 v706=x11 v707=x9 v708=x10 v709=x10 v710=x10 v711=x11 v712=x10 v713=x11 v714=x9 v715=x10 v716=x10 v717=x10 v718=x11 v719=x10 v720=x11 v721=x9 v722=x10 v723=x10 v724=x10 v725=x11 v726=x10 v727=x11 v728=x9 v729=x10 v730=x10 v731=x10 v732=x11 v733=x10 v734=x11 v735=x9 v736=x10 v737=x10 v738=x10 v739=x11 v740=x10 v741=x11 v742=x9 v743=x10 v744=x10 v745=x10 v746=x11 v747=x10 v748=x11 v749=x9 v750=x10 v751=x10 v752=x10 v753=x11 v754=x10 v755=x11 v756=x9 v757=x10 v758=x10 v759=x10 v760=x11 v761=x10 v762=x11 v763=x9 v764=x10 v765=x10 v766=x10 v767=x11 v768=x10 v769=x11 v770=x9 v771=x10 v772=x10 v773=x10 v774=x11 v775=x10 v776=x11 v777=x9 v778=x10 v779=x10 v780=x10 v781=x11 v782=x10 v783=x11 v784=x9 v785=x10 v786=x10 v787=x10 v788=x11 v789=x10 v790=x11 v791=x9 v792=x10 v793=x10 v794=x10 v795=x11 v796=x10 v797=x11 v798=x9 v799=x10 v800=x10 v801=x10 v802=x11 v803=x10 v804=x11 v805=x9 v806=x10 v807=x10 v808=x10 v809=x11 v810=x10 v811=x11 v812=x9 v813=x10 v814=x10 v815=x10 v816=x11 v817=x10 v818=x11 v819=x9 v820=x10 v821=x10 v822=x10 v823=x11 v824=x10 v825=x11 v826=x9 v827=x10 v828=x10 v829=x10 v830=x11 v831=x10 v832=x11 v833=x9 v834=x10 v835=x10 v836=x11 v837=x10 v838=x11 v839=x9 v840=x10 v841=x10 v842=x11 v843=x10 v844=x11 v845=x9 v846=x10 v847=x10 v848=x11 v849=x10 v850=x11 v851=x9 v852=x10 v853=x10 v854=x11 v855=x10 v856=x11 v857=x9 v858=x10 v859=x10 v860=x11 v861=x10 v862=x11 v863=x9 v864=x10 v865=x10 v866=x11 v867=x10 v868=x11 v869=x9 v870=x10 v871=x10 v872=x11 v873=x10 v874=x11 v875=x9 v876=x10 v877=x10 v878=x11 v879=x10 v880=x11 v881=x9 v882=x10 v883=x10 v884=x11 v885=x10 v886=x11 v887=x9 v888=x10 v889=x10 v890=x11 v891=x10 v892=x11 v893=x9 v894=x10 v895=x10 v896=x11 v897=x10 v898=x11 v899=x9 v900=x10 v901=x10 v902=x11 v903=x10 v904=x11 v905=x9 v906=x10 v907=x10 v908=x11 v909=x10 v910=x11 v911=x9 v912=x10 v913=x10 v914=x11 v915=x10 v916=x11 v917=x9 v918=x10 v919=x10 v920=x11 v921=x10 v922=x11 v923=x9 v924=x10 v925=x10 v926=x11 v927=x10 v928=x11 v929=x9 v930=x10 v931=x10 v932=x11 v933=x10 v934=x11 v935=x9 v936=x10 v937=x10 v938=x11 v939=x10 v940=x11 v941=x9 v942=x10 v943=x10 v944=x11 v945=x10 v946=x11 v947=x9 v948=x10 v949=x10 v950=x11 v951=x10 v952=x11 v953=x9 v954=x10 v955=x10 v956=x11 v957=x10 v958=x11 v959=x9 v960=x10 v961=x10 v962=x11 v963=x10 v964=x11 v965=x9 v966=x10 v967=x10 v968=x11 v969=x10 v970=x11 v971=x9 v972=x10 v973=x10 v974=x11 v975=x10 v976=x11 v977=x9 v978=x10 v979=x10 v980=x11 v981=x10 v982=x11 v983=x9 v984=x10 v985=x10 v986=x11 v987=x10 v988=x11 v989=x9 v990=x10 v991=x10 v992=x11 v993=x10 v994=x11 v995=x9 v996=x10 v997=x10 v998=x11 v999=x10 v1000=x11 v1001=x9 v1002=x10 v1003=x10 v1004=x11 v1005=x10 v1006=x11 v1007=x9 v1008=x10 v1009=x10 v1010=x11 v1011=x10 v1012=x11 v1013=x9 v1014=x10 v1015=x10 v1016=x11 v1017=x10 v1018=x11 v1019=x9 v1020=x10 v1021=x10 v1022=x11 v1023=x10 v1024=x11 v1025=x9 v1026=x10 v1027=x10 v1028=x11 v1029=x10 v1030=x11 v1031=x9 v1032=x10 v1033=x10 v1034=x11 v1035=x10 v1036=x11 v1037=x9 v1038=x10 v1039=x10 v1040=x11 v1041=x10 v1042=x11 v1043=x9 v1044=x10 v1045=x10 v1046=x11 v1047=x10 v1048=x11 v1049=x9 v1050=x10 v1051=x10 v1052=x11 v1053=x10 v1054=x11 v1055=x9 v1056=x10 v1057=x10 v1058=x11 v1059=x10 v1060=x11 v1061=x9 v1062=x10 v1063=x10 v1064=x11 v1065=x10 v1066=x11 v1067=x9 v1068=x10 v1069=x10 v1070=x11 v1071=x10 v1072=x11 v1073=x9 v1074=x10 v1075=x10 v1076=x11 v1077=x10 v1078=x11 v1079=x9 v1080=x10 v1081=x10 v1082=x11 v1083=x10 v1084=x11 v1085=x9
	.cfi_startproc
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	.cfi_def_cfa x29, 16
	.cfi_offset x30, -8
	.cfi_offset x29, -16
Lmain_block0:
	; v0 = iconst.i32 0
	movz	x9, #0x0
	; v1 = iconst.i64 -6502179613573254144
	movz	x10, #0xf400
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v2 = trunc.i8 v1
	; v3 = ineg v2
	neg	w10, w10
	; v4 = iconst.i8 0
	; v5 = icmp.ne v3, v4
	uxtb	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v6 = iconst.i32 1
	movz	x11, #0x1
	; v7 = select v5, v6, v0
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v8 = iconst.i64 -6502179613573254144
	movz	x10, #0xf400
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v9 = trunc.i8 v8
	; v10 = bnot v9
	mvn	w10, w10
	; v11 = iconst.i8 -1
	movn	x11, #0x0
	; v12 = icmp.ne v10, v11
	uxtb	w16, w10
	uxtb	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v13 = iconst.i32 2
	movz	x11, #0x2
	; v14 = select v12, v13, v7
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v15 = iconst.i64 -6502179613573254144
	movz	x10, #0xf400
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v16 = trunc.i8 v15
	; v17 = clz v16
	uxtb	w16, w10
	clz	w10, w16
	sub	w10, w10, #24
	; v18 = iconst.i8 8
	; v19 = icmp.ne v17, v18
	uxtb	w16, w10
	cmp	w16, #8
	cset	w10, ne
	; v20 = iconst.i32 3
	movz	x11, #0x3
	; v21 = select v19, v20, v14
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v22 = iconst.i64 -6502179613573254144
	movz	x10, #0xf400
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v23 = trunc.i8 v22
	; v24 = ctz v23
	orr	w16, w10, #256
	rbit	w16, w16
	clz	w10, w16
	; v25 = iconst.i8 8
	; v26 = icmp.ne v24, v25
	uxtb	w16, w10
	cmp	w16, #8
	cset	w10, ne
	; v27 = iconst.i32 4
	movz	x11, #0x4
	; v28 = select v26, v27, v21
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v29 = iconst.i64 -6502179613573254144
	movz	x10, #0xf400
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v30 = trunc.i8 v29
	; v31 = popcnt v30
	uxtb	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v32 = iconst.i8 0
	; v33 = icmp.ne v31, v32
	uxtb	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v34 = iconst.i32 5
	movz	x11, #0x5
	; v35 = select v33, v34, v28
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v36 = iconst.i64 -6502179613573254143
	movz	x10, #0xf401
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v37 = trunc.i8 v36
	; v38 = ineg v37
	neg	w10, w10
	; v39 = iconst.i8 -1
	movn	x11, #0x0
	; v40 = icmp.ne v38, v39
	uxtb	w16, w10
	uxtb	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v41 = iconst.i32 6
	movz	x11, #0x6
	; v42 = select v40, v41, v35
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v43 = iconst.i64 -6502179613573254143
	movz	x10, #0xf401
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v44 = trunc.i8 v43
	; v45 = bnot v44
	mvn	w10, w10
	; v46 = iconst.i8 -2
	movn	x11, #0x1
	; v47 = icmp.ne v45, v46
	uxtb	w16, w10
	uxtb	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v48 = iconst.i32 7
	movz	x11, #0x7
	; v49 = select v47, v48, v42
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v50 = iconst.i64 -6502179613573254143
	movz	x10, #0xf401
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v51 = trunc.i8 v50
	; v52 = clz v51
	uxtb	w16, w10
	clz	w10, w16
	sub	w10, w10, #24
	; v53 = iconst.i8 7
	; v54 = icmp.ne v52, v53
	uxtb	w16, w10
	cmp	w16, #7
	cset	w10, ne
	; v55 = iconst.i32 8
	movz	x11, #0x8
	; v56 = select v54, v55, v49
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v57 = iconst.i64 -6502179613573254143
	movz	x10, #0xf401
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v58 = trunc.i8 v57
	; v59 = ctz v58
	orr	w16, w10, #256
	rbit	w16, w16
	clz	w10, w16
	; v60 = iconst.i8 0
	; v61 = icmp.ne v59, v60
	uxtb	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v62 = iconst.i32 9
	movz	x11, #0x9
	; v63 = select v61, v62, v56
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v64 = iconst.i64 -6502179613573254143
	movz	x10, #0xf401
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v65 = trunc.i8 v64
	; v66 = popcnt v65
	uxtb	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v67 = iconst.i8 1
	; v68 = icmp.ne v66, v67
	uxtb	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v69 = iconst.i32 10
	movz	x11, #0xa
	; v70 = select v68, v69, v63
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v71 = iconst.i64 -6502179613573253889
	movz	x10, #0xf4ff
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v72 = trunc.i8 v71
	; v73 = ineg v72
	neg	w10, w10
	; v74 = iconst.i8 1
	; v75 = icmp.ne v73, v74
	uxtb	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v76 = iconst.i32 11
	movz	x11, #0xb
	; v77 = select v75, v76, v70
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v78 = iconst.i64 -6502179613573253889
	movz	x10, #0xf4ff
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v79 = trunc.i8 v78
	; v80 = bnot v79
	mvn	w10, w10
	; v81 = iconst.i8 0
	; v82 = icmp.ne v80, v81
	uxtb	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v83 = iconst.i32 12
	movz	x11, #0xc
	; v84 = select v82, v83, v77
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v85 = iconst.i64 -6502179613573253889
	movz	x10, #0xf4ff
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v86 = trunc.i8 v85
	; v87 = clz v86
	uxtb	w16, w10
	clz	w10, w16
	sub	w10, w10, #24
	; v88 = iconst.i8 0
	; v89 = icmp.ne v87, v88
	uxtb	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v90 = iconst.i32 13
	movz	x11, #0xd
	; v91 = select v89, v90, v84
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v92 = iconst.i64 -6502179613573253889
	movz	x10, #0xf4ff
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v93 = trunc.i8 v92
	; v94 = ctz v93
	orr	w16, w10, #256
	rbit	w16, w16
	clz	w10, w16
	; v95 = iconst.i8 0
	; v96 = icmp.ne v94, v95
	uxtb	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v97 = iconst.i32 14
	movz	x11, #0xe
	; v98 = select v96, v97, v91
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v99 = iconst.i64 -6502179613573253889
	movz	x10, #0xf4ff
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v100 = trunc.i8 v99
	; v101 = popcnt v100
	uxtb	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v102 = iconst.i8 8
	; v103 = icmp.ne v101, v102
	uxtb	w16, w10
	cmp	w16, #8
	cset	w10, ne
	; v104 = iconst.i32 15
	movz	x11, #0xf
	; v105 = select v103, v104, v98
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v106 = iconst.i64 -6502179613573254016
	movz	x10, #0xf480
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v107 = trunc.i8 v106
	; v108 = ineg v107
	neg	w10, w10
	; v109 = iconst.i8 -128
	movn	x11, #0x7f
	; v110 = icmp.ne v108, v109
	uxtb	w16, w10
	uxtb	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v111 = iconst.i32 16
	movz	x11, #0x10
	; v112 = select v110, v111, v105
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v113 = iconst.i64 -6502179613573254016
	movz	x10, #0xf480
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v114 = trunc.i8 v113
	; v115 = bnot v114
	mvn	w10, w10
	; v116 = iconst.i8 127
	; v117 = icmp.ne v115, v116
	uxtb	w16, w10
	cmp	w16, #127
	cset	w10, ne
	; v118 = iconst.i32 17
	movz	x11, #0x11
	; v119 = select v117, v118, v112
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v120 = iconst.i64 -6502179613573254016
	movz	x10, #0xf480
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v121 = trunc.i8 v120
	; v122 = clz v121
	uxtb	w16, w10
	clz	w10, w16
	sub	w10, w10, #24
	; v123 = iconst.i8 0
	; v124 = icmp.ne v122, v123
	uxtb	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v125 = iconst.i32 18
	movz	x11, #0x12
	; v126 = select v124, v125, v119
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v127 = iconst.i64 -6502179613573254016
	movz	x10, #0xf480
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v128 = trunc.i8 v127
	; v129 = ctz v128
	orr	w16, w10, #256
	rbit	w16, w16
	clz	w10, w16
	; v130 = iconst.i8 7
	; v131 = icmp.ne v129, v130
	uxtb	w16, w10
	cmp	w16, #7
	cset	w10, ne
	; v132 = iconst.i32 19
	movz	x11, #0x13
	; v133 = select v131, v132, v126
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v134 = iconst.i64 -6502179613573254016
	movz	x10, #0xf480
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v135 = trunc.i8 v134
	; v136 = popcnt v135
	uxtb	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v137 = iconst.i8 1
	; v138 = icmp.ne v136, v137
	uxtb	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v139 = iconst.i32 20
	movz	x11, #0x14
	; v140 = select v138, v139, v133
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v141 = iconst.i64 -6502179613573254054
	movz	x10, #0xf45a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v142 = trunc.i8 v141
	; v143 = ineg v142
	neg	w10, w10
	; v144 = iconst.i8 -90
	movn	x11, #0x59
	; v145 = icmp.ne v143, v144
	uxtb	w16, w10
	uxtb	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v146 = iconst.i32 21
	movz	x11, #0x15
	; v147 = select v145, v146, v140
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v148 = iconst.i64 -6502179613573254054
	movz	x10, #0xf45a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v149 = trunc.i8 v148
	; v150 = bnot v149
	mvn	w10, w10
	; v151 = iconst.i8 -91
	movn	x11, #0x5a
	; v152 = icmp.ne v150, v151
	uxtb	w16, w10
	uxtb	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v153 = iconst.i32 22
	movz	x11, #0x16
	; v154 = select v152, v153, v147
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v155 = iconst.i64 -6502179613573254054
	movz	x10, #0xf45a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v156 = trunc.i8 v155
	; v157 = clz v156
	uxtb	w16, w10
	clz	w10, w16
	sub	w10, w10, #24
	; v158 = iconst.i8 1
	; v159 = icmp.ne v157, v158
	uxtb	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v160 = iconst.i32 23
	movz	x11, #0x17
	; v161 = select v159, v160, v154
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v162 = iconst.i64 -6502179613573254054
	movz	x10, #0xf45a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v163 = trunc.i8 v162
	; v164 = ctz v163
	orr	w16, w10, #256
	rbit	w16, w16
	clz	w10, w16
	; v165 = iconst.i8 1
	; v166 = icmp.ne v164, v165
	uxtb	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v167 = iconst.i32 24
	movz	x11, #0x18
	; v168 = select v166, v167, v161
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v169 = iconst.i64 -6502179613573254054
	movz	x10, #0xf45a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v170 = trunc.i8 v169
	; v171 = popcnt v170
	uxtb	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v172 = iconst.i8 4
	; v173 = icmp.ne v171, v172
	uxtb	w16, w10
	cmp	w16, #4
	cset	w10, ne
	; v174 = iconst.i32 25
	movz	x11, #0x19
	; v175 = select v173, v174, v168
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v176 = iconst.i64 -6502179613573253905
	movz	x10, #0xf4ef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v177 = trunc.i8 v176
	; v178 = ineg v177
	neg	w10, w10
	; v179 = iconst.i8 17
	; v180 = icmp.ne v178, v179
	uxtb	w16, w10
	cmp	w16, #17
	cset	w10, ne
	; v181 = iconst.i32 26
	movz	x11, #0x1a
	; v182 = select v180, v181, v175
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v183 = iconst.i64 -6502179613573253905
	movz	x10, #0xf4ef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v184 = trunc.i8 v183
	; v185 = bnot v184
	mvn	w10, w10
	; v186 = iconst.i8 16
	; v187 = icmp.ne v185, v186
	uxtb	w16, w10
	cmp	w16, #16
	cset	w10, ne
	; v188 = iconst.i32 27
	movz	x11, #0x1b
	; v189 = select v187, v188, v182
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v190 = iconst.i64 -6502179613573253905
	movz	x10, #0xf4ef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v191 = trunc.i8 v190
	; v192 = clz v191
	uxtb	w16, w10
	clz	w10, w16
	sub	w10, w10, #24
	; v193 = iconst.i8 0
	; v194 = icmp.ne v192, v193
	uxtb	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v195 = iconst.i32 28
	movz	x11, #0x1c
	; v196 = select v194, v195, v189
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v197 = iconst.i64 -6502179613573253905
	movz	x10, #0xf4ef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v198 = trunc.i8 v197
	; v199 = ctz v198
	orr	w16, w10, #256
	rbit	w16, w16
	clz	w10, w16
	; v200 = iconst.i8 0
	; v201 = icmp.ne v199, v200
	uxtb	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v202 = iconst.i32 29
	movz	x11, #0x1d
	; v203 = select v201, v202, v196
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v204 = iconst.i64 -6502179613573253905
	movz	x10, #0xf4ef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v205 = trunc.i8 v204
	; v206 = popcnt v205
	uxtb	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v207 = iconst.i8 7
	; v208 = icmp.ne v206, v207
	uxtb	w16, w10
	cmp	w16, #7
	cset	w10, ne
	; v209 = iconst.i32 30
	movz	x11, #0x1e
	; v210 = select v208, v209, v203
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v211 = iconst.i64 -6502179613573254128
	movz	x10, #0xf410
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v212 = trunc.i8 v211
	; v213 = ineg v212
	neg	w10, w10
	; v214 = iconst.i8 -16
	movn	x11, #0xf
	; v215 = icmp.ne v213, v214
	uxtb	w16, w10
	uxtb	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v216 = iconst.i32 31
	movz	x11, #0x1f
	; v217 = select v215, v216, v210
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v218 = iconst.i64 -6502179613573254128
	movz	x10, #0xf410
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v219 = trunc.i8 v218
	; v220 = bnot v219
	mvn	w10, w10
	; v221 = iconst.i8 -17
	movn	x11, #0x10
	; v222 = icmp.ne v220, v221
	uxtb	w16, w10
	uxtb	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v223 = iconst.i32 32
	movz	x11, #0x20
	; v224 = select v222, v223, v217
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v225 = iconst.i64 -6502179613573254128
	movz	x10, #0xf410
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v226 = trunc.i8 v225
	; v227 = clz v226
	uxtb	w16, w10
	clz	w10, w16
	sub	w10, w10, #24
	; v228 = iconst.i8 3
	; v229 = icmp.ne v227, v228
	uxtb	w16, w10
	cmp	w16, #3
	cset	w10, ne
	; v230 = iconst.i32 33
	movz	x11, #0x21
	; v231 = select v229, v230, v224
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v232 = iconst.i64 -6502179613573254128
	movz	x10, #0xf410
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v233 = trunc.i8 v232
	; v234 = ctz v233
	orr	w16, w10, #256
	rbit	w16, w16
	clz	w10, w16
	; v235 = iconst.i8 4
	; v236 = icmp.ne v234, v235
	uxtb	w16, w10
	cmp	w16, #4
	cset	w10, ne
	; v237 = iconst.i32 34
	movz	x11, #0x22
	; v238 = select v236, v237, v231
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v239 = iconst.i64 -6502179613573254128
	movz	x10, #0xf410
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v240 = trunc.i8 v239
	; v241 = popcnt v240
	uxtb	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v242 = iconst.i8 1
	; v243 = icmp.ne v241, v242
	uxtb	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v244 = iconst.i32 35
	movz	x11, #0x23
	; v245 = select v243, v244, v238
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v246 = iconst.i64 -6502179613573316608
	movz	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v247 = trunc.i16 v246
	; v248 = ineg v247
	neg	w10, w10
	; v249 = iconst.i16 0
	; v250 = icmp.ne v248, v249
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v251 = iconst.i32 36
	movz	x11, #0x24
	; v252 = select v250, v251, v245
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v253 = iconst.i64 -6502179613573316608
	movz	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v254 = trunc.i16 v253
	; v255 = bnot v254
	mvn	w10, w10
	; v256 = iconst.i16 -1
	movn	x11, #0x0
	; v257 = icmp.ne v255, v256
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v258 = iconst.i32 37
	movz	x11, #0x25
	; v259 = select v257, v258, v252
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v260 = iconst.i64 -6502179613573316608
	movz	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v261 = trunc.i16 v260
	; v262 = clz v261
	uxth	w16, w10
	clz	w10, w16
	sub	w10, w10, #16
	; v263 = iconst.i16 16
	; v264 = icmp.ne v262, v263
	uxth	w16, w10
	cmp	w16, #16
	cset	w10, ne
	; v265 = iconst.i32 38
	movz	x11, #0x26
	; v266 = select v264, v265, v259
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v267 = iconst.i64 -6502179613573316608
	movz	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v268 = trunc.i16 v267
	; v269 = ctz v268
	orr	w16, w10, #65536
	rbit	w16, w16
	clz	w10, w16
	; v270 = iconst.i16 16
	; v271 = icmp.ne v269, v270
	uxth	w16, w10
	cmp	w16, #16
	cset	w10, ne
	; v272 = iconst.i32 39
	movz	x11, #0x27
	; v273 = select v271, v272, v266
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v274 = iconst.i64 -6502179613573316608
	movz	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v275 = trunc.i16 v274
	; v276 = popcnt v275
	uxth	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v277 = iconst.i16 0
	; v278 = icmp.ne v276, v277
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v279 = iconst.i32 40
	movz	x11, #0x28
	; v280 = select v278, v279, v273
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v281 = iconst.i64 -6502179613573316608
	movz	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v282 = trunc.i16 v281
	; v283 = bswap v282
	rev16	w10, w10
	; v284 = iconst.i16 0
	; v285 = icmp.ne v283, v284
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v286 = iconst.i32 41
	movz	x11, #0x29
	; v287 = select v285, v286, v280
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v288 = iconst.i64 -6502179613573316607
	movz	x10, #0x1
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v289 = trunc.i16 v288
	; v290 = ineg v289
	neg	w10, w10
	; v291 = iconst.i16 -1
	movn	x11, #0x0
	; v292 = icmp.ne v290, v291
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v293 = iconst.i32 42
	movz	x11, #0x2a
	; v294 = select v292, v293, v287
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v295 = iconst.i64 -6502179613573316607
	movz	x10, #0x1
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v296 = trunc.i16 v295
	; v297 = bnot v296
	mvn	w10, w10
	; v298 = iconst.i16 -2
	movn	x11, #0x1
	; v299 = icmp.ne v297, v298
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v300 = iconst.i32 43
	movz	x11, #0x2b
	; v301 = select v299, v300, v294
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v302 = iconst.i64 -6502179613573316607
	movz	x10, #0x1
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v303 = trunc.i16 v302
	; v304 = clz v303
	uxth	w16, w10
	clz	w10, w16
	sub	w10, w10, #16
	; v305 = iconst.i16 15
	; v306 = icmp.ne v304, v305
	uxth	w16, w10
	cmp	w16, #15
	cset	w10, ne
	; v307 = iconst.i32 44
	movz	x11, #0x2c
	; v308 = select v306, v307, v301
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v309 = iconst.i64 -6502179613573316607
	movz	x10, #0x1
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v310 = trunc.i16 v309
	; v311 = ctz v310
	orr	w16, w10, #65536
	rbit	w16, w16
	clz	w10, w16
	; v312 = iconst.i16 0
	; v313 = icmp.ne v311, v312
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v314 = iconst.i32 45
	movz	x11, #0x2d
	; v315 = select v313, v314, v308
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v316 = iconst.i64 -6502179613573316607
	movz	x10, #0x1
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v317 = trunc.i16 v316
	; v318 = popcnt v317
	uxth	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v319 = iconst.i16 1
	; v320 = icmp.ne v318, v319
	uxth	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v321 = iconst.i32 46
	movz	x11, #0x2e
	; v322 = select v320, v321, v315
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v323 = iconst.i64 -6502179613573316607
	movz	x10, #0x1
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v324 = trunc.i16 v323
	; v325 = bswap v324
	rev16	w10, w10
	; v326 = iconst.i16 256
	; v327 = icmp.ne v325, v326
	uxth	w16, w10
	cmp	w16, #256
	cset	w10, ne
	; v328 = iconst.i32 47
	movz	x11, #0x2f
	; v329 = select v327, v328, v322
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v330 = iconst.i64 -6502179613573251073
	movn	x10, #0x94d2, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v331 = trunc.i16 v330
	; v332 = ineg v331
	neg	w10, w10
	; v333 = iconst.i16 1
	; v334 = icmp.ne v332, v333
	uxth	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v335 = iconst.i32 48
	movz	x11, #0x30
	; v336 = select v334, v335, v329
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v337 = iconst.i64 -6502179613573251073
	movn	x10, #0x94d2, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v338 = trunc.i16 v337
	; v339 = bnot v338
	mvn	w10, w10
	; v340 = iconst.i16 0
	; v341 = icmp.ne v339, v340
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v342 = iconst.i32 49
	movz	x11, #0x31
	; v343 = select v341, v342, v336
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v344 = iconst.i64 -6502179613573251073
	movn	x10, #0x94d2, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v345 = trunc.i16 v344
	; v346 = clz v345
	uxth	w16, w10
	clz	w10, w16
	sub	w10, w10, #16
	; v347 = iconst.i16 0
	; v348 = icmp.ne v346, v347
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v349 = iconst.i32 50
	movz	x11, #0x32
	; v350 = select v348, v349, v343
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v351 = iconst.i64 -6502179613573251073
	movn	x10, #0x94d2, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v352 = trunc.i16 v351
	; v353 = ctz v352
	orr	w16, w10, #65536
	rbit	w16, w16
	clz	w10, w16
	; v354 = iconst.i16 0
	; v355 = icmp.ne v353, v354
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v356 = iconst.i32 51
	movz	x11, #0x33
	; v357 = select v355, v356, v350
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v358 = iconst.i64 -6502179613573251073
	movn	x10, #0x94d2, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v359 = trunc.i16 v358
	; v360 = popcnt v359
	uxth	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v361 = iconst.i16 16
	; v362 = icmp.ne v360, v361
	uxth	w16, w10
	cmp	w16, #16
	cset	w10, ne
	; v363 = iconst.i32 52
	movz	x11, #0x34
	; v364 = select v362, v363, v357
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v365 = iconst.i64 -6502179613573251073
	movn	x10, #0x94d2, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v366 = trunc.i16 v365
	; v367 = bswap v366
	rev16	w10, w10
	; v368 = iconst.i16 -1
	movn	x11, #0x0
	; v369 = icmp.ne v367, v368
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v370 = iconst.i32 53
	movz	x11, #0x35
	; v371 = select v369, v370, v364
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v372 = iconst.i64 -6502179613573283840
	movz	x10, #0x8000
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v373 = trunc.i16 v372
	; v374 = ineg v373
	neg	w10, w10
	; v375 = iconst.i16 -32768
	movn	x11, #0x7fff
	; v376 = icmp.ne v374, v375
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v377 = iconst.i32 54
	movz	x11, #0x36
	; v378 = select v376, v377, v371
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v379 = iconst.i64 -6502179613573283840
	movz	x10, #0x8000
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v380 = trunc.i16 v379
	; v381 = bnot v380
	mvn	w10, w10
	; v382 = iconst.i16 32767
	movz	x11, #0x7fff
	; v383 = icmp.ne v381, v382
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v384 = iconst.i32 55
	movz	x11, #0x37
	; v385 = select v383, v384, v378
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v386 = iconst.i64 -6502179613573283840
	movz	x10, #0x8000
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v387 = trunc.i16 v386
	; v388 = clz v387
	uxth	w16, w10
	clz	w10, w16
	sub	w10, w10, #16
	; v389 = iconst.i16 0
	; v390 = icmp.ne v388, v389
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v391 = iconst.i32 56
	movz	x11, #0x38
	; v392 = select v390, v391, v385
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v393 = iconst.i64 -6502179613573283840
	movz	x10, #0x8000
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v394 = trunc.i16 v393
	; v395 = ctz v394
	orr	w16, w10, #65536
	rbit	w16, w16
	clz	w10, w16
	; v396 = iconst.i16 15
	; v397 = icmp.ne v395, v396
	uxth	w16, w10
	cmp	w16, #15
	cset	w10, ne
	; v398 = iconst.i32 57
	movz	x11, #0x39
	; v399 = select v397, v398, v392
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v400 = iconst.i64 -6502179613573283840
	movz	x10, #0x8000
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v401 = trunc.i16 v400
	; v402 = popcnt v401
	uxth	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v403 = iconst.i16 1
	; v404 = icmp.ne v402, v403
	uxth	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v405 = iconst.i32 58
	movz	x11, #0x3a
	; v406 = select v404, v405, v399
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v407 = iconst.i64 -6502179613573283840
	movz	x10, #0x8000
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v408 = trunc.i16 v407
	; v409 = bswap v408
	rev16	w10, w10
	; v410 = iconst.i16 128
	; v411 = icmp.ne v409, v410
	uxth	w16, w10
	cmp	w16, #128
	cset	w10, ne
	; v412 = iconst.i32 59
	movz	x11, #0x3b
	; v413 = select v411, v412, v406
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v414 = iconst.i64 -6502179613573316518
	movz	x10, #0x5a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v415 = trunc.i16 v414
	; v416 = ineg v415
	neg	w10, w10
	; v417 = iconst.i16 -90
	movn	x11, #0x59
	; v418 = icmp.ne v416, v417
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v419 = iconst.i32 60
	movz	x11, #0x3c
	; v420 = select v418, v419, v413
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v421 = iconst.i64 -6502179613573316518
	movz	x10, #0x5a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v422 = trunc.i16 v421
	; v423 = bnot v422
	mvn	w10, w10
	; v424 = iconst.i16 -91
	movn	x11, #0x5a
	; v425 = icmp.ne v423, v424
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v426 = iconst.i32 61
	movz	x11, #0x3d
	; v427 = select v425, v426, v420
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v428 = iconst.i64 -6502179613573316518
	movz	x10, #0x5a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v429 = trunc.i16 v428
	; v430 = clz v429
	uxth	w16, w10
	clz	w10, w16
	sub	w10, w10, #16
	; v431 = iconst.i16 9
	; v432 = icmp.ne v430, v431
	uxth	w16, w10
	cmp	w16, #9
	cset	w10, ne
	; v433 = iconst.i32 62
	movz	x11, #0x3e
	; v434 = select v432, v433, v427
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v435 = iconst.i64 -6502179613573316518
	movz	x10, #0x5a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v436 = trunc.i16 v435
	; v437 = ctz v436
	orr	w16, w10, #65536
	rbit	w16, w16
	clz	w10, w16
	; v438 = iconst.i16 1
	; v439 = icmp.ne v437, v438
	uxth	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v440 = iconst.i32 63
	movz	x11, #0x3f
	; v441 = select v439, v440, v434
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v442 = iconst.i64 -6502179613573316518
	movz	x10, #0x5a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v443 = trunc.i16 v442
	; v444 = popcnt v443
	uxth	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v445 = iconst.i16 4
	; v446 = icmp.ne v444, v445
	uxth	w16, w10
	cmp	w16, #4
	cset	w10, ne
	; v447 = iconst.i32 64
	movz	x11, #0x40
	; v448 = select v446, v447, v441
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v449 = iconst.i64 -6502179613573316518
	movz	x10, #0x5a
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v450 = trunc.i16 v449
	; v451 = bswap v450
	rev16	w10, w10
	; v452 = iconst.i16 23040
	movz	x11, #0x5a00
	; v453 = icmp.ne v451, v452
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v454 = iconst.i32 65
	movz	x11, #0x41
	; v455 = select v453, v454, v448
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v456 = iconst.i64 -6502179613573263889
	movz	x10, #0xcdef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v457 = trunc.i16 v456
	; v458 = ineg v457
	neg	w10, w10
	; v459 = iconst.i16 12817
	movz	x11, #0x3211
	; v460 = icmp.ne v458, v459
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v461 = iconst.i32 66
	movz	x11, #0x42
	; v462 = select v460, v461, v455
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v463 = iconst.i64 -6502179613573263889
	movz	x10, #0xcdef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v464 = trunc.i16 v463
	; v465 = bnot v464
	mvn	w10, w10
	; v466 = iconst.i16 12816
	movz	x11, #0x3210
	; v467 = icmp.ne v465, v466
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v468 = iconst.i32 67
	movz	x11, #0x43
	; v469 = select v467, v468, v462
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v470 = iconst.i64 -6502179613573263889
	movz	x10, #0xcdef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v471 = trunc.i16 v470
	; v472 = clz v471
	uxth	w16, w10
	clz	w10, w16
	sub	w10, w10, #16
	; v473 = iconst.i16 0
	; v474 = icmp.ne v472, v473
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v475 = iconst.i32 68
	movz	x11, #0x44
	; v476 = select v474, v475, v469
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v477 = iconst.i64 -6502179613573263889
	movz	x10, #0xcdef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v478 = trunc.i16 v477
	; v479 = ctz v478
	orr	w16, w10, #65536
	rbit	w16, w16
	clz	w10, w16
	; v480 = iconst.i16 0
	; v481 = icmp.ne v479, v480
	uxth	w16, w10
	cmp	w16, #0
	cset	w10, ne
	; v482 = iconst.i32 69
	movz	x11, #0x45
	; v483 = select v481, v482, v476
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v484 = iconst.i64 -6502179613573263889
	movz	x10, #0xcdef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v485 = trunc.i16 v484
	; v486 = popcnt v485
	uxth	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v487 = iconst.i16 12
	; v488 = icmp.ne v486, v487
	uxth	w16, w10
	cmp	w16, #12
	cset	w10, ne
	; v489 = iconst.i32 70
	movz	x11, #0x46
	; v490 = select v488, v489, v483
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v491 = iconst.i64 -6502179613573263889
	movz	x10, #0xcdef
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v492 = trunc.i16 v491
	; v493 = bswap v492
	rev16	w10, w10
	; v494 = iconst.i16 -4147
	movn	x11, #0x1032
	; v495 = icmp.ne v493, v494
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v496 = iconst.i32 71
	movz	x11, #0x47
	; v497 = select v495, v496, v490
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v498 = iconst.i64 -6502179613573316352
	movz	x10, #0x100
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v499 = trunc.i16 v498
	; v500 = ineg v499
	neg	w10, w10
	; v501 = iconst.i16 -256
	movn	x11, #0xff
	; v502 = icmp.ne v500, v501
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v503 = iconst.i32 72
	movz	x11, #0x48
	; v504 = select v502, v503, v497
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v505 = iconst.i64 -6502179613573316352
	movz	x10, #0x100
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v506 = trunc.i16 v505
	; v507 = bnot v506
	mvn	w10, w10
	; v508 = iconst.i16 -257
	movn	x11, #0x100
	; v509 = icmp.ne v507, v508
	uxth	w16, w10
	uxth	w17, w11
	cmp	w16, w17
	cset	w10, ne
	; v510 = iconst.i32 73
	movz	x11, #0x49
	; v511 = select v509, v510, v504
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v512 = iconst.i64 -6502179613573316352
	movz	x10, #0x100
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v513 = trunc.i16 v512
	; v514 = clz v513
	uxth	w16, w10
	clz	w10, w16
	sub	w10, w10, #16
	; v515 = iconst.i16 7
	; v516 = icmp.ne v514, v515
	uxth	w16, w10
	cmp	w16, #7
	cset	w10, ne
	; v517 = iconst.i32 74
	movz	x11, #0x4a
	; v518 = select v516, v517, v511
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v519 = iconst.i64 -6502179613573316352
	movz	x10, #0x100
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v520 = trunc.i16 v519
	; v521 = ctz v520
	orr	w16, w10, #65536
	rbit	w16, w16
	clz	w10, w16
	; v522 = iconst.i16 8
	; v523 = icmp.ne v521, v522
	uxth	w16, w10
	cmp	w16, #8
	cset	w10, ne
	; v524 = iconst.i32 75
	movz	x11, #0x4b
	; v525 = select v523, v524, v518
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v526 = iconst.i64 -6502179613573316352
	movz	x10, #0x100
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v527 = trunc.i16 v526
	; v528 = popcnt v527
	uxth	w16, w10
	fmov	s0, w16
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v529 = iconst.i16 1
	; v530 = icmp.ne v528, v529
	uxth	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v531 = iconst.i32 76
	movz	x11, #0x4c
	; v532 = select v530, v531, v525
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v533 = iconst.i64 -6502179613573316352
	movz	x10, #0x100
	movk	x10, #0x6b2d, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v534 = trunc.i16 v533
	; v535 = bswap v534
	rev16	w10, w10
	; v536 = iconst.i16 1
	; v537 = icmp.ne v535, v536
	uxth	w16, w10
	cmp	w16, #1
	cset	w10, ne
	; v538 = iconst.i32 77
	movz	x11, #0x4d
	; v539 = select v537, v538, v532
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v540 = iconst.i64 -6502179615371427840
	movz	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v541 = trunc.i32 v540
	; v542 = ineg v541
	neg	w10, w10
	; v543 = iconst.i32 0
	; v544 = icmp.ne v542, v543
	cmp	w10, #0
	cset	w10, ne
	; v545 = iconst.i32 78
	movz	x11, #0x4e
	; v546 = select v544, v545, v539
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v547 = iconst.i64 -6502179615371427840
	movz	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v548 = trunc.i32 v547
	; v549 = bnot v548
	mvn	w10, w10
	; v550 = iconst.i32 -1
	; v551 = icmp.ne v549, v550
	cmn	w10, #1
	cset	w10, ne
	; v552 = iconst.i32 79
	movz	x11, #0x4f
	; v553 = select v551, v552, v546
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v554 = iconst.i64 -6502179615371427840
	movz	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v555 = trunc.i32 v554
	; v556 = clz v555
	clz	w10, w10
	; v557 = iconst.i32 32
	; v558 = icmp.ne v556, v557
	cmp	w10, #32
	cset	w10, ne
	; v559 = iconst.i32 80
	movz	x11, #0x50
	; v560 = select v558, v559, v553
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v561 = iconst.i64 -6502179615371427840
	movz	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v562 = trunc.i32 v561
	; v563 = ctz v562
	rbit	w16, w10
	clz	w10, w16
	; v564 = iconst.i32 32
	; v565 = icmp.ne v563, v564
	cmp	w10, #32
	cset	w10, ne
	; v566 = iconst.i32 81
	movz	x11, #0x51
	; v567 = select v565, v566, v560
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v568 = iconst.i64 -6502179615371427840
	movz	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v569 = trunc.i32 v568
	; v570 = popcnt v569
	fmov	s0, w10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v571 = iconst.i32 0
	; v572 = icmp.ne v570, v571
	cmp	w10, #0
	cset	w10, ne
	; v573 = iconst.i32 82
	movz	x11, #0x52
	; v574 = select v572, v573, v567
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v575 = iconst.i64 -6502179615371427840
	movz	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v576 = trunc.i32 v575
	; v577 = bswap v576
	rev	w10, w10
	; v578 = iconst.i32 0
	; v579 = icmp.ne v577, v578
	cmp	w10, #0
	cset	w10, ne
	; v580 = iconst.i32 83
	movz	x11, #0x53
	; v581 = select v579, v580, v574
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v582 = iconst.i64 -6502179615371427839
	movz	x10, #0x1
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v583 = trunc.i32 v582
	; v584 = ineg v583
	neg	w10, w10
	; v585 = iconst.i32 -1
	; v586 = icmp.ne v584, v585
	cmn	w10, #1
	cset	w10, ne
	; v587 = iconst.i32 84
	movz	x11, #0x54
	; v588 = select v586, v587, v581
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v589 = iconst.i64 -6502179615371427839
	movz	x10, #0x1
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v590 = trunc.i32 v589
	; v591 = bnot v590
	mvn	w10, w10
	; v592 = iconst.i32 -2
	; v593 = icmp.ne v591, v592
	cmn	w10, #2
	cset	w10, ne
	; v594 = iconst.i32 85
	movz	x11, #0x55
	; v595 = select v593, v594, v588
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v596 = iconst.i64 -6502179615371427839
	movz	x10, #0x1
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v597 = trunc.i32 v596
	; v598 = clz v597
	clz	w10, w10
	; v599 = iconst.i32 31
	; v600 = icmp.ne v598, v599
	cmp	w10, #31
	cset	w10, ne
	; v601 = iconst.i32 86
	movz	x11, #0x56
	; v602 = select v600, v601, v595
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v603 = iconst.i64 -6502179615371427839
	movz	x10, #0x1
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v604 = trunc.i32 v603
	; v605 = ctz v604
	rbit	w16, w10
	clz	w10, w16
	; v606 = iconst.i32 0
	; v607 = icmp.ne v605, v606
	cmp	w10, #0
	cset	w10, ne
	; v608 = iconst.i32 87
	movz	x11, #0x57
	; v609 = select v607, v608, v602
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v610 = iconst.i64 -6502179615371427839
	movz	x10, #0x1
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v611 = trunc.i32 v610
	; v612 = popcnt v611
	fmov	s0, w10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v613 = iconst.i32 1
	; v614 = icmp.ne v612, v613
	cmp	w10, #1
	cset	w10, ne
	; v615 = iconst.i32 88
	movz	x11, #0x58
	; v616 = select v614, v615, v609
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v617 = iconst.i64 -6502179615371427839
	movz	x10, #0x1
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v618 = trunc.i32 v617
	; v619 = bswap v618
	rev	w10, w10
	; v620 = iconst.i32 16777216
	movz	x11, #0x100, lsl #16
	; v621 = icmp.ne v619, v620
	cmp	w10, w11
	cset	w10, ne
	; v622 = iconst.i32 89
	movz	x11, #0x59
	; v623 = select v621, v622, v616
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v624 = iconst.i64 -6502179611076460545
	movn	x10, #0x61e8, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v625 = trunc.i32 v624
	; v626 = ineg v625
	neg	w10, w10
	; v627 = iconst.i32 1
	; v628 = icmp.ne v626, v627
	cmp	w10, #1
	cset	w10, ne
	; v629 = iconst.i32 90
	movz	x11, #0x5a
	; v630 = select v628, v629, v623
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v631 = iconst.i64 -6502179611076460545
	movn	x10, #0x61e8, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v632 = trunc.i32 v631
	; v633 = bnot v632
	mvn	w10, w10
	; v634 = iconst.i32 0
	; v635 = icmp.ne v633, v634
	cmp	w10, #0
	cset	w10, ne
	; v636 = iconst.i32 91
	movz	x11, #0x5b
	; v637 = select v635, v636, v630
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v638 = iconst.i64 -6502179611076460545
	movn	x10, #0x61e8, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v639 = trunc.i32 v638
	; v640 = clz v639
	clz	w10, w10
	; v641 = iconst.i32 0
	; v642 = icmp.ne v640, v641
	cmp	w10, #0
	cset	w10, ne
	; v643 = iconst.i32 92
	movz	x11, #0x5c
	; v644 = select v642, v643, v637
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v645 = iconst.i64 -6502179611076460545
	movn	x10, #0x61e8, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v646 = trunc.i32 v645
	; v647 = ctz v646
	rbit	w16, w10
	clz	w10, w16
	; v648 = iconst.i32 0
	; v649 = icmp.ne v647, v648
	cmp	w10, #0
	cset	w10, ne
	; v650 = iconst.i32 93
	movz	x11, #0x5d
	; v651 = select v649, v650, v644
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v652 = iconst.i64 -6502179611076460545
	movn	x10, #0x61e8, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v653 = trunc.i32 v652
	; v654 = popcnt v653
	fmov	s0, w10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v655 = iconst.i32 32
	; v656 = icmp.ne v654, v655
	cmp	w10, #32
	cset	w10, ne
	; v657 = iconst.i32 94
	movz	x11, #0x5e
	; v658 = select v656, v657, v651
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v659 = iconst.i64 -6502179611076460545
	movn	x10, #0x61e8, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v660 = trunc.i32 v659
	; v661 = bswap v660
	rev	w10, w10
	; v662 = iconst.i32 -1
	; v663 = icmp.ne v661, v662
	cmn	w10, #1
	cset	w10, ne
	; v664 = iconst.i32 95
	movz	x11, #0x5f
	; v665 = select v663, v664, v658
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v666 = iconst.i64 -6502179613223944192
	movz	x10, #0x8000, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v667 = trunc.i32 v666
	; v668 = ineg v667
	neg	w10, w10
	; v669 = iconst.i32 -2147483648
	movn	x11, #0xffff
	movk	x11, #0x8000, lsl #16
	; v670 = icmp.ne v668, v669
	cmp	w10, w11
	cset	w10, ne
	; v671 = iconst.i32 96
	movz	x11, #0x60
	; v672 = select v670, v671, v665
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v673 = iconst.i64 -6502179613223944192
	movz	x10, #0x8000, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v674 = trunc.i32 v673
	; v675 = bnot v674
	mvn	w10, w10
	; v676 = iconst.i32 2147483647
	movz	x11, #0xffff
	movk	x11, #0x7fff, lsl #16
	; v677 = icmp.ne v675, v676
	cmp	w10, w11
	cset	w10, ne
	; v678 = iconst.i32 97
	movz	x11, #0x61
	; v679 = select v677, v678, v672
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v680 = iconst.i64 -6502179613223944192
	movz	x10, #0x8000, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v681 = trunc.i32 v680
	; v682 = clz v681
	clz	w10, w10
	; v683 = iconst.i32 0
	; v684 = icmp.ne v682, v683
	cmp	w10, #0
	cset	w10, ne
	; v685 = iconst.i32 98
	movz	x11, #0x62
	; v686 = select v684, v685, v679
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v687 = iconst.i64 -6502179613223944192
	movz	x10, #0x8000, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v688 = trunc.i32 v687
	; v689 = ctz v688
	rbit	w16, w10
	clz	w10, w16
	; v690 = iconst.i32 31
	; v691 = icmp.ne v689, v690
	cmp	w10, #31
	cset	w10, ne
	; v692 = iconst.i32 99
	movz	x11, #0x63
	; v693 = select v691, v692, v686
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v694 = iconst.i64 -6502179613223944192
	movz	x10, #0x8000, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v695 = trunc.i32 v694
	; v696 = popcnt v695
	fmov	s0, w10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v697 = iconst.i32 1
	; v698 = icmp.ne v696, v697
	cmp	w10, #1
	cset	w10, ne
	; v699 = iconst.i32 100
	movz	x11, #0x64
	; v700 = select v698, v699, v693
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v701 = iconst.i64 -6502179613223944192
	movz	x10, #0x8000, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v702 = trunc.i32 v701
	; v703 = bswap v702
	rev	w10, w10
	; v704 = iconst.i32 128
	; v705 = icmp.ne v703, v704
	cmp	w10, #128
	cset	w10, ne
	; v706 = iconst.i32 101
	movz	x11, #0x65
	; v707 = select v705, v706, v700
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v708 = iconst.i64 -6502179615371427750
	movz	x10, #0x5a
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v709 = trunc.i32 v708
	; v710 = ineg v709
	neg	w10, w10
	; v711 = iconst.i32 -90
	; v712 = icmp.ne v710, v711
	cmn	w10, #90
	cset	w10, ne
	; v713 = iconst.i32 102
	movz	x11, #0x66
	; v714 = select v712, v713, v707
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v715 = iconst.i64 -6502179615371427750
	movz	x10, #0x5a
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v716 = trunc.i32 v715
	; v717 = bnot v716
	mvn	w10, w10
	; v718 = iconst.i32 -91
	; v719 = icmp.ne v717, v718
	cmn	w10, #91
	cset	w10, ne
	; v720 = iconst.i32 103
	movz	x11, #0x67
	; v721 = select v719, v720, v714
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v722 = iconst.i64 -6502179615371427750
	movz	x10, #0x5a
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v723 = trunc.i32 v722
	; v724 = clz v723
	clz	w10, w10
	; v725 = iconst.i32 25
	; v726 = icmp.ne v724, v725
	cmp	w10, #25
	cset	w10, ne
	; v727 = iconst.i32 104
	movz	x11, #0x68
	; v728 = select v726, v727, v721
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v729 = iconst.i64 -6502179615371427750
	movz	x10, #0x5a
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v730 = trunc.i32 v729
	; v731 = ctz v730
	rbit	w16, w10
	clz	w10, w16
	; v732 = iconst.i32 1
	; v733 = icmp.ne v731, v732
	cmp	w10, #1
	cset	w10, ne
	; v734 = iconst.i32 105
	movz	x11, #0x69
	; v735 = select v733, v734, v728
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v736 = iconst.i64 -6502179615371427750
	movz	x10, #0x5a
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v737 = trunc.i32 v736
	; v738 = popcnt v737
	fmov	s0, w10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v739 = iconst.i32 4
	; v740 = icmp.ne v738, v739
	cmp	w10, #4
	cset	w10, ne
	; v741 = iconst.i32 106
	movz	x11, #0x6a
	; v742 = select v740, v741, v735
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v743 = iconst.i64 -6502179615371427750
	movz	x10, #0x5a
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v744 = trunc.i32 v743
	; v745 = bswap v744
	rev	w10, w10
	; v746 = iconst.i32 1509949440
	movz	x11, #0x5a00, lsl #16
	; v747 = icmp.ne v745, v746
	cmp	w10, w11
	cset	w10, ne
	; v748 = iconst.i32 107
	movz	x11, #0x6b
	; v749 = select v747, v748, v742
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v750 = iconst.i64 -6502179613061689873
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v751 = trunc.i32 v750
	; v752 = ineg v751
	neg	w10, w10
	; v753 = iconst.i32 1985229329
	movz	x11, #0x3211
	movk	x11, #0x7654, lsl #16
	; v754 = icmp.ne v752, v753
	cmp	w10, w11
	cset	w10, ne
	; v755 = iconst.i32 108
	movz	x11, #0x6c
	; v756 = select v754, v755, v749
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v757 = iconst.i64 -6502179613061689873
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v758 = trunc.i32 v757
	; v759 = bnot v758
	mvn	w10, w10
	; v760 = iconst.i32 1985229328
	movz	x11, #0x3210
	movk	x11, #0x7654, lsl #16
	; v761 = icmp.ne v759, v760
	cmp	w10, w11
	cset	w10, ne
	; v762 = iconst.i32 109
	movz	x11, #0x6d
	; v763 = select v761, v762, v756
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v764 = iconst.i64 -6502179613061689873
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v765 = trunc.i32 v764
	; v766 = clz v765
	clz	w10, w10
	; v767 = iconst.i32 0
	; v768 = icmp.ne v766, v767
	cmp	w10, #0
	cset	w10, ne
	; v769 = iconst.i32 110
	movz	x11, #0x6e
	; v770 = select v768, v769, v763
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v771 = iconst.i64 -6502179613061689873
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v772 = trunc.i32 v771
	; v773 = ctz v772
	rbit	w16, w10
	clz	w10, w16
	; v774 = iconst.i32 0
	; v775 = icmp.ne v773, v774
	cmp	w10, #0
	cset	w10, ne
	; v776 = iconst.i32 111
	movz	x11, #0x6f
	; v777 = select v775, v776, v770
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v778 = iconst.i64 -6502179613061689873
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v779 = trunc.i32 v778
	; v780 = popcnt v779
	fmov	s0, w10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v781 = iconst.i32 20
	; v782 = icmp.ne v780, v781
	cmp	w10, #20
	cset	w10, ne
	; v783 = iconst.i32 112
	movz	x11, #0x70
	; v784 = select v782, v783, v777
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v785 = iconst.i64 -6502179613061689873
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v786 = trunc.i32 v785
	; v787 = bswap v786
	rev	w10, w10
	; v788 = iconst.i32 -271733879
	movn	x11, #0x5476
	movk	x11, #0xefcd, lsl #16
	; v789 = icmp.ne v787, v788
	cmp	w10, w11
	cset	w10, ne
	; v790 = iconst.i32 113
	movz	x11, #0x71
	; v791 = select v789, v790, v784
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v792 = iconst.i64 -6502179615371362304
	movz	x10, #0x1, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v793 = trunc.i32 v792
	; v794 = ineg v793
	neg	w10, w10
	; v795 = iconst.i32 -65536
	; v796 = icmp.ne v794, v795
	cmn	w10, #16, lsl #12
	cset	w10, ne
	; v797 = iconst.i32 114
	movz	x11, #0x72
	; v798 = select v796, v797, v791
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v799 = iconst.i64 -6502179615371362304
	movz	x10, #0x1, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v800 = trunc.i32 v799
	; v801 = bnot v800
	mvn	w10, w10
	; v802 = iconst.i32 -65537
	movn	x11, #0x1, lsl #16
	; v803 = icmp.ne v801, v802
	cmp	w10, w11
	cset	w10, ne
	; v804 = iconst.i32 115
	movz	x11, #0x73
	; v805 = select v803, v804, v798
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v806 = iconst.i64 -6502179615371362304
	movz	x10, #0x1, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v807 = trunc.i32 v806
	; v808 = clz v807
	clz	w10, w10
	; v809 = iconst.i32 15
	; v810 = icmp.ne v808, v809
	cmp	w10, #15
	cset	w10, ne
	; v811 = iconst.i32 116
	movz	x11, #0x74
	; v812 = select v810, v811, v805
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v813 = iconst.i64 -6502179615371362304
	movz	x10, #0x1, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v814 = trunc.i32 v813
	; v815 = ctz v814
	rbit	w16, w10
	clz	w10, w16
	; v816 = iconst.i32 16
	; v817 = icmp.ne v815, v816
	cmp	w10, #16
	cset	w10, ne
	; v818 = iconst.i32 117
	movz	x11, #0x75
	; v819 = select v817, v818, v812
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v820 = iconst.i64 -6502179615371362304
	movz	x10, #0x1, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v821 = trunc.i32 v820
	; v822 = popcnt v821
	fmov	s0, w10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v823 = iconst.i32 1
	; v824 = icmp.ne v822, v823
	cmp	w10, #1
	cset	w10, ne
	; v825 = iconst.i32 118
	movz	x11, #0x76
	; v826 = select v824, v825, v819
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v827 = iconst.i64 -6502179615371362304
	movz	x10, #0x1, lsl #16
	movk	x10, #0x9e17, lsl #32
	movk	x10, #0xa5c3, lsl #48
	; v828 = trunc.i32 v827
	; v829 = bswap v828
	rev	w10, w10
	; v830 = iconst.i32 256
	; v831 = icmp.ne v829, v830
	cmp	w10, #256
	cset	w10, ne
	; v832 = iconst.i32 119
	movz	x11, #0x77
	; v833 = select v831, v832, v826
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v834 = iconst.i64 0
	movz	x10, #0x0
	; v835 = ineg v834
	neg	x10, x10
	; v836 = iconst.i64 0
	; v837 = icmp.ne v835, v836
	cmp	x10, #0
	cset	w10, ne
	; v838 = iconst.i32 120
	movz	x11, #0x78
	; v839 = select v837, v838, v833
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v840 = iconst.i64 0
	movz	x10, #0x0
	; v841 = bnot v840
	mvn	x10, x10
	; v842 = iconst.i64 -1
	; v843 = icmp.ne v841, v842
	cmn	x10, #1
	cset	w10, ne
	; v844 = iconst.i32 121
	movz	x11, #0x79
	; v845 = select v843, v844, v839
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v846 = iconst.i64 0
	movz	x10, #0x0
	; v847 = clz v846
	clz	x10, x10
	; v848 = iconst.i64 64
	; v849 = icmp.ne v847, v848
	cmp	x10, #64
	cset	w10, ne
	; v850 = iconst.i32 122
	movz	x11, #0x7a
	; v851 = select v849, v850, v845
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v852 = iconst.i64 0
	movz	x10, #0x0
	; v853 = ctz v852
	rbit	x16, x10
	clz	x10, x16
	; v854 = iconst.i64 64
	; v855 = icmp.ne v853, v854
	cmp	x10, #64
	cset	w10, ne
	; v856 = iconst.i32 123
	movz	x11, #0x7b
	; v857 = select v855, v856, v851
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v858 = iconst.i64 0
	movz	x10, #0x0
	; v859 = popcnt v858
	fmov	d0, x10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v860 = iconst.i64 0
	; v861 = icmp.ne v859, v860
	cmp	x10, #0
	cset	w10, ne
	; v862 = iconst.i32 124
	movz	x11, #0x7c
	; v863 = select v861, v862, v857
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v864 = iconst.i64 0
	movz	x10, #0x0
	; v865 = bswap v864
	rev	x10, x10
	; v866 = iconst.i64 0
	; v867 = icmp.ne v865, v866
	cmp	x10, #0
	cset	w10, ne
	; v868 = iconst.i32 125
	movz	x11, #0x7d
	; v869 = select v867, v868, v863
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v870 = iconst.i64 1
	movz	x10, #0x1
	; v871 = ineg v870
	neg	x10, x10
	; v872 = iconst.i64 -1
	; v873 = icmp.ne v871, v872
	cmn	x10, #1
	cset	w10, ne
	; v874 = iconst.i32 126
	movz	x11, #0x7e
	; v875 = select v873, v874, v869
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v876 = iconst.i64 1
	movz	x10, #0x1
	; v877 = bnot v876
	mvn	x10, x10
	; v878 = iconst.i64 -2
	; v879 = icmp.ne v877, v878
	cmn	x10, #2
	cset	w10, ne
	; v880 = iconst.i32 127
	movz	x11, #0x7f
	; v881 = select v879, v880, v875
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v882 = iconst.i64 1
	movz	x10, #0x1
	; v883 = clz v882
	clz	x10, x10
	; v884 = iconst.i64 63
	; v885 = icmp.ne v883, v884
	cmp	x10, #63
	cset	w10, ne
	; v886 = iconst.i32 128
	movz	x11, #0x80
	; v887 = select v885, v886, v881
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v888 = iconst.i64 1
	movz	x10, #0x1
	; v889 = ctz v888
	rbit	x16, x10
	clz	x10, x16
	; v890 = iconst.i64 0
	; v891 = icmp.ne v889, v890
	cmp	x10, #0
	cset	w10, ne
	; v892 = iconst.i32 129
	movz	x11, #0x81
	; v893 = select v891, v892, v887
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v894 = iconst.i64 1
	movz	x10, #0x1
	; v895 = popcnt v894
	fmov	d0, x10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v896 = iconst.i64 1
	; v897 = icmp.ne v895, v896
	cmp	x10, #1
	cset	w10, ne
	; v898 = iconst.i32 130
	movz	x11, #0x82
	; v899 = select v897, v898, v893
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v900 = iconst.i64 1
	movz	x10, #0x1
	; v901 = bswap v900
	rev	x10, x10
	; v902 = iconst.i64 72057594037927936
	movz	x11, #0x100, lsl #48
	; v903 = icmp.ne v901, v902
	cmp	x10, x11
	cset	w10, ne
	; v904 = iconst.i32 131
	movz	x11, #0x83
	; v905 = select v903, v904, v899
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v906 = iconst.i64 -1
	movn	x10, #0x0
	; v907 = ineg v906
	neg	x10, x10
	; v908 = iconst.i64 1
	; v909 = icmp.ne v907, v908
	cmp	x10, #1
	cset	w10, ne
	; v910 = iconst.i32 132
	movz	x11, #0x84
	; v911 = select v909, v910, v905
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v912 = iconst.i64 -1
	movn	x10, #0x0
	; v913 = bnot v912
	mvn	x10, x10
	; v914 = iconst.i64 0
	; v915 = icmp.ne v913, v914
	cmp	x10, #0
	cset	w10, ne
	; v916 = iconst.i32 133
	movz	x11, #0x85
	; v917 = select v915, v916, v911
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v918 = iconst.i64 -1
	movn	x10, #0x0
	; v919 = clz v918
	clz	x10, x10
	; v920 = iconst.i64 0
	; v921 = icmp.ne v919, v920
	cmp	x10, #0
	cset	w10, ne
	; v922 = iconst.i32 134
	movz	x11, #0x86
	; v923 = select v921, v922, v917
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v924 = iconst.i64 -1
	movn	x10, #0x0
	; v925 = ctz v924
	rbit	x16, x10
	clz	x10, x16
	; v926 = iconst.i64 0
	; v927 = icmp.ne v925, v926
	cmp	x10, #0
	cset	w10, ne
	; v928 = iconst.i32 135
	movz	x11, #0x87
	; v929 = select v927, v928, v923
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v930 = iconst.i64 -1
	movn	x10, #0x0
	; v931 = popcnt v930
	fmov	d0, x10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v932 = iconst.i64 64
	; v933 = icmp.ne v931, v932
	cmp	x10, #64
	cset	w10, ne
	; v934 = iconst.i32 136
	movz	x11, #0x88
	; v935 = select v933, v934, v929
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v936 = iconst.i64 -1
	movn	x10, #0x0
	; v937 = bswap v936
	rev	x10, x10
	; v938 = iconst.i64 -1
	; v939 = icmp.ne v937, v938
	cmn	x10, #1
	cset	w10, ne
	; v940 = iconst.i32 137
	movz	x11, #0x89
	; v941 = select v939, v940, v935
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v942 = iconst.i64 -9223372036854775808
	movz	x10, #0x8000, lsl #48
	; v943 = ineg v942
	neg	x10, x10
	; v944 = iconst.i64 -9223372036854775808
	movz	x11, #0x8000, lsl #48
	; v945 = icmp.ne v943, v944
	cmp	x10, x11
	cset	w10, ne
	; v946 = iconst.i32 138
	movz	x11, #0x8a
	; v947 = select v945, v946, v941
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v948 = iconst.i64 -9223372036854775808
	movz	x10, #0x8000, lsl #48
	; v949 = bnot v948
	mvn	x10, x10
	; v950 = iconst.i64 9223372036854775807
	movn	x11, #0x8000, lsl #48
	; v951 = icmp.ne v949, v950
	cmp	x10, x11
	cset	w10, ne
	; v952 = iconst.i32 139
	movz	x11, #0x8b
	; v953 = select v951, v952, v947
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v954 = iconst.i64 -9223372036854775808
	movz	x10, #0x8000, lsl #48
	; v955 = clz v954
	clz	x10, x10
	; v956 = iconst.i64 0
	; v957 = icmp.ne v955, v956
	cmp	x10, #0
	cset	w10, ne
	; v958 = iconst.i32 140
	movz	x11, #0x8c
	; v959 = select v957, v958, v953
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v960 = iconst.i64 -9223372036854775808
	movz	x10, #0x8000, lsl #48
	; v961 = ctz v960
	rbit	x16, x10
	clz	x10, x16
	; v962 = iconst.i64 63
	; v963 = icmp.ne v961, v962
	cmp	x10, #63
	cset	w10, ne
	; v964 = iconst.i32 141
	movz	x11, #0x8d
	; v965 = select v963, v964, v959
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v966 = iconst.i64 -9223372036854775808
	movz	x10, #0x8000, lsl #48
	; v967 = popcnt v966
	fmov	d0, x10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v968 = iconst.i64 1
	; v969 = icmp.ne v967, v968
	cmp	x10, #1
	cset	w10, ne
	; v970 = iconst.i32 142
	movz	x11, #0x8e
	; v971 = select v969, v970, v965
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v972 = iconst.i64 -9223372036854775808
	movz	x10, #0x8000, lsl #48
	; v973 = bswap v972
	rev	x10, x10
	; v974 = iconst.i64 128
	; v975 = icmp.ne v973, v974
	cmp	x10, #128
	cset	w10, ne
	; v976 = iconst.i32 143
	movz	x11, #0x8f
	; v977 = select v975, v976, v971
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v978 = iconst.i64 90
	movz	x10, #0x5a
	; v979 = ineg v978
	neg	x10, x10
	; v980 = iconst.i64 -90
	; v981 = icmp.ne v979, v980
	cmn	x10, #90
	cset	w10, ne
	; v982 = iconst.i32 144
	movz	x11, #0x90
	; v983 = select v981, v982, v977
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v984 = iconst.i64 90
	movz	x10, #0x5a
	; v985 = bnot v984
	mvn	x10, x10
	; v986 = iconst.i64 -91
	; v987 = icmp.ne v985, v986
	cmn	x10, #91
	cset	w10, ne
	; v988 = iconst.i32 145
	movz	x11, #0x91
	; v989 = select v987, v988, v983
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v990 = iconst.i64 90
	movz	x10, #0x5a
	; v991 = clz v990
	clz	x10, x10
	; v992 = iconst.i64 57
	; v993 = icmp.ne v991, v992
	cmp	x10, #57
	cset	w10, ne
	; v994 = iconst.i32 146
	movz	x11, #0x92
	; v995 = select v993, v994, v989
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v996 = iconst.i64 90
	movz	x10, #0x5a
	; v997 = ctz v996
	rbit	x16, x10
	clz	x10, x16
	; v998 = iconst.i64 1
	; v999 = icmp.ne v997, v998
	cmp	x10, #1
	cset	w10, ne
	; v1000 = iconst.i32 147
	movz	x11, #0x93
	; v1001 = select v999, v1000, v995
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1002 = iconst.i64 90
	movz	x10, #0x5a
	; v1003 = popcnt v1002
	fmov	d0, x10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v1004 = iconst.i64 4
	; v1005 = icmp.ne v1003, v1004
	cmp	x10, #4
	cset	w10, ne
	; v1006 = iconst.i32 148
	movz	x11, #0x94
	; v1007 = select v1005, v1006, v1001
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1008 = iconst.i64 90
	movz	x10, #0x5a
	; v1009 = bswap v1008
	rev	x10, x10
	; v1010 = iconst.i64 6485183463413514240
	movz	x11, #0x5a00, lsl #48
	; v1011 = icmp.ne v1009, v1010
	cmp	x10, x11
	cset	w10, ne
	; v1012 = iconst.i32 149
	movz	x11, #0x95
	; v1013 = select v1011, v1012, v1007
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1014 = iconst.i64 81985529216486895
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x4567, lsl #32
	movk	x10, #0x123, lsl #48
	; v1015 = ineg v1014
	neg	x10, x10
	; v1016 = iconst.i64 -81985529216486895
	movz	x11, #0x3211
	movk	x11, #0x7654, lsl #16
	movk	x11, #0xba98, lsl #32
	movk	x11, #0xfedc, lsl #48
	; v1017 = icmp.ne v1015, v1016
	cmp	x10, x11
	cset	w10, ne
	; v1018 = iconst.i32 150
	movz	x11, #0x96
	; v1019 = select v1017, v1018, v1013
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1020 = iconst.i64 81985529216486895
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x4567, lsl #32
	movk	x10, #0x123, lsl #48
	; v1021 = bnot v1020
	mvn	x10, x10
	; v1022 = iconst.i64 -81985529216486896
	movz	x11, #0x3210
	movk	x11, #0x7654, lsl #16
	movk	x11, #0xba98, lsl #32
	movk	x11, #0xfedc, lsl #48
	; v1023 = icmp.ne v1021, v1022
	cmp	x10, x11
	cset	w10, ne
	; v1024 = iconst.i32 151
	movz	x11, #0x97
	; v1025 = select v1023, v1024, v1019
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1026 = iconst.i64 81985529216486895
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x4567, lsl #32
	movk	x10, #0x123, lsl #48
	; v1027 = clz v1026
	clz	x10, x10
	; v1028 = iconst.i64 7
	; v1029 = icmp.ne v1027, v1028
	cmp	x10, #7
	cset	w10, ne
	; v1030 = iconst.i32 152
	movz	x11, #0x98
	; v1031 = select v1029, v1030, v1025
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1032 = iconst.i64 81985529216486895
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x4567, lsl #32
	movk	x10, #0x123, lsl #48
	; v1033 = ctz v1032
	rbit	x16, x10
	clz	x10, x16
	; v1034 = iconst.i64 0
	; v1035 = icmp.ne v1033, v1034
	cmp	x10, #0
	cset	w10, ne
	; v1036 = iconst.i32 153
	movz	x11, #0x99
	; v1037 = select v1035, v1036, v1031
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1038 = iconst.i64 81985529216486895
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x4567, lsl #32
	movk	x10, #0x123, lsl #48
	; v1039 = popcnt v1038
	fmov	d0, x10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v1040 = iconst.i64 32
	; v1041 = icmp.ne v1039, v1040
	cmp	x10, #32
	cset	w10, ne
	; v1042 = iconst.i32 154
	movz	x11, #0x9a
	; v1043 = select v1041, v1042, v1037
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1044 = iconst.i64 81985529216486895
	movz	x10, #0xcdef
	movk	x10, #0x89ab, lsl #16
	movk	x10, #0x4567, lsl #32
	movk	x10, #0x123, lsl #48
	; v1045 = bswap v1044
	rev	x10, x10
	; v1046 = iconst.i64 -1167088121787636991
	movz	x11, #0x2301
	movk	x11, #0x6745, lsl #16
	movk	x11, #0xab89, lsl #32
	movk	x11, #0xefcd, lsl #48
	; v1047 = icmp.ne v1045, v1046
	cmp	x10, x11
	cset	w10, ne
	; v1048 = iconst.i32 155
	movz	x11, #0x9b
	; v1049 = select v1047, v1048, v1043
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1050 = iconst.i64 4294967296
	movz	x10, #0x1, lsl #32
	; v1051 = ineg v1050
	neg	x10, x10
	; v1052 = iconst.i64 -4294967296
	movz	x11, #0xffff, lsl #32
	movk	x11, #0xffff, lsl #48
	; v1053 = icmp.ne v1051, v1052
	cmp	x10, x11
	cset	w10, ne
	; v1054 = iconst.i32 156
	movz	x11, #0x9c
	; v1055 = select v1053, v1054, v1049
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1056 = iconst.i64 4294967296
	movz	x10, #0x1, lsl #32
	; v1057 = bnot v1056
	mvn	x10, x10
	; v1058 = iconst.i64 -4294967297
	movn	x11, #0x1, lsl #32
	; v1059 = icmp.ne v1057, v1058
	cmp	x10, x11
	cset	w10, ne
	; v1060 = iconst.i32 157
	movz	x11, #0x9d
	; v1061 = select v1059, v1060, v1055
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1062 = iconst.i64 4294967296
	movz	x10, #0x1, lsl #32
	; v1063 = clz v1062
	clz	x10, x10
	; v1064 = iconst.i64 31
	; v1065 = icmp.ne v1063, v1064
	cmp	x10, #31
	cset	w10, ne
	; v1066 = iconst.i32 158
	movz	x11, #0x9e
	; v1067 = select v1065, v1066, v1061
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1068 = iconst.i64 4294967296
	movz	x10, #0x1, lsl #32
	; v1069 = ctz v1068
	rbit	x16, x10
	clz	x10, x16
	; v1070 = iconst.i64 32
	; v1071 = icmp.ne v1069, v1070
	cmp	x10, #32
	cset	w10, ne
	; v1072 = iconst.i32 159
	movz	x11, #0x9f
	; v1073 = select v1071, v1072, v1067
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1074 = iconst.i64 4294967296
	movz	x10, #0x1, lsl #32
	; v1075 = popcnt v1074
	fmov	d0, x10
	cnt	v0.8b, v0.8b
	addv	b0, v0.8b
	fmov	w10, s0
	; v1076 = iconst.i64 1
	; v1077 = icmp.ne v1075, v1076
	cmp	x10, #1
	cset	w10, ne
	; v1078 = iconst.i32 160
	movz	x11, #0xa0
	; v1079 = select v1077, v1078, v1073
	tst	w10, #255
	csel	w9, w11, w9, ne
	; v1080 = iconst.i64 4294967296
	movz	x10, #0x1, lsl #32
	; v1081 = bswap v1080
	rev	x10, x10
	; v1082 = iconst.i64 16777216
	movz	x11, #0x100, lsl #16
	; v1083 = icmp.ne v1081, v1082
	cmp	x10, x11
	cset	w10, ne
	; v1084 = iconst.i32 161
	movz	x11, #0xa1
	; v1085 = select v1083, v1084, v1079
	tst	w10, #255
	csel	w9, w11, w9, ne
	; return v1085
	mov	x0, x9
	ldp	x29, x30, [sp], #16
	ret
Lmain_end:
	.cfi_endproc
	.subsections_via_symbols
