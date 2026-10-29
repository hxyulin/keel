/* Exact lane construction; included after the owning vector types. */
#ifndef _KEEL_SSE_CONSTRUCT_H
#define _KEEL_SSE_CONSTRUCT_H
static __inline__ __attribute__((always_inline)) __m128 _mm_setr_ps(float x0, float x1, float x2, float x3) {
  return (__m128){x0, x1, x2, x3};
}
#endif /* _KEEL_SSE_CONSTRUCT_H */
