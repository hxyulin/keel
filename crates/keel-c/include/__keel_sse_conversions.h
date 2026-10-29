/* Architectural conversions retain MXCSR rounding and exception effects. */
#ifndef _KEEL_SSE_CONVERSIONS_H
#define _KEEL_SSE_CONVERSIONS_H

static __inline__ __attribute__((always_inline)) int _mm_cvtss_si32(__m128 b) {
  return (int)__builtin_keel_sse_cvtss2si32(b[0]);
}
static __inline__ __attribute__((always_inline)) int _mm_cvttss_si32(__m128 b) {
  return (int)__builtin_keel_sse_cvttss2si32(b[0]);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cvtsi32_ss(__m128 a, int b) {
  a[0] = __builtin_keel_sse_cvtsi2ss32(b);
  return a;
}
static __inline__ __attribute__((always_inline)) long long _mm_cvtss_si64(__m128 b) {
  return (long long)__builtin_keel_sse_cvtss2si64(b[0]);
}
static __inline__ __attribute__((always_inline)) long long _mm_cvttss_si64(__m128 b) {
  return (long long)__builtin_keel_sse_cvttss2si64(b[0]);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_cvtsi64_ss(__m128 a, long long b) {
  a[0] = __builtin_keel_sse_cvtsi2ss64(b);
  return a;
}
static __inline__ __attribute__((always_inline)) int _mm_cvt_ss2si(__m128 a) { return _mm_cvtss_si32(a); }
static __inline__ __attribute__((always_inline)) int _mm_cvtt_ss2si(__m128 a) { return _mm_cvttss_si32(a); }
static __inline__ __attribute__((always_inline)) __m128 _mm_cvt_si2ss(__m128 a, int b) { return _mm_cvtsi32_ss(a,b); }

#endif /* _KEEL_SSE_CONVERSIONS_H */
