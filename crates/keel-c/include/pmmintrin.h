/* Bounded Keel SSE3 intrinsic subset. */
#ifndef _KEEL_PMMINTRIN_H
#define _KEEL_PMMINTRIN_H
#ifndef __x86_64__
#error "Keel x86 intrinsic headers require x86-64"
#endif
#include <emmintrin.h>

static __inline__ __m128 _mm_hadd_ps(__m128 a, __m128 b) {
  return __builtin_keel_sse3_haddps(a, b);
}
static __inline__ __m128 _mm_hsub_ps(__m128 a, __m128 b) {
  return __builtin_keel_sse3_hsubps(a, b);
}
static __inline__ __m128d _mm_hadd_pd(__m128d a, __m128d b) {
  return __builtin_keel_sse3_haddpd(a, b);
}
static __inline__ __m128d _mm_hsub_pd(__m128d a, __m128d b) {
  return __builtin_keel_sse3_hsubpd(a, b);
}

static __inline__ __m128 _mm_addsub_ps(__m128 a, __m128 b) {
  return __builtin_keel_sse3_addsubps(a, b);
}
static __inline__ __m128d _mm_addsub_pd(__m128d a, __m128d b) {
  return __builtin_keel_sse3_addsubpd(a, b);
}

#endif /* _KEEL_PMMINTRIN_H */
