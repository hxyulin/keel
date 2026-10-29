/* Exact-width memory transfers; no arithmetic on loaded bits. */
#ifndef _KEEL_SSE2_MEMORY_H
#define _KEEL_SSE2_MEMORY_H
static __inline__ __attribute__((always_inline)) __m128d _mm_load_sd(const double *p) { __m128d r={0}; double x; __builtin_memcpy(&x,p,8); r[0]=x; return r; }
static __inline__ __attribute__((always_inline)) void _mm_store_sd(double *p, __m128d a) { double x=a[0]; __builtin_memcpy(p,&x,8); }
static __inline__ __attribute__((always_inline)) __m128d _mm_load1_pd(const double *p) { double x; __builtin_memcpy(&x,p,8); return (__m128d){x,x}; }
static __inline__ __attribute__((always_inline)) __m128d _mm_load_pd1(const double *p) { double x; __builtin_memcpy(&x,p,8); return (__m128d){x,x}; }
static __inline__ __attribute__((always_inline)) void _mm_store1_pd(double *p, __m128d a) { double x=a[0]; __m128d r={x,x}; __builtin_memcpy(p,&r,16); }
static __inline__ __attribute__((always_inline)) void _mm_store_pd1(double *p, __m128d a) { double x=a[0]; __m128d r={x,x}; __builtin_memcpy(p,&r,16); }
static __inline__ __attribute__((always_inline)) __m128d _mm_loadr_pd(const double *p) { __m128d a; __builtin_memcpy(&a,p,16); return __builtin_shufflevector(a,a,1,0); }
static __inline__ __attribute__((always_inline)) void _mm_storer_pd(double *p, __m128d a) { __m128d r=__builtin_shufflevector(a,a,1,0); __builtin_memcpy(p,&r,16); }
static __inline__ __attribute__((always_inline)) __m128d _mm_move_sd(__m128d a, __m128d b) { a[0]=b[0];return a; }
static __inline__ __attribute__((always_inline)) __m128d _mm_loadl_pd(__m128d a,const double*p) { double x;__builtin_memcpy(&x,p,8);a[0]=x;return a; }
static __inline__ __attribute__((always_inline)) void _mm_storel_pd(double*p,__m128d a) { double x=a[0];__builtin_memcpy(p,&x,8); }
static __inline__ __attribute__((always_inline)) __m128d _mm_loadh_pd(__m128d a,const double*p) { double x;__builtin_memcpy(&x,p,8);a[1]=x;return a; }
static __inline__ __attribute__((always_inline)) void _mm_storeh_pd(double*p,__m128d a) { double x=a[1];__builtin_memcpy(p,&x,8); }
static __inline__ __attribute__((always_inline)) __m128i _mm_loadu_si16(const void*p) { __m128i a={0};__builtin_memcpy(&a,p,2);return a; }
static __inline__ __attribute__((always_inline)) void _mm_storeu_si16(void*p,__m128i a) { __builtin_memcpy(p,&a,2); }
static __inline__ __attribute__((always_inline)) __m128i _mm_loadu_si32(const void*p) { __m128i a={0};__builtin_memcpy(&a,p,4);return a; }
static __inline__ __attribute__((always_inline)) void _mm_storeu_si32(void*p,__m128i a) { __builtin_memcpy(p,&a,4); }
static __inline__ __attribute__((always_inline)) __m128i _mm_loadu_si64(const void*p) { __m128i a={0};__builtin_memcpy(&a,p,8);return a; }
static __inline__ __attribute__((always_inline)) void _mm_storeu_si64(void*p,__m128i a) { __builtin_memcpy(p,&a,8); }
static __inline__ __attribute__((always_inline)) __m128i _mm_loadl_epi64(const __m128i_u*p) { return _mm_loadu_si64(p); }
static __inline__ __attribute__((always_inline)) void _mm_storel_epi64(__m128i_u*p,__m128i a) { _mm_storeu_si64(p,a); }
#endif
