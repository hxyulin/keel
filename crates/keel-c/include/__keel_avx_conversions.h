/* Architectural conversions retain MXCSR rounding and exception effects. */
#ifndef _KEEL_AVX_CONVERSIONS_H
#define _KEEL_AVX_CONVERSIONS_H

static __inline__ __attribute__((always_inline)) __m256d _mm256_cvtepi32_pd(__m128i b) {
  return (__m256d)__builtin_keel_avx_cvtdq2pd256((__keel_i32_128)b);
}
static __inline__ __attribute__((always_inline)) __m256 _mm256_cvtepi32_ps(__m256i b) {
  return (__m256)__builtin_keel_avx_cvtdq2ps256((__keel_i32_256)b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm256_cvtpd_epi32(__m256d b) {
  return (__m128i)__builtin_keel_avx_cvtpd2dq256(b);
}
static __inline__ __attribute__((always_inline)) __m128 _mm256_cvtpd_ps(__m256d b) {
  return (__m128)__builtin_keel_avx_cvtpd2ps256(b);
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvtps_epi32(__m256 b) {
  return (__m256i)__builtin_keel_avx_cvtps2dq256(b);
}
static __inline__ __attribute__((always_inline)) __m256d _mm256_cvtps_pd(__m128 b) {
  return (__m256d)__builtin_keel_avx_cvtps2pd256(b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm256_cvttpd_epi32(__m256d b) {
  return (__m128i)__builtin_keel_avx_cvttpd2dq256(b);
}
static __inline__ __attribute__((always_inline)) __m256i _mm256_cvttps_epi32(__m256 b) {
  return (__m256i)__builtin_keel_avx_cvttps2dq256(b);
}

#endif /* _KEEL_AVX_CONVERSIONS_H */
