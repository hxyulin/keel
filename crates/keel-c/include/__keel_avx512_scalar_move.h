/* Scalar AVX512F bit transfers. Only mask bit zero controls the low lane. */
#ifndef _KEEL_AVX512_SCALAR_MOVE_H
#define _KEEL_AVX512_SCALAR_MOVE_H
/* Register moves copy upper lanes from a, including when the low lane merges. */
#define _mm_mask_move_ss(src,k,a,b) __builtin_keel_avx512_maskmovess((__m128)(src),(signed char)(k),(__m128)(a),(__m128)(b))
#define _mm_maskz_move_ss(k,a,b) _mm_mask_move_ss((__m128){0},(k),(a),(b))
#define _mm_mask_move_sd(src,k,a,b) __builtin_keel_avx512_maskmovesd((__m128d)(src),(signed char)(k),(__m128d)(a),(__m128d)(b))
#define _mm_maskz_move_sd(k,a,b) _mm_mask_move_sd((__m128d){0},(k),(a),(b))
/* Memory loads clear upper XMM lanes even when the low lane merges. */
#define _mm_mask_load_ss(src,k,p) __builtin_keel_avx512_maskloadss((void*)(p),(__m128)(src),(signed char)(k))
#define _mm_maskz_load_ss(k,p) _mm_mask_load_ss((__m128){0},(k),(p))
#define _mm_mask_load_sd(src,k,p) __builtin_keel_avx512_maskloadsd((void*)(p),(__m128d)(src),(signed char)(k))
#define _mm_maskz_load_sd(k,p) _mm_mask_load_sd((__m128d){0},(k),(p))
#define _mm_mask_store_ss(p,k,a) __builtin_keel_avx512_maskstoress((void*)(p),(__m128)(a),(signed char)(k))
#define _mm_mask_store_sd(p,k,a) __builtin_keel_avx512_maskstoresd((void*)(p),(__m128d)(a),(signed char)(k))
#endif
