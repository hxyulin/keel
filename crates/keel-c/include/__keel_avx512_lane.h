/* AVX512F lane wrappers, included by __keel_avx512f.h. */
#ifndef _KEEL_AVX512_LANE_H
#define _KEEL_AVX512_LANE_H

#define _mm512_mask_slli_epi32(src,k,a,imm) _mm512_mask_sll_epi32((src),(k),(a),((__m128i){(unsigned int)(imm),0}))
#define _mm512_maskz_slli_epi32(k,a,imm) _mm512_mask_slli_epi32(((__keel_avx512_i32x16){0}),(k),(a),(imm))
#define _mm512_slli_epi32(a,imm) _mm512_mask_slli_epi32(((__keel_avx512_i32x16){0}),0xffff,(a),(imm))
#define _mm512_mask_srli_epi32(src,k,a,imm) _mm512_mask_srl_epi32((src),(k),(a),((__m128i){(unsigned int)(imm),0}))
#define _mm512_maskz_srli_epi32(k,a,imm) _mm512_mask_srli_epi32(((__keel_avx512_i32x16){0}),(k),(a),(imm))
#define _mm512_srli_epi32(a,imm) _mm512_mask_srli_epi32(((__keel_avx512_i32x16){0}),0xffff,(a),(imm))
#define _mm512_mask_srai_epi32(src,k,a,imm) _mm512_mask_sra_epi32((src),(k),(a),((__m128i){(unsigned int)(imm),0}))
#define _mm512_maskz_srai_epi32(k,a,imm) _mm512_mask_srai_epi32(((__keel_avx512_i32x16){0}),(k),(a),(imm))
#define _mm512_srai_epi32(a,imm) _mm512_mask_srai_epi32(((__keel_avx512_i32x16){0}),0xffff,(a),(imm))
#define _mm512_mask_sll_epi32(src,k,a,count) ((__m512i)__builtin_keel_avx512_maskslld((__keel_avx512_i32x16)(src),(short)(k),(__keel_avx512_i32x16)(a),(__m128i)(count)))
#define _mm512_maskz_sll_epi32(k,a,count) _mm512_mask_sll_epi32(((__keel_avx512_i32x16){0}),(k),(a),(count))
#define _mm512_sll_epi32(a,count) _mm512_mask_sll_epi32(((__keel_avx512_i32x16){0}),0xffff,(a),(count))
#define _mm512_mask_srl_epi32(src,k,a,count) ((__m512i)__builtin_keel_avx512_masksrld((__keel_avx512_i32x16)(src),(short)(k),(__keel_avx512_i32x16)(a),(__m128i)(count)))
#define _mm512_maskz_srl_epi32(k,a,count) _mm512_mask_srl_epi32(((__keel_avx512_i32x16){0}),(k),(a),(count))
#define _mm512_srl_epi32(a,count) _mm512_mask_srl_epi32(((__keel_avx512_i32x16){0}),0xffff,(a),(count))
#define _mm512_mask_sra_epi32(src,k,a,count) ((__m512i)__builtin_keel_avx512_masksrad((__keel_avx512_i32x16)(src),(short)(k),(__keel_avx512_i32x16)(a),(__m128i)(count)))
#define _mm512_maskz_sra_epi32(k,a,count) _mm512_mask_sra_epi32(((__keel_avx512_i32x16){0}),(k),(a),(count))
#define _mm512_sra_epi32(a,count) _mm512_mask_sra_epi32(((__keel_avx512_i32x16){0}),0xffff,(a),(count))

