#ifndef _KEEL_SSE_CONVENIENCE_H
#define _KEEL_SSE_CONVENIENCE_H
static __inline__ __attribute__((always_inline)) __m128 _mm_movehl_ps(__m128 a,__m128 b) { return __builtin_shufflevector(a,b,6,7,2,3); }
static __inline__ __attribute__((always_inline)) __m128 _mm_movelh_ps(__m128 a,__m128 b) { return __builtin_shufflevector(a,b,0,1,4,5); }
static __inline__ __attribute__((always_inline)) __m128 _mm_set_ps1(float a) { return (__m128){a,a,a,a}; }
static __inline__ __attribute__((always_inline)) __m128 _mm_set_ss(float a) { return (__m128){a,0,0,0}; }
static __inline__ __attribute__((always_inline)) __m128 _mm_undefined_ps(void) { return (__m128){0}; }
#endif /* _KEEL_SSE_CONVENIENCE_H */
