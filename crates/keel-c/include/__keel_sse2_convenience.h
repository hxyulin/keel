#ifndef _KEEL_SSE2_CONVENIENCE_H
#define _KEEL_SSE2_CONVENIENCE_H
static __inline__ __attribute__((always_inline)) __m128d _mm_set_pd1(double a) { return (__m128d){a,a}; }
static __inline__ __attribute__((always_inline)) __m128d _mm_set_sd(double a) { return (__m128d){a,0}; }
static __inline__ __attribute__((always_inline)) double _mm_cvtsd_f64(__m128d a) { return (double)a[0]; }
static __inline__ __attribute__((always_inline)) int _mm_cvtsi128_si32(__m128i a) { return (int)a[0]; }
static __inline__ __attribute__((always_inline)) long long _mm_cvtsi128_si64(__m128i a) { return (long long)a[0]; }
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtsi32_si128(int a) { return (__m128i){(long long)(unsigned int)a,0}; }
static __inline__ __attribute__((always_inline)) __m128i _mm_cvtsi64_si128(long long a) { return (__m128i){a,0}; }
static __inline__ __attribute__((always_inline)) __m128i _mm_move_epi64(__m128i a) { return (__m128i){a[0],0}; }
static __inline__ __attribute__((always_inline)) __m128d _mm_undefined_pd(void) { return (__m128d){0}; }
static __inline__ __attribute__((always_inline)) __m128i _mm_undefined_si128(void) { return (__m128i){0}; }
static __inline__ __attribute__((always_inline)) __m128i _mm_cmplt_epi8(__m128i a,__m128i b) { return _mm_cmpgt_epi8(b,a); }
static __inline__ __attribute__((always_inline)) __m128i _mm_cmplt_epi16(__m128i a,__m128i b) { return _mm_cmpgt_epi16(b,a); }
static __inline__ __attribute__((always_inline)) __m128i _mm_cmplt_epi32(__m128i a,__m128i b) { return _mm_cmpgt_epi32(b,a); }
#endif /* _KEEL_SSE2_CONVENIENCE_H */
