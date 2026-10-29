/* Architectural conversions retain MXCSR rounding and exception effects. */
#ifndef _KEEL_SSE2_NUMERIC_H
#define _KEEL_SSE2_NUMERIC_H

static __inline__ __attribute__((always_inline)) __m128d _mm_cvtepi32_pd(__m128i b) {
  return (__m128d)__builtin_keel_sse_cvtdq2pd128((__keel_i32_128)b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtpd_epi32(__m128d b) {
  return (__m128i)__builtin_keel_sse_cvtpd2dq128(b);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cvtpd_ps(__m128d b) {
  return (__m128)__builtin_keel_sse_cvtpd2ps128(b);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cvtps_pd(__m128 b) {
  return (__m128d)__builtin_keel_sse_cvtps2pd128(b);
}
static __inline__ __attribute__((always_inline)) __m128i _mm_cvttpd_epi32(__m128d b) {
  return (__m128i)__builtin_keel_sse_cvttpd2dq128(b);
}
static __inline__ __attribute__((always_inline)) int _mm_cvtsd_si32(__m128d b) {
  return (int)__builtin_keel_sse_cvtsd2si32(b[0]);
}
static __inline__ __attribute__((always_inline)) int _mm_cvttsd_si32(__m128d b) {
  return (int)__builtin_keel_sse_cvttsd2si32(b[0]);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cvtsi32_sd(__m128d a, int b) {
  a[0] = __builtin_keel_sse_cvtsi2sd32(b);
  return a;
}
static __inline__ __attribute__((always_inline)) long long _mm_cvtsd_si64(__m128d b) {
  return (long long)__builtin_keel_sse_cvtsd2si64(b[0]);
}
static __inline__ __attribute__((always_inline)) long long _mm_cvttsd_si64(__m128d b) {
  return (long long)__builtin_keel_sse_cvttsd2si64(b[0]);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cvtsi64_sd(__m128d a, long long b) {
  a[0] = __builtin_keel_sse_cvtsi2sd64(b);
  return a;
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cvtsd_ss(__m128 a, __m128d b) {
  a[0] = __builtin_keel_sse_cvtsd2ss(b[0]);
  return a;
}
static __inline__ __attribute__((always_inline)) __m128d _mm_cvtss_sd(__m128d a, __m128 b) {
  a[0] = __builtin_keel_sse_cvtss2sd(b[0]);
  return a;
}

#endif /* _KEEL_SSE2_NUMERIC_H */
