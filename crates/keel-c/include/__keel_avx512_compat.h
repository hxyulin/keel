/* AVX512F aliases retain shortened widths and single evaluation. */
#ifndef _KEEL_AVX512_COMPAT_H
#define _KEEL_AVX512_COMPAT_H
#define _mm512_loadu_epi32(p) _mm512_loadu_si512((p))
#define _mm512_loadu_epi64(p) _mm512_loadu_si512((p))
#define _mm512_storeu_epi32(p,a) _mm512_storeu_si512((p),(a))
#define _mm512_storeu_epi64(p,a) _mm512_storeu_si512((p),(a))
#define _mm512_cvtepi32lo_pd(a) _mm512_cvtepi32_pd(_mm512_castsi512_si256((a)))
#define _mm512_mask_cvtepi32lo_pd(src,k,a) _mm512_mask_cvtepi32_pd((src),(k),_mm512_castsi512_si256((a)))
#define _mm512_cvtepu32lo_pd(a) _mm512_cvtepu32_pd(_mm512_castsi512_si256((a)))
#define _mm512_mask_cvtepu32lo_pd(src,k,a) _mm512_mask_cvtepu32_pd((src),(k),_mm512_castsi512_si256((a)))
#define _mm512_cvtpslo_pd(a) _mm512_cvtps_pd(_mm512_castps512_ps256((a)))
#define _mm512_mask_cvtpslo_pd(src,k,a) _mm512_mask_cvtps_pd((src),(k),_mm512_castps512_ps256((a)))
#define _mm512_cvtpd_pslo(a) _mm512_zextps256_ps512(_mm512_cvtpd_ps((a)))
#define _mm512_mask_cvtpd_pslo(src,k,a) _mm512_zextps256_ps512(_mm512_mask_cvtpd_ps(_mm512_castps512_ps256((src)),(k),(a)))
#define _mm512_i32logather_epi64(index,p,scale) _mm512_i32gather_epi64(_mm512_castsi512_si256((index)),(p),(scale))
#define _mm512_mask_i32logather_epi64(src,k,index,p,scale) _mm512_mask_i32gather_epi64((src),(k),_mm512_castsi512_si256((index)),(p),(scale))
#define _mm512_i32logather_pd(index,p,scale) _mm512_i32gather_pd(_mm512_castsi512_si256((index)),(p),(scale))
#define _mm512_mask_i32logather_pd(src,k,index,p,scale) _mm512_mask_i32gather_pd((src),(k),_mm512_castsi512_si256((index)),(p),(scale))
#define _mm512_i32loscatter_epi64(p,index,a,scale) _mm512_i32scatter_epi64((p),_mm512_castsi512_si256((index)),(a),(scale))
#define _mm512_mask_i32loscatter_epi64(p,k,index,a,scale) _mm512_mask_i32scatter_epi64((p),(k),_mm512_castsi512_si256((index)),(a),(scale))
#define _mm512_i32loscatter_pd(p,index,a,scale) _mm512_i32scatter_pd((p),_mm512_castsi512_si256((index)),(a),(scale))
#define _mm512_mask_i32loscatter_pd(p,k,index,a,scale) _mm512_mask_i32scatter_pd((p),(k),_mm512_castsi512_si256((index)),(a),(scale))
static __inline__ __attribute__((always_inline)) __mmask16 _mm512_int2mask(int a) { return (__mmask16)a; }
static __inline__ __attribute__((always_inline)) int _mm512_mask2int(__mmask16 a) { return (int)a; }
static __inline__ __attribute__((always_inline)) __mmask16 _mm512_kmov(__mmask16 a) { return a; }
#define _mm512_permutevar_epi32(index,a) _mm512_permutexvar_epi32((index),(a))
#define _mm512_mask_permutevar_epi32(src,k,index,a) _mm512_mask_permutexvar_epi32((src),(k),(index),(a))
static __inline__ __attribute__((always_inline)) __m512 _mm512_undefined(void) { return (__m512){0}; }
static __inline__ __attribute__((always_inline)) __m512 _mm512_undefined_ps(void) { return (__m512){0}; }
static __inline__ __attribute__((always_inline)) __m512d _mm512_undefined_pd(void) { return (__m512d){0}; }
static __inline__ __attribute__((always_inline)) __m512i _mm512_undefined_epi32(void) { return (__m512i){0}; }
/* FIXUP's active response may select original A. Zero only after masked FIXUP. */
#define _mm512_maskz_fixupimm_ps(k,a,b,c,imm) ({ __mmask16 __keel_fix_k=(__mmask16)(k); __m512 __keel_fix_r=_mm512_mask_fixupimm_ps((a),__keel_fix_k,(b),(c),(imm)); _mm512_maskz_mov_ps(__keel_fix_k,__keel_fix_r); })
#define _mm512_maskz_fixupimm_pd(k,a,b,c,imm) ({ __mmask8 __keel_fix_k=(__mmask8)(k); __m512d __keel_fix_r=_mm512_mask_fixupimm_pd((a),__keel_fix_k,(b),(c),(imm)); _mm512_maskz_mov_pd(__keel_fix_k,__keel_fix_r); })
#define _mm512_maskz_fixupimm_round_ps(k,a,b,c,imm,r) ({ __mmask16 __keel_fix_k=(__mmask16)(k); __m512 __keel_fix_r=_mm512_mask_fixupimm_round_ps((a),__keel_fix_k,(b),(c),(imm),(r)); _mm512_maskz_mov_ps(__keel_fix_k,__keel_fix_r); })
#define _mm512_maskz_fixupimm_round_pd(k,a,b,c,imm,r) ({ __mmask8 __keel_fix_k=(__mmask8)(k); __m512d __keel_fix_r=_mm512_mask_fixupimm_round_pd((a),__keel_fix_k,(b),(c),(imm),(r)); _mm512_maskz_mov_pd(__keel_fix_k,__keel_fix_r); })
#define _mm_maskz_fixupimm_ss(k,a,b,c,imm) ({ __mmask8 __keel_fix_k=(__mmask8)(k); __m128 __keel_fix_r=_mm_mask_fixupimm_ss((a),__keel_fix_k,(b),(c),(imm)); _mm_maskz_move_ss(__keel_fix_k,__keel_fix_r,__keel_fix_r); })
#define _mm_maskz_fixupimm_sd(k,a,b,c,imm) ({ __mmask8 __keel_fix_k=(__mmask8)(k); __m128d __keel_fix_r=_mm_mask_fixupimm_sd((a),__keel_fix_k,(b),(c),(imm)); _mm_maskz_move_sd(__keel_fix_k,__keel_fix_r,__keel_fix_r); })
#define _mm_maskz_fixupimm_round_ss(k,a,b,c,imm,r) ({ __mmask8 __keel_fix_k=(__mmask8)(k); __m128 __keel_fix_r=_mm_mask_fixupimm_round_ss((a),__keel_fix_k,(b),(c),(imm),(r)); _mm_maskz_move_ss(__keel_fix_k,__keel_fix_r,__keel_fix_r); })
#define _mm_maskz_fixupimm_round_sd(k,a,b,c,imm,r) ({ __mmask8 __keel_fix_k=(__mmask8)(k); __m128d __keel_fix_r=_mm_mask_fixupimm_round_sd((a),__keel_fix_k,(b),(c),(imm),(r)); _mm_maskz_move_sd(__keel_fix_k,__keel_fix_r,__keel_fix_r); })
#define _mm512_abs_ps(a) ((__m512)_mm512_and_epi32((__m512i)(a),_mm512_set1_epi32(0x7fffffff)))
#define _mm512_mask_abs_ps(src,k,a) ((__m512)_mm512_mask_and_epi32((__m512i)(src),(k),(__m512i)(a),_mm512_set1_epi32(0x7fffffff)))
#define _mm512_abs_pd(a) ((__m512d)_mm512_and_epi64((__m512i)(a),_mm512_set1_epi64(0x7fffffffffffffffLL)))
#define _mm512_mask_abs_pd(src,k,a) ((__m512d)_mm512_mask_and_epi64((__m512i)(src),(k),(__m512i)(a),_mm512_set1_epi64(0x7fffffffffffffffLL)))
#define _mm512_ceil_ps(a) _mm512_roundscale_ps((a),2)
#define _mm512_mask_ceil_ps(src,k,a) _mm512_mask_roundscale_ps((src),(k),(a),2)
#define _mm512_floor_ps(a) _mm512_roundscale_ps((a),1)
#define _mm512_mask_floor_ps(src,k,a) _mm512_mask_roundscale_ps((src),(k),(a),1)
#define _mm512_ceil_pd(a) _mm512_roundscale_pd((a),2)
#define _mm512_mask_ceil_pd(src,k,a) _mm512_mask_roundscale_pd((src),(k),(a),2)
#define _mm512_floor_pd(a) _mm512_roundscale_pd((a),1)
#define _mm512_mask_floor_pd(src,k,a) _mm512_mask_roundscale_pd((src),(k),(a),1)
/* Unsigned vector multiplication lowers through wrapping scalar qword lanes. */
static __inline__ __attribute__((always_inline)) __m512i _mm512_mullox_epi64(__m512i a, __m512i b) { return (__m512i)((__keel_avx512_u64)a * (__keel_avx512_u64)b); }
#define _mm512_mask_mullox_epi64(src,k,a,b) _mm512_mask_mov_epi64((src),(k),_mm512_mullox_epi64((a),(b)))
#endif