#define _mm512_mask_slli_epi64(src,k,a,imm) _mm512_mask_sll_epi64((src),(k),(a),((__m128i){(unsigned int)(imm),0}))
#define _mm512_maskz_slli_epi64(k,a,imm) _mm512_mask_slli_epi64(((__m512i){0}),(k),(a),(imm))
#define _mm512_slli_epi64(a,imm) _mm512_mask_slli_epi64(((__m512i){0}),0xff,(a),(imm))
#define _mm512_mask_srli_epi64(src,k,a,imm) _mm512_mask_srl_epi64((src),(k),(a),((__m128i){(unsigned int)(imm),0}))
#define _mm512_maskz_srli_epi64(k,a,imm) _mm512_mask_srli_epi64(((__m512i){0}),(k),(a),(imm))
#define _mm512_srli_epi64(a,imm) _mm512_mask_srli_epi64(((__m512i){0}),0xff,(a),(imm))
#define _mm512_mask_srai_epi64(src,k,a,imm) _mm512_mask_sra_epi64((src),(k),(a),((__m128i){(unsigned int)(imm),0}))
#define _mm512_maskz_srai_epi64(k,a,imm) _mm512_mask_srai_epi64(((__m512i){0}),(k),(a),(imm))
#define _mm512_srai_epi64(a,imm) _mm512_mask_srai_epi64(((__m512i){0}),0xff,(a),(imm))
#define _mm512_mask_sll_epi64(src,k,a,count) ((__m512i)__builtin_keel_avx512_masksllq((__m512i)(src),(signed char)(k),(__m512i)(a),(__m128i)(count)))
#define _mm512_maskz_sll_epi64(k,a,count) _mm512_mask_sll_epi64(((__m512i){0}),(k),(a),(count))
#define _mm512_sll_epi64(a,count) _mm512_mask_sll_epi64(((__m512i){0}),0xff,(a),(count))
#define _mm512_mask_srl_epi64(src,k,a,count) ((__m512i)__builtin_keel_avx512_masksrlq((__m512i)(src),(signed char)(k),(__m512i)(a),(__m128i)(count)))
#define _mm512_maskz_srl_epi64(k,a,count) _mm512_mask_srl_epi64(((__m512i){0}),(k),(a),(count))
#define _mm512_srl_epi64(a,count) _mm512_mask_srl_epi64(((__m512i){0}),0xff,(a),(count))
#define _mm512_mask_sra_epi64(src,k,a,count) ((__m512i)__builtin_keel_avx512_masksraq((__m512i)(src),(signed char)(k),(__m512i)(a),(__m128i)(count)))
#define _mm512_maskz_sra_epi64(k,a,count) _mm512_mask_sra_epi64(((__m512i){0}),(k),(a),(count))
#define _mm512_sra_epi64(a,count) _mm512_mask_sra_epi64(((__m512i){0}),0xff,(a),(count))

#define _mm512_mask_broadcastd_epi32(src,k,a) ((__m512i)__builtin_keel_avx512_maskpbroadcastd((__keel_avx512_i32x16)(src),(short)(k),((__keel_i32_128)(a))[0]))
#define _mm512_maskz_broadcastd_epi32(k,a) _mm512_mask_broadcastd_epi32(((__keel_avx512_i32x16){0}),(k),(a))
#define _mm512_broadcastd_epi32(a) _mm512_mask_broadcastd_epi32(((__keel_avx512_i32x16){0}),0xffff,(a))
#define __keel_mm512_mask_load_broadcastd_epi32(src,k,p) ((__m512i)__builtin_keel_avx512_maskloadpbroadcastd((void*)(p),(__keel_avx512_i32x16)(src),(short)(k)))

#define _mm512_mask_broadcastq_epi64(src,k,a) ((__m512i)__builtin_keel_avx512_maskpbroadcastq((__m512i)(src),(signed char)(k),((__m128i)(a))[0]))
#define _mm512_maskz_broadcastq_epi64(k,a) _mm512_mask_broadcastq_epi64(((__m512i){0}),(k),(a))
#define _mm512_broadcastq_epi64(a) _mm512_mask_broadcastq_epi64(((__m512i){0}),0xff,(a))
#define __keel_mm512_mask_load_broadcastq_epi64(src,k,p) ((__m512i)__builtin_keel_avx512_maskloadpbroadcastq((void*)(p),(__m512i)(src),(signed char)(k)))

