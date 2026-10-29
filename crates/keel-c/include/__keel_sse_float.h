/* Architectural floating operations; keep operand order and MXCSR effects. */
#ifndef _KEEL_KEEL_SSE_FLOAT_H
#define _KEEL_KEEL_SSE_FLOAT_H
static __inline__ __attribute__((always_inline)) __m128 _mm_min_ps(__m128 a, __m128 b) { return __builtin_keel_sse_minps128(a,b); }
static __inline__ __attribute__((always_inline)) __m128 _mm_max_ps(__m128 a, __m128 b) { return __builtin_keel_sse_maxps128(a,b); }
static __inline__ __attribute__((always_inline)) int _mm_movemask_ps(__m128 a) { return __builtin_keel_sse_movmskps128(a); }
static __inline__ __attribute__((always_inline)) __m128 _mm_rcp_ps(__m128 a) { return __builtin_keel_sse_rcpps128(a); }
static __inline__ __attribute__((always_inline)) __m128 _mm_rsqrt_ps(__m128 a) { return __builtin_keel_sse_rsqrtps128(a); }
static __inline__ __attribute__((always_inline)) __m128 _mm_min_ss(__m128 a, __m128 b) { a[0] = __builtin_keel_sse_minss(a[0],b[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_max_ss(__m128 a, __m128 b) { a[0] = __builtin_keel_sse_maxss(a[0],b[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_rcp_ss(__m128 a) { a[0] = __builtin_keel_sse_rcpss(a[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_rsqrt_ss(__m128 a) { a[0] = __builtin_keel_sse_rsqrtss(a[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_sqrt_ss(__m128 a) { a[0] = __builtin_keel_sse_sqrtss(a[0]); return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_add_ss(__m128 a, __m128 b) { a[0] = a[0] + b[0]; return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_sub_ss(__m128 a, __m128 b) { a[0] = a[0] - b[0]; return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_mul_ss(__m128 a, __m128 b) { a[0] = a[0] * b[0]; return a; }
static __inline__ __attribute__((always_inline)) __m128 _mm_div_ss(__m128 a, __m128 b) { a[0] = a[0] / b[0]; return a; }
#endif
