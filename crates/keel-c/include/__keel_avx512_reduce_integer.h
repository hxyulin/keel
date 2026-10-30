/* AVX512F integer reductions. Inactive lanes use the operation's identity. */
#ifndef _KEEL_AVX512_REDUCE_INTEGER_H
#define _KEEL_AVX512_REDUCE_INTEGER_H
static __inline__ __attribute__((always_inline)) int _mm512_reduce_add_epi32(__m512i a) { return __builtin_keel_avx512_reduce_add_i32x16((__keel_avx512_i32x16)a); }
static __inline__ __attribute__((always_inline)) int _mm512_reduce_and_epi32(__m512i a) { return __builtin_keel_avx512_reduce_and_i32x16((__keel_avx512_i32x16)a); }
static __inline__ __attribute__((always_inline)) int _mm512_reduce_or_epi32(__m512i a) { return __builtin_keel_avx512_reduce_or_i32x16((__keel_avx512_i32x16)a); }
static __inline__ __attribute__((always_inline)) int _mm512_reduce_min_epi32(__m512i a) { return __builtin_keel_avx512_reduce_smin_i32x16((__keel_avx512_i32x16)a); }
static __inline__ __attribute__((always_inline)) int _mm512_reduce_max_epi32(__m512i a) { return __builtin_keel_avx512_reduce_smax_i32x16((__keel_avx512_i32x16)a); }
static __inline__ __attribute__((always_inline)) unsigned int _mm512_reduce_min_epu32(__m512i a) { return (unsigned int)__builtin_keel_avx512_reduce_umin_i32x16((__keel_avx512_i32x16)a); }
static __inline__ __attribute__((always_inline)) unsigned int _mm512_reduce_max_epu32(__m512i a) { return (unsigned int)__builtin_keel_avx512_reduce_umax_i32x16((__keel_avx512_i32x16)a); }
static __inline__ __attribute__((always_inline)) long long _mm512_reduce_add_epi64(__m512i a) { return __builtin_keel_avx512_reduce_add_i64x8(a); }
static __inline__ __attribute__((always_inline)) long long _mm512_reduce_and_epi64(__m512i a) { return __builtin_keel_avx512_reduce_and_i64x8(a); }
static __inline__ __attribute__((always_inline)) long long _mm512_reduce_or_epi64(__m512i a) { return __builtin_keel_avx512_reduce_or_i64x8(a); }
static __inline__ __attribute__((always_inline)) long long _mm512_reduce_min_epi64(__m512i a) { return __builtin_keel_avx512_reduce_smin_i64x8(a); }
static __inline__ __attribute__((always_inline)) long long _mm512_reduce_max_epi64(__m512i a) { return __builtin_keel_avx512_reduce_smax_i64x8(a); }
static __inline__ __attribute__((always_inline)) unsigned long long _mm512_reduce_min_epu64(__m512i a) { return (unsigned long long)__builtin_keel_avx512_reduce_umin_i64x8(a); }
static __inline__ __attribute__((always_inline)) unsigned long long _mm512_reduce_max_epu64(__m512i a) { return (unsigned long long)__builtin_keel_avx512_reduce_umax_i64x8(a); }
/* Each shuffle halves the number of independent products. Both multiply helpers
 * use wrapping unsigned lanes; qword multiplication needs no AVX512DQ form. */
static __inline__ __attribute__((always_inline)) int _mm512_reduce_mul_epi32(__m512i a) {
  __m512i x=_mm512_mullo_epi32(a,_mm512_shuffle_i32x4(a,a,0x4e));
  x=_mm512_mullo_epi32(x,_mm512_shuffle_i32x4(x,x,0xb1));
  x=_mm512_mullo_epi32(x,_mm512_shuffle_epi32(x,0x4e));
  x=_mm512_mullo_epi32(x,_mm512_shuffle_epi32(x,0xb1));
  return (int)((__keel_avx512_u32)x)[0];
}
static __inline__ __attribute__((always_inline)) long long _mm512_reduce_mul_epi64(__m512i a) {
  __m512i x=_mm512_mullox_epi64(a,_mm512_shuffle_i64x2(a,a,0x4e));
  x=_mm512_mullox_epi64(x,_mm512_shuffle_i64x2(x,x,0xb1));
  x=_mm512_mullox_epi64(x,_mm512_shuffle_epi32(x,0x4e));
  return (long long)((__keel_avx512_u64)x)[0];
}
#define _mm512_mask_reduce_add_epi32(k,a) _mm512_reduce_add_epi32(_mm512_maskz_mov_epi32((k),(a)))
#define _mm512_mask_reduce_add_epi64(k,a) _mm512_reduce_add_epi64(_mm512_maskz_mov_epi64((k),(a)))
#define _mm512_mask_reduce_mul_epi32(k,a) _mm512_reduce_mul_epi32(_mm512_mask_mov_epi32(_mm512_set1_epi32(1),(k),(a)))
#define _mm512_mask_reduce_mul_epi64(k,a) _mm512_reduce_mul_epi64(_mm512_mask_mov_epi64(_mm512_set1_epi64(1),(k),(a)))
#define _mm512_mask_reduce_and_epi32(k,a) _mm512_reduce_and_epi32(_mm512_mask_mov_epi32(_mm512_set1_epi32(-1),(k),(a)))
#define _mm512_mask_reduce_and_epi64(k,a) _mm512_reduce_and_epi64(_mm512_mask_mov_epi64(_mm512_set1_epi64(-1),(k),(a)))
#define _mm512_mask_reduce_or_epi32(k,a) _mm512_reduce_or_epi32(_mm512_maskz_mov_epi32((k),(a)))
#define _mm512_mask_reduce_or_epi64(k,a) _mm512_reduce_or_epi64(_mm512_maskz_mov_epi64((k),(a)))
#define _mm512_mask_reduce_min_epi32(k,a) _mm512_reduce_min_epi32(_mm512_mask_mov_epi32(_mm512_set1_epi32(2147483647),(k),(a)))
#define _mm512_mask_reduce_min_epi64(k,a) _mm512_reduce_min_epi64(_mm512_mask_mov_epi64(_mm512_set1_epi64(9223372036854775807LL),(k),(a)))
#define _mm512_mask_reduce_max_epi32(k,a) _mm512_reduce_max_epi32(_mm512_mask_mov_epi32(_mm512_set1_epi32(-2147483647-1),(k),(a)))
#define _mm512_mask_reduce_max_epi64(k,a) _mm512_reduce_max_epi64(_mm512_mask_mov_epi64(_mm512_set1_epi64(-9223372036854775807LL-1),(k),(a)))
#define _mm512_mask_reduce_min_epu32(k,a) _mm512_reduce_min_epu32(_mm512_mask_mov_epi32(_mm512_set1_epi32(-1),(k),(a)))
#define _mm512_mask_reduce_min_epu64(k,a) _mm512_reduce_min_epu64(_mm512_mask_mov_epi64(_mm512_set1_epi64(-1),(k),(a)))
#define _mm512_mask_reduce_max_epu32(k,a) _mm512_reduce_max_epu32(_mm512_maskz_mov_epi32((k),(a)))
#define _mm512_mask_reduce_max_epu64(k,a) _mm512_reduce_max_epu64(_mm512_maskz_mov_epi64((k),(a)))
#endif