#define _mm512_mask_broadcastss_ps(src,k,a) ((__m512)__builtin_keel_avx512_maskbroadcastss((__m512)(src),(short)(k),((__m128)(a))[0]))
#define _mm512_maskz_broadcastss_ps(k,a) _mm512_mask_broadcastss_ps(((__m512){0}),(k),(a))
#define _mm512_broadcastss_ps(a) _mm512_mask_broadcastss_ps(((__m512){0}),0xffff,(a))
#define __keel_mm512_mask_load_broadcastss_ps(src,k,p) ((__m512)__builtin_keel_avx512_maskloadbroadcastss((void*)(p),(__m512)(src),(short)(k)))

#define _mm512_mask_broadcastsd_pd(src,k,a) ((__m512d)__builtin_keel_avx512_maskbroadcastsd((__m512d)(src),(signed char)(k),((__m128d)(a))[0]))
#define _mm512_maskz_broadcastsd_pd(k,a) _mm512_mask_broadcastsd_pd(((__m512d){0}),(k),(a))
#define _mm512_broadcastsd_pd(a) _mm512_mask_broadcastsd_pd(((__m512d){0}),0xff,(a))
#define __keel_mm512_mask_load_broadcastsd_pd(src,k,p) ((__m512d)__builtin_keel_avx512_maskloadbroadcastsd((void*)(p),(__m512d)(src),(signed char)(k)))

#define _mm512_mask_set1_epi32(src,k,a) ((__m512i)__builtin_keel_avx512_maskpbroadcastd((__keel_avx512_i32x16)(src),(short)(k),(int)(a)))
#define _mm512_maskz_set1_epi32(k,a) _mm512_mask_set1_epi32(((__keel_avx512_i32x16){0}),(k),(a))

#define _mm512_mask_set1_epi64(src,k,a) ((__m512i)__builtin_keel_avx512_maskpbroadcastq((__m512i)(src),(signed char)(k),(long long)(a)))
#define _mm512_maskz_set1_epi64(k,a) _mm512_mask_set1_epi64(((__m512i){0}),(k),(a))

#define _mm512_mask_broadcast_i32x4(src,k,a) ((__m512i)__builtin_keel_avx512_maskbroadcasti32x4((__keel_avx512_i32x16)(src),(short)(k),(__keel_i32_128)(a)))
#define _mm512_maskz_broadcast_i32x4(k,a) _mm512_mask_broadcast_i32x4(((__keel_avx512_i32x16){0}),(k),(a))
#define _mm512_broadcast_i32x4(a) _mm512_mask_broadcast_i32x4(((__keel_avx512_i32x16){0}),0xffff,(a))
#define __keel_mm512_mask_load_broadcast_i32x4(src,k,p) ((__m512i)__builtin_keel_avx512_maskloadbroadcasti32x4((void*)(p),(__keel_avx512_i32x16)(src),(short)(k)))

#define _mm512_mask_extracti32x4_epi32(src,k,a,lane) ((__m128i)__builtin_keel_avx512_maskextracti32x4((__keel_i32_128)(src),(signed char)(k),(__keel_avx512_i32x16)(a),(lane)))
#define _mm512_maskz_extracti32x4_epi32(k,a,lane) _mm512_mask_extracti32x4_epi32(((__keel_i32_128){0}),(k),(a),(lane))
#define _mm512_extracti32x4_epi32(a,lane) _mm512_mask_extracti32x4_epi32(((__keel_i32_128){0}),0xff,(a),(lane))
#define __keel_mm512_mask_store_extracti32x4_epi32(p,k,a,lane) __builtin_keel_avx512_maskstoreextracti32x4((void*)(p),(__keel_avx512_i32x16)(a),(signed char)(k),(lane))

