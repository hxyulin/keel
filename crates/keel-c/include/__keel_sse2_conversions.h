/* Private fragment of emmintrin.h; architectural MXCSR semantics. */
#ifndef _KEEL_SSE2_CONVERSIONS_H
#define _KEEL_SSE2_CONVERSIONS_H
#include <emmintrin.h>
static __inline__ __m128 _mm_cvtepi32_ps(__m128i a) {
  return __builtin_keel_sse2_cvtdq2ps((__keel_i32_128)a);
}
static __inline__ __m128i _mm_cvttps_epi32(__m128 a) {
  return (__m128i)__builtin_keel_sse2_cvttps2dq(a);
}
static __inline__ __m128i _mm_cvtps_epi32(__m128 a) {
  return (__m128i)__builtin_keel_sse2_cvtps2dq(a);
}
#endif
