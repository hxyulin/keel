/* Exact-width memory transfers; no arithmetic on loaded bits. */
#ifndef _KEEL_SSE3_MEMORY_H
#define _KEEL_SSE3_MEMORY_H
static __inline__ __attribute__((always_inline)) __m128d _mm_loaddup_pd(const double*p) { return _mm_load1_pd(p); }
#endif