#define _mm512_mask_inserti32x4(src,k,a,b,lane) ((__m512i)__builtin_keel_avx512_maskinserti32x4((__keel_avx512_i32x16)(src),(short)(k),(__keel_avx512_i32x16)(a),(__keel_i32_128)(b),(lane)))
#define _mm512_maskz_inserti32x4(k,a,b,lane) _mm512_mask_inserti32x4(((__keel_avx512_i32x16){0}),(k),(a),(b),(lane))
#define _mm512_inserti32x4(a,b,lane) _mm512_mask_inserti32x4(((__keel_avx512_i32x16){0}),0xffff,(a),(b),(lane))
#define __keel_mm512_mask_load_inserti32x4(src,k,a,p,lane) ((__m512i)__builtin_keel_avx512_maskloadinserti32x4((void*)(p),(__keel_avx512_i32x16)(src),(short)(k),(__keel_avx512_i32x16)(a),(lane)))

#define _mm512_mask_broadcast_i64x4(src,k,a) ((__m512i)__builtin_keel_avx512_maskbroadcasti64x4((__m512i)(src),(signed char)(k),(__m256i)(a)))
#define _mm512_maskz_broadcast_i64x4(k,a) _mm512_mask_broadcast_i64x4(((__m512i){0}),(k),(a))
#define _mm512_broadcast_i64x4(a) _mm512_mask_broadcast_i64x4(((__m512i){0}),0xff,(a))
#define __keel_mm512_mask_load_broadcast_i64x4(src,k,p) ((__m512i)__builtin_keel_avx512_maskloadbroadcasti64x4((void*)(p),(__m512i)(src),(signed char)(k)))

#define _mm512_mask_extracti64x4_epi64(src,k,a,lane) ((__m256i)__builtin_keel_avx512_maskextracti64x4((__m256i)(src),(signed char)(k),(__m512i)(a),(lane)))
#define _mm512_maskz_extracti64x4_epi64(k,a,lane) _mm512_mask_extracti64x4_epi64(((__m256i){0}),(k),(a),(lane))
#define _mm512_extracti64x4_epi64(a,lane) _mm512_mask_extracti64x4_epi64(((__m256i){0}),0xff,(a),(lane))
#define __keel_mm512_mask_store_extracti64x4_epi64(p,k,a,lane) __builtin_keel_avx512_maskstoreextracti64x4((void*)(p),(__m512i)(a),(signed char)(k),(lane))

#define _mm512_mask_inserti64x4(src,k,a,b,lane) ((__m512i)__builtin_keel_avx512_maskinserti64x4((__m512i)(src),(signed char)(k),(__m512i)(a),(__m256i)(b),(lane)))
#define _mm512_maskz_inserti64x4(k,a,b,lane) _mm512_mask_inserti64x4(((__m512i){0}),(k),(a),(b),(lane))
#define _mm512_inserti64x4(a,b,lane) _mm512_mask_inserti64x4(((__m512i){0}),0xff,(a),(b),(lane))
#define __keel_mm512_mask_load_inserti64x4(src,k,a,p,lane) ((__m512i)__builtin_keel_avx512_maskloadinserti64x4((void*)(p),(__m512i)(src),(signed char)(k),(__m512i)(a),(lane)))

#define _mm512_mask_broadcast_f32x4(src,k,a) ((__m512)__builtin_keel_avx512_maskbroadcastf32x4((__m512)(src),(short)(k),(__m128)(a)))
#define _mm512_maskz_broadcast_f32x4(k,a) _mm512_mask_broadcast_f32x4(((__m512){0}),(k),(a))
#define _mm512_broadcast_f32x4(a) _mm512_mask_broadcast_f32x4(((__m512){0}),0xffff,(a))
#define __keel_mm512_mask_load_broadcast_f32x4(src,k,p) ((__m512)__builtin_keel_avx512_maskloadbroadcastf32x4((void*)(p),(__m512)(src),(short)(k)))

