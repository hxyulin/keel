/* Exact-width memory transfers; no arithmetic on loaded bits. */
#ifndef _KEEL_SSE_MEMORY_H
#define _KEEL_SSE_MEMORY_H
static __inline__ __attribute__((always_inline)) __m128 _mm_load_ss(const float *p) { __m128 r={0}; float x; __builtin_memcpy(&x,p,4); r[0]=x; return r; }
static __inline__ __attribute__((always_inline)) void _mm_store_ss(float *p, __m128 a) { float x=a[0]; __builtin_memcpy(p,&x,4); }
static __inline__ __attribute__((always_inline)) __m128 _mm_load1_ps(const float *p) { float x; __builtin_memcpy(&x,p,4); return (__m128){x,x,x,x}; }
static __inline__ __attribute__((always_inline)) __m128 _mm_load_ps1(const float *p) { float x; __builtin_memcpy(&x,p,4); return (__m128){x,x,x,x}; }
static __inline__ __attribute__((always_inline)) void _mm_store1_ps(float *p, __m128 a) { float x=a[0]; __m128 r={x,x,x,x}; __builtin_memcpy(p,&r,16); }
static __inline__ __attribute__((always_inline)) void _mm_store_ps1(float *p, __m128 a) { float x=a[0]; __m128 r={x,x,x,x}; __builtin_memcpy(p,&r,16); }
static __inline__ __attribute__((always_inline)) __m128 _mm_loadr_ps(const float *p) { __m128 a; __builtin_memcpy(&a,p,16); return __builtin_shufflevector(a,a,3,2,1,0); }
static __inline__ __attribute__((always_inline)) void _mm_storer_ps(float *p, __m128 a) { __m128 r=__builtin_shufflevector(a,a,3,2,1,0); __builtin_memcpy(p,&r,16); }
static __inline__ __attribute__((always_inline)) __m128 _mm_move_ss(__m128 a, __m128 b) { a[0]=b[0];return a; }
#endif
