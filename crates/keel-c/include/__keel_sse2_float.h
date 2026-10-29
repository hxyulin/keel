/* Architectural floating operations; keep operand order and MXCSR effects. */
#ifndef _KEEL_KEEL_SSE2_FLOAT_H
#define _KEEL_KEEL_SSE2_FLOAT_H
static __inline__ __attribute__((always_inline)) __m128d _mm_min_pd(__m128d a, __m128d b) { return __builtin_keel_sse_minpd128(a,b); }
static __inline__ __attribute__((always_inline)) __m128d _mm_max_pd(__m128d a, __m128d b) { return __builtin_keel_sse_maxpd128(a,b); }
static __inline__ __attribute__((always_inline)) int _mm_movemask_pd(__m128d a) { return __builtin_keel_sse_movmskpd128(a); }
static __inline__ __attribute__((always_inline)) __m128d _mm_min_sd(__m128d a, __m128d b) { a[0] = __builtin_keel_sse_minsd(a[0],b[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_max_sd(__m128d a, __m128d b) { a[0] = __builtin_keel_sse_maxsd(a[0],b[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_sqrt_sd(__m128d a, __m128d b) { a[0] = __builtin_keel_sse_sqrtsd(b[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_add_sd(__m128d a, __m128d b) { a[0] = a[0] + b[0]; return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_sub_sd(__m128d a, __m128d b) { a[0] = a[0] - b[0]; return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_mul_sd(__m128d a, __m128d b) { a[0] = a[0] * b[0]; return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_div_sd(__m128d a, __m128d b) { a[0] = a[0] / b[0]; return a; }
#endif