#define _mm512_mask_extractf32x4_ps(src,k,a,lane) ((__m128)__builtin_keel_avx512_maskextractf32x4((__m128)(src),(signed char)(k),(__m512)(a),(lane)))
#define _mm512_maskz_extractf32x4_ps(k,a,lane) _mm512_mask_extractf32x4_ps(((__m128){0}),(k),(a),(lane))
#define _mm512_extractf32x4_ps(a,lane) _mm512_mask_extractf32x4_ps(((__m128){0}),0xff,(a),(lane))
#define __keel_mm512_mask_store_extractf32x4_ps(p,k,a,lane) __builtin_keel_avx512_maskstoreextractf32x4((void*)(p),(__m512)(a),(signed char)(k),(lane))

#define _mm512_mask_insertf32x4(src,k,a,b,lane) ((__m512)__builtin_keel_avx512_maskinsertf32x4((__m512)(src),(short)(k),(__m512)(a),(__m128)(b),(lane)))
#define _mm512_maskz_insertf32x4(k,a,b,lane) _mm512_mask_insertf32x4(((__m512){0}),(k),(a),(b),(lane))
#define _mm512_insertf32x4(a,b,lane) _mm512_mask_insertf32x4(((__m512){0}),0xffff,(a),(b),(lane))
#define __keel_mm512_mask_load_insertf32x4(src,k,a,p,lane) ((__m512)__builtin_keel_avx512_maskloadinsertf32x4((void*)(p),(__m512)(src),(short)(k),(__m512)(a),(lane)))

#define _mm512_mask_broadcast_f64x4(src,k,a) ((__m512d)__builtin_keel_avx512_maskbroadcastf64x4((__m512d)(src),(signed char)(k),(__m256d)(a)))
#define _mm512_maskz_broadcast_f64x4(k,a) _mm512_mask_broadcast_f64x4(((__m512d){0}),(k),(a))
#define _mm512_broadcast_f64x4(a) _mm512_mask_broadcast_f64x4(((__m512d){0}),0xff,(a))
#define __keel_mm512_mask_load_broadcast_f64x4(src,k,p) ((__m512d)__builtin_keel_avx512_maskloadbroadcastf64x4((void*)(p),(__m512d)(src),(signed char)(k)))

#define _mm512_mask_extractf64x4_pd(src,k,a,lane) ((__m256d)__builtin_keel_avx512_maskextractf64x4((__m256d)(src),(signed char)(k),(__m512d)(a),(lane)))
#define _mm512_maskz_extractf64x4_pd(k,a,lane) _mm512_mask_extractf64x4_pd(((__m256d){0}),(k),(a),(lane))
#define _mm512_extractf64x4_pd(a,lane) _mm512_mask_extractf64x4_pd(((__m256d){0}),0xff,(a),(lane))
#define __keel_mm512_mask_store_extractf64x4_pd(p,k,a,lane) __builtin_keel_avx512_maskstoreextractf64x4((void*)(p),(__m512d)(a),(signed char)(k),(lane))

#define _mm512_mask_insertf64x4(src,k,a,b,lane) ((__m512d)__builtin_keel_avx512_maskinsertf64x4((__m512d)(src),(signed char)(k),(__m512d)(a),(__m256d)(b),(lane)))
#define _mm512_maskz_insertf64x4(k,a,b,lane) _mm512_mask_insertf64x4(((__m512d){0}),(k),(a),(b),(lane))
#define _mm512_insertf64x4(a,b,lane) _mm512_mask_insertf64x4(((__m512d){0}),0xff,(a),(b),(lane))
#define __keel_mm512_mask_load_insertf64x4(src,k,a,p,lane) ((__m512d)__builtin_keel_avx512_maskloadinsertf64x4((void*)(p),(__m512d)(src),(signed char)(k),(__m512d)(a),(lane)))

#endif
