/* Exact-width memory transfers; no arithmetic on loaded bits. */
#ifndef _KEEL_SSE_MEMORY_H
#define _KEEL_SSE_MEMORY_H
static __inline__ __attribute__((always_inline)) __m128 _mm_load_ss(const float *p) { return (__m128){*p, 0, 0, 0}; }
static __inline__ __attribute__((always_inline)) void _mm_store_ss(float *p, __m128 a) { *p = a[0]; }
static __inline__ __attribute__((always_inline)) __m128 _mm_load1_ps(const float *p) { float x = *p; return (__m128){x,x,x,x}; }
static __inline__ __attribute__((always_inline)) __m128 _mm_load_ps1(const float *p) { float x = *p; return (__m128){x,x,x,x}; }
static __inline__ __attribute__((always_inline)) void _mm_store1_ps(float *p, __m128 a) { *(__m128 *)p = __builtin_shufflevector(a,a,0,0,0,0); }
static __inline__ __attribute__((always_inline)) void _mm_store_ps1(float *p, __m128 a) { *(__m128 *)p = __builtin_shufflevector(a,a,0,0,0,0); }
static __inline__ __attribute__((always_inline)) __m128 _mm_loadr_ps(const float *p) { __m128 a = *(const __m128 *)p; return __builtin_shufflevector(a,a,3,2,1,0); }
static __inline__ __attribute__((always_inline)) void _mm_storer_ps(float *p, __m128 a) { *(__m128 *)p = __builtin_shufflevector(a,a,3,2,1,0); }
static __inline__ __attribute__((always_inline)) __m128 _mm_move_ss(__m128 a, __m128 b) { a[0]=b[0];return a; }
#endif
