#ifndef _KEEL_AVX_QUERIES_H
#define _KEEL_AVX_QUERIES_H
static __inline__ __attribute__((always_inline)) int _mm_testz_ps(__m128 a,__m128 b) { int flags=__builtin_keel_query_avxvtestps128(a,b); return (flags >> 6) & 1; }
static __inline__ __attribute__((always_inline)) int _mm_testc_ps(__m128 a,__m128 b) { int flags=__builtin_keel_query_avxvtestps128(a,b); return flags & 1; }
static __inline__ __attribute__((always_inline)) int _mm_testnzc_ps(__m128 a,__m128 b) { int flags=__builtin_keel_query_avxvtestps128(a,b); return (flags & 65) == 0; }
static __inline__ __attribute__((always_inline)) int _mm_testz_pd(__m128d a,__m128d b) { int flags=__builtin_keel_query_avxvtestpd128(a,b); return (flags >> 6) & 1; }
static __inline__ __attribute__((always_inline)) int _mm_testc_pd(__m128d a,__m128d b) { int flags=__builtin_keel_query_avxvtestpd128(a,b); return flags & 1; }
static __inline__ __attribute__((always_inline)) int _mm_testnzc_pd(__m128d a,__m128d b) { int flags=__builtin_keel_query_avxvtestpd128(a,b); return (flags & 65) == 0; }
static __inline__ __attribute__((always_inline)) int _mm256_testz_si256(__m256i a, __m256i b) { int flags=__builtin_keel_query_avxvptest256((__keel_i8_256)a,(__keel_i8_256)b); return (flags >> 6) & 1; }
static __inline__ __attribute__((always_inline)) int _mm256_testc_si256(__m256i a, __m256i b) { int flags=__builtin_keel_query_avxvptest256((__keel_i8_256)a,(__keel_i8_256)b); return flags & 1; }
static __inline__ __attribute__((always_inline)) int _mm256_testnzc_si256(__m256i a, __m256i b) { int flags=__builtin_keel_query_avxvptest256((__keel_i8_256)a,(__keel_i8_256)b); return (flags & 65) == 0; }
static __inline__ __attribute__((always_inline)) int _mm256_testz_ps(__m256 a,__m256 b) { int flags=__builtin_keel_query_avxvtestps256(a,b); return (flags >> 6) & 1; }
static __inline__ __attribute__((always_inline)) int _mm256_testc_ps(__m256 a,__m256 b) { int flags=__builtin_keel_query_avxvtestps256(a,b); return flags & 1; }
static __inline__ __attribute__((always_inline)) int _mm256_testnzc_ps(__m256 a,__m256 b) { int flags=__builtin_keel_query_avxvtestps256(a,b); return (flags & 65) == 0; }
static __inline__ __attribute__((always_inline)) int _mm256_testz_pd(__m256d a,__m256d b) { int flags=__builtin_keel_query_avxvtestpd256(a,b); return (flags >> 6) & 1; }
static __inline__ __attribute__((always_inline)) int _mm256_testc_pd(__m256d a,__m256d b) { int flags=__builtin_keel_query_avxvtestpd256(a,b); return flags & 1; }
static __inline__ __attribute__((always_inline)) int _mm256_testnzc_pd(__m256d a,__m256d b) { int flags=__builtin_keel_query_avxvtestpd256(a,b); return (flags & 65) == 0; }
#define _mm256_dp_ps(a,b,i) __builtin_keel_query_avxvdpps256((__m256)(a),(__m256)(b),(i))
#endif /* _KEEL_AVX_QUERIES_H */
