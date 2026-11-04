/* Exact-width memory transfers; no arithmetic on loaded bits. */
#ifndef _KEEL_SSE2_MEMORY_H
#define _KEEL_SSE2_MEMORY_H
typedef short __keel_i16_u __attribute__((aligned(1)));
typedef int __keel_i32_u __attribute__((aligned(1)));
typedef long long __keel_i64_u __attribute__((aligned(1)));
static __inline__ __attribute__((always_inline)) __m128d _mm_load_sd(const double *p) { return (__m128d){*p, 0}; }
static __inline__ __attribute__((always_inline)) void _mm_store_sd(double *p, __m128d a) { *p = a[0]; }
static __inline__ __attribute__((always_inline)) __m128d _mm_load1_pd(const double *p) { double x = *p; return (__m128d){x,x}; }
static __inline__ __attribute__((always_inline)) __m128d _mm_load_pd1(const double *p) { double x = *p; return (__m128d){x,x}; }
static __inline__ __attribute__((always_inline)) void _mm_store1_pd(double *p, __m128d a) { *(__m128d *)p = __builtin_shufflevector(a,a,0,0); }
static __inline__ __attribute__((always_inline)) void _mm_store_pd1(double *p, __m128d a) { *(__m128d *)p = __builtin_shufflevector(a,a,0,0); }
static __inline__ __attribute__((always_inline)) __m128d _mm_loadr_pd(const double *p) { __m128d a = *(const __m128d *)p; return __builtin_shufflevector(a,a,1,0); }
static __inline__ __attribute__((always_inline)) void _mm_storer_pd(double *p, __m128d a) { *(__m128d *)p = __builtin_shufflevector(a,a,1,0); }
static __inline__ __attribute__((always_inline)) __m128d _mm_move_sd(__m128d a, __m128d b) { a[0]=b[0];return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_loadl_pd(__m128d a,const double*p) { a[0]=*p;return a; }
static __inline__ __attribute__((always_inline)) void _mm_storel_pd(double*p,__m128d a) { *p=a[0]; }
static __inline__ __attribute__((always_inline)) __m128d _mm_loadh_pd(__m128d a,const double*p) { a[1]=*p;return a; }
static __inline__ __attribute__((always_inline)) void _mm_storeh_pd(double*p,__m128d a) { *p=a[1]; }
static __inline__ __attribute__((always_inline)) __m128i _mm_loadu_si16(const void*p) { return (__m128i)(__keel_i16_128){*(const __keel_i16_u*)p,0,0,0,0,0,0,0}; }
static __inline__ __attribute__((always_inline)) void _mm_storeu_si16(void*p,__m128i a) { *(__keel_i16_u*)p=((__keel_i16_128)a)[0]; }
static __inline__ __attribute__((always_inline)) __m128i _mm_loadu_si32(const void*p) { return (__m128i)(__keel_i32_128){*(const __keel_i32_u*)p,0,0,0}; }
static __inline__ __attribute__((always_inline)) void _mm_storeu_si32(void*p,__m128i a) { *(__keel_i32_u*)p=((__keel_i32_128)a)[0]; }
static __inline__ __attribute__((always_inline)) __m128i _mm_loadu_si64(const void*p) { return (__m128i){*(const __keel_i64_u*)p,0}; }
static __inline__ __attribute__((always_inline)) void _mm_storeu_si64(void*p,__m128i a) { *(__keel_i64_u*)p=a[0]; }
static __inline__ __attribute__((always_inline)) __m128i _mm_loadl_epi64(const __m128i_u*p) { return _mm_loadu_si64(p); }
static __inline__ __attribute__((always_inline)) void _mm_storel_epi64(__m128i_u*p,__m128i a) { _mm_storeu_si64(p,a); }
#endif
