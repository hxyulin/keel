/* Exact lane construction; included after the owning vector types. */
#ifndef _KEEL_SSE3_DUPLICATE_H
#define _KEEL_SSE3_DUPLICATE_H
static __inline__ __attribute__((always_inline)) __m128 _mm_movehdup_ps(__m128 a) {
  return (__m128)__builtin_shufflevector(a, a, 1, 1, 3, 3);
}
static __inline__ __attribute__((always_inline)) __m128 _mm_moveldup_ps(__m128 a) {
  return (__m128)__builtin_shufflevector(a, a, 0, 0, 2, 2);
}
static __inline__ __attribute__((always_inline)) __m128d _mm_movedup_pd(__m128d a) {
  return (__m128d)__builtin_shufflevector(a, a, 0, 0);
}
#endif /* _KEEL_SSE3_DUPLICATE_H */
